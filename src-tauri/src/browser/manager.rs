//! 自动化浏览器：启动 / 复用本机 Edge 或 Chrome（独立用户目录，登录会话只保存在本机）、标签页、窗口显示、Cookie。
//!
//! 修复 v3.4：
//! - 窗口显示改为"可见标签集合"，不再用全局计数（登录超时后窗口永远留在屏幕上）。
//! - 退出单个平台只删除该平台域名下的 Cookie（不再清空全部再写回）。
//! - 不再使用 `--remote-allow-origins=*`：本程序的 websocket 客户端不带 Origin 头，网页无法接入调试端口。

use super::cdp::Conn;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

pub const OFFSCREEN: (i64, i64) = (-32000, -32000);
const PERSIST_DAYS: f64 = 30.0;

#[derive(Default)]
struct Inner {
    port: Option<u16>,
    child: Option<Child>,
    conn: Option<Arc<Conn>>,
}

pub struct Browser {
    inner: Mutex<Inner>,
    profile: PathBuf,
    http: reqwest::Client,
    background: RwLock<bool>,
    headless: bool,
    lang: RwLock<String>,
    sem: RwLock<Arc<Semaphore>>,
    last_nav: StdMutex<HashMap<String, Instant>>,
    visible: StdMutex<HashSet<String>>,
    fail: StdMutex<Option<(Instant, String)>>,
    executable: RwLock<Option<String>>,
    wiping: AtomicBool,
}

/// 标签守卫：搜索被取消或超时导致 future 被丢弃时，仍然关闭标签（修复标签泄漏、窗口留在屏幕上）。
pub struct TabGuard {
    browser: Arc<Browser>,
    id: String,
    armed: bool,
}

impl TabGuard {
    pub fn new(browser: &Arc<Browser>, id: &str) -> Self {
        TabGuard { browser: browser.clone(), id: id.to_string(), armed: true }
    }
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TabGuard {
    fn drop(&mut self) {
        if self.armed {
            let b = self.browser.clone();
            let id = std::mem::take(&mut self.id);
            tauri::async_runtime::spawn(async move { b.close_target(&id).await });
        }
    }
}

pub struct Tab {
    pub id: String,
    conn: Arc<Conn>,
}

impl Tab {
    pub async fn eval(&self, expr: &str, timeout: Duration) -> Result<Value, String> {
        let res = self
            .conn
            .send("Runtime.evaluate", json!({"expression": expr, "returnByValue": true, "awaitPromise": true}), timeout)
            .await?;
        if let Some(ex) = res.get("exceptionDetails") {
            let text = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
                .or_else(|| ex.get("text").and_then(|t| t.as_str())).unwrap_or("页面脚本执行失败");
            return Err(format!("页面脚本执行失败：{}", text.chars().take(160).collect::<String>()));
        }
        Ok(res.get("result").and_then(|r| r.get("value")).cloned().unwrap_or(Value::Null))
    }

    /// 等待页面离开 about:blank 且不再 loading（最多 timeout，不报错）。
    pub async fn ready(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(v) = self.eval("[location.href, document.readyState]", Duration::from_secs(5)).await {
                let href = v.get(0).and_then(|x| x.as_str()).unwrap_or("");
                let st = v.get(1).and_then(|x| x.as_str()).unwrap_or("loading");
                if href != "about:blank" && st != "loading" {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    pub async fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        self.conn.send(method, params, Duration::from_secs(20)).await
    }
}

pub fn find_executable() -> Option<String> {
    if let Ok(p) = std::env::var("HAWWWKER_BROWSER") {
        if Path::new(&p).exists() {
            return Some(p);
        }
    }
    #[cfg(target_os = "windows")]
    {
        for base in ["PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"] {
            if let Ok(b) = std::env::var(base) {
                for rel in ["Microsoft\\Edge\\Application\\msedge.exe", "Google\\Chrome\\Application\\chrome.exe"] {
                    let p = Path::new(&b).join(rel);
                    if p.exists() {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for p in ["/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"] {
            if Path::new(p).exists() {
                return Some(p.into());
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    for name in ["msedge", "microsoft-edge", "google-chrome", "google-chrome-stable", "chrome", "chromium", "chromium-browser"] {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok().map(|a| a.port())
}

pub fn domain_match(cookie_domain: &str, suffixes: &[String]) -> bool {
    let d = cookie_domain.trim_start_matches('.').to_lowercase();
    suffixes.iter().any(|s| d == *s || d.ends_with(&format!(".{s}")))
}

impl Browser {
    pub fn new(profile: PathBuf) -> Self {
        let headless = std::env::var("HAWWWKER_HEADLESS").map(|v| v == "1").unwrap_or(false);
        Browser {
            inner: Mutex::new(Inner::default()),
            profile,
            http: reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(10)).build().unwrap_or_default(),
            background: RwLock::new(true),
            headless,
            lang: RwLock::new("zh-CN".into()),
            sem: RwLock::new(Arc::new(Semaphore::new(3))),
            last_nav: StdMutex::new(HashMap::new()),
            visible: StdMutex::new(HashSet::new()),
            fail: StdMutex::new(None),
            executable: RwLock::new(None),
            wiping: AtomicBool::new(false),
        }
    }

    pub fn configure(&self, background: bool, concurrency: u32) {
        if let Ok(mut b) = self.background.write() {
            *b = background;
        }
        if let Ok(mut s) = self.sem.write() {
            *s = Arc::new(Semaphore::new(concurrency.clamp(1, 4) as usize));
        }
    }

    pub fn profile_dir(&self) -> &Path {
        &self.profile
    }

    pub fn executable(&self) -> Option<String> {
        if let Some(e) = self.executable.read().ok().and_then(|e| e.clone()) {
            return Some(e);
        }
        let found = find_executable();
        if let Ok(mut w) = self.executable.write() {
            *w = found.clone();
        }
        found
    }

    /// 平台并行名额（不同平台同时打开的标签数）。
    pub async fn slot(&self) -> Option<OwnedSemaphorePermit> {
        let sem = self.sem.read().ok()?.clone();
        sem.acquire_owned().await.ok()
    }

    async fn port(&self) -> Option<u16> {
        self.inner.lock().await.port
    }

    async fn http_json(&self, port: u16, method: reqwest::Method, path: &str) -> Result<Value, String> {
        let resp = self
            .http
            .request(method, format!("http://127.0.0.1:{port}{path}"))
            .send()
            .await
            .map_err(|e| format!("DevTools 请求失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("DevTools HTTP {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    pub async fn alive(&self) -> bool {
        match self.port().await {
            Some(p) => self.http_json(p, reqwest::Method::GET, "/json/version").await.is_ok(),
            None => false,
        }
    }

    fn prepare_profile(&self) {
        let dir = self.profile.join("Default");
        let _ = std::fs::create_dir_all(&dir);
        let pref = dir.join("Preferences");
        let mut data: Value = std::fs::read_to_string(&pref).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}));
        if !data.is_object() {
            data = json!({});
        }
        data["session"]["restore_on_startup"] = json!(1);
        data["profile"]["exit_type"] = json!("Normal");
        data["profile"]["exited_cleanly"] = json!(true);
        let _ = std::fs::write(&pref, data.to_string());
    }

    /// 确保浏览器已启动并返回浏览器级连接。启动失败 60 秒内快速失败。
    pub async fn ensure(&self) -> Result<Arc<Conn>, String> {
        if self.wiping.load(Ordering::SeqCst) {
            return Err("正在清除登录数据，请稍候".into());
        }
        let mut inner = self.inner.lock().await;
        if let (Some(port), Some(conn)) = (inner.port, inner.conn.clone()) {
            if !conn.is_closed() && self.http_json(port, reqwest::Method::GET, "/json/version").await.is_ok() {
                return Ok(conn);
            }
        }
        if let Some((t, msg)) = self.fail.lock().ok().and_then(|f| f.clone()) {
            if t.elapsed() < Duration::from_secs(60) {
                return Err(msg);
            }
        }
        // 已有端口但连接断开：尝试重连
        if let Some(port) = inner.port {
            if let Ok(info) = self.http_json(port, reqwest::Method::GET, "/json/version").await {
                if let Some(ws) = info.get("webSocketDebuggerUrl").and_then(|w| w.as_str()) {
                    if let Ok(conn) = Conn::connect(ws).await {
                        inner.conn = Some(conn.clone());
                        return Ok(conn);
                    }
                }
            }
        }
        // 上次异常退出遗留的浏览器仍占用用户目录：按 DevToolsActivePort 重新接入，而不是再启动一个（否则会被转交给旧实例而超时）
        if let Some(port) = std::fs::read_to_string(self.profile.join("DevToolsActivePort")).ok().and_then(|s| s.lines().next().and_then(|l| l.trim().parse::<u16>().ok())) {
            if let Ok(info) = self.http_json(port, reqwest::Method::GET, "/json/version").await {
                if let Some(ws) = info.get("webSocketDebuggerUrl").and_then(|w| w.as_str()) {
                    if let Ok(conn) = Conn::connect(ws).await {
                        log::info!("reattached to running browser on port {port}");
                        inner.port = Some(port);
                        inner.conn = Some(conn.clone());
                        return Ok(conn);
                    }
                }
            }
        }
        match self.launch(&mut inner).await {
            Ok(c) => {
                if let Ok(mut f) = self.fail.lock() {
                    *f = None;
                }
                Ok(c)
            }
            Err(e) => {
                if let Ok(mut f) = self.fail.lock() {
                    *f = Some((Instant::now(), e.clone()));
                }
                Err(e)
            }
        }
    }

    async fn launch(&self, inner: &mut Inner) -> Result<Arc<Conn>, String> {
        let exe = self.executable().ok_or("没有找到 Edge 或 Chrome 浏览器，请先安装其中之一")?;
        std::fs::create_dir_all(&self.profile).map_err(|e| format!("无法创建浏览器目录：{e}"))?;
        self.prepare_profile();
        let port = free_port().ok_or("没有可用的本地端口")?;
        let lang = self.lang.read().map(|l| l.clone()).unwrap_or_else(|_| "zh-CN".into());
        let mut args = vec![
            format!("--remote-debugging-port={port}"),
            "--remote-debugging-address=127.0.0.1".into(),
            format!("--user-data-dir={}", self.profile.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--disable-session-crashed-bubble".into(),
            "--hide-crash-restore-bubble".into(),
            "--window-size=1280,900".into(),
            // 窗口在屏幕外时不要被系统判定为"被遮挡"而降频（否则懒加载与无限滚动不触发）
            "--disable-backgrounding-occluded-windows".into(),
            "--disable-renderer-backgrounding".into(),
            "--disable-background-timer-throttling".into(),
            "--disable-features=CalculateNativeWinOcclusion".into(),
            format!("--lang={lang}"),
        ];
        let background = self.background.read().map(|b| *b).unwrap_or(true);
        if self.headless {
            args.push("--headless=new".into());
            args.push("--no-sandbox".into());
        } else if background {
            args.push(format!("--window-position={},{}", OFFSCREEN.0, OFFSCREEN.1));
        }
        args.push("about:blank".into());
        log::info!("launch browser {exe} (background={background})");
        let mut cmd = Command::new(&exe);
        cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let child = cmd.spawn().map_err(|e| format!("启动浏览器失败：{e}"))?;
        inner.child = Some(child);
        inner.port = Some(port);
        for _ in 0..100 {
            if let Ok(info) = self.http_json(port, reqwest::Method::GET, "/json/version").await {
                if let Some(ws) = info.get("webSocketDebuggerUrl").and_then(|w| w.as_str()) {
                    let conn = Conn::connect(ws).await?;
                    inner.conn = Some(conn.clone());
                    if let Ok(mut v) = self.visible.lock() {
                        v.clear();
                    }
                    return Ok(conn);
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        if let Some(mut c) = inner.child.take() {
            let _ = c.kill();
        }
        inner.port = None;
        Err("浏览器启动超时（20 秒），请关闭所有 Edge/Chrome 窗口后重试".into())
    }

    /* ------------------------------------------------------------ 标签页 */
    async fn pace(&self, url: &str, gap: Duration) {
        let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").to_string();
        let parts: Vec<&str> = host.split('.').collect();
        let site = if parts.len() >= 2 { parts[parts.len() - 2..].join(".") } else { host.clone() };
        let wait = {
            let mut m = match self.last_nav.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let now = Instant::now();
            let next_allowed = m.get(&site).map(|t| *t + gap).unwrap_or(now);
            let start = next_allowed.max(now);
            m.insert(site, start);
            start - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    async fn create_target(&self, url: &str) -> Result<Value, String> {
        self.ensure().await?;
        let port = self.port().await.ok_or("浏览器未启动")?;
        let enc = urlencoding::encode(url).into_owned();
        match self.http_json(port, reqwest::Method::PUT, &format!("/json/new?{enc}")).await {
            Ok(v) if v.get("id").is_some() => Ok(v),
            _ => self.http_json(port, reqwest::Method::GET, &format!("/json/new?{enc}")).await,
        }
    }

    pub async fn new_tab(&self, url: &str, throttle: bool) -> Result<Tab, String> {
        if throttle {
            self.pace(url, Duration::from_millis(3000)).await;
        }
        let info = self.create_target(url).await?;
        let id = info.get("id").and_then(|x| x.as_str()).ok_or("无法创建标签页")?.to_string();
        let ws = info.get("webSocketDebuggerUrl").and_then(|x| x.as_str()).ok_or("无法接入标签页")?.to_string();
        let conn = match Conn::connect(&ws).await {
            Ok(c) => c,
            Err(e) => {
                self.close_target(&id).await;
                return Err(e);
            }
        };
        let tab = Tab { id, conn };
        tab.ready(Duration::from_secs(12)).await;
        Ok(tab)
    }

    /// 打开官网登录页：普通标签页，不接入、不执行脚本，只把窗口移到屏幕上。
    pub async fn open_plain(&self, url: &str) -> Result<String, String> {
        let info = self.create_target(url).await?;
        let id = info.get("id").and_then(|x| x.as_str()).ok_or("无法创建标签页")?.to_string();
        self.show(&id).await;
        Ok(id)
    }

    pub async fn close_tab(&self, tab: Tab) {
        tab.conn.close();
        self.hide(&tab.id).await;
        self.close_target(&tab.id).await;
    }

    pub async fn close_target(&self, id: &str) {
        self.hide(id).await;
        if let Some(port) = self.port().await {
            let _ = self.http_json(port, reqwest::Method::GET, &format!("/json/close/{id}")).await;
        }
    }

    /// 只读列出标签页（不接入页面）：[(id, url)]
    pub async fn targets(&self) -> Vec<(String, String)> {
        let Some(port) = self.port().await else { return vec![] };
        match self.http_json(port, reqwest::Method::GET, "/json/list").await {
            Ok(Value::Array(a)) => a
                .into_iter()
                .filter(|t| t.get("type").and_then(|x| x.as_str()) == Some("page"))
                .filter_map(|t| Some((t.get("id")?.as_str()?.to_string(), t.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string())))
                .collect(),
            _ => vec![],
        }
    }

    /* ------------------------------------------------------------ 窗口 */
    async fn set_window(&self, target_id: &str, visible: bool) {
        if self.headless {
            return;
        }
        let conn = match self.inner.lock().await.conn.clone() {
            Some(c) if !c.is_closed() => c,
            _ => return,
        };
        let t = Duration::from_secs(5);
        let Ok(win) = conn.send("Browser.getWindowForTarget", json!({"targetId": target_id}), t).await else { return };
        let Some(wid) = win.get("windowId").cloned() else { return };
        let _ = conn.send("Browser.setWindowBounds", json!({"windowId": wid, "bounds": {"windowState": "normal"}}), t).await;
        if visible {
            let _ = conn.send("Browser.setWindowBounds", json!({"windowId": wid, "bounds": {"left": 80, "top": 60, "width": 1280, "height": 900}}), t).await;
            let _ = conn.send("Target.activateTarget", json!({"targetId": target_id}), t).await;
        } else {
            let _ = conn.send("Browser.setWindowBounds", json!({"windowId": wid, "bounds": {"left": OFFSCREEN.0, "top": OFFSCREEN.1}}), t).await;
        }
    }

    pub async fn show(&self, target_id: &str) {
        if let Ok(mut v) = self.visible.lock() {
            v.insert(target_id.to_string());
        }
        self.set_window(target_id, true).await;
    }

    pub async fn hide(&self, target_id: &str) {
        let now_empty = match self.visible.lock() {
            Ok(mut v) => v.remove(target_id) && v.is_empty(),
            Err(_) => false,
        };
        let background = self.background.read().map(|b| *b).unwrap_or(true);
        if now_empty && background {
            // 窗口可能已随标签关闭；挑一个仍存在的标签来定位窗口
            let any = self.targets().await.into_iter().map(|(id, _)| id).find(|id| id != target_id);
            if let Some(id) = any {
                self.set_window(&id, false).await;
            }
        }
    }

    /// 把自动化浏览器窗口显示出来（用户想看看它在做什么）。
    pub async fn show_any(&self) -> Result<(), String> {
        self.ensure().await?;
        let id = match self.targets().await.into_iter().next() {
            Some((id, _)) => id,
            None => self.create_target("about:blank").await?.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        };
        self.show(&id).await;
        Ok(())
    }

    /* ------------------------------------------------------------ Cookie（只在本机浏览器内部处理） */
    pub async fn cookies(&self) -> Result<Vec<Value>, String> {
        let conn = self.ensure().await?;
        let r = conn.send("Storage.getCookies", json!({}), Duration::from_secs(15)).await?;
        Ok(r.get("cookies").and_then(|c| c.as_array()).cloned().unwrap_or_default())
    }

    /// 把平台域名下的会话 Cookie 转存为持久 Cookie（关闭浏览器也不丢登录）。
    pub async fn persist_session_cookies(&self, suffixes: &[String]) -> Result<usize, String> {
        if suffixes.is_empty() {
            return Ok(0);
        }
        let cookies = self.cookies().await?;
        let expiry = chrono::Utc::now().timestamp() as f64 + PERSIST_DAYS * 86400.0;
        let mut fixed = vec![];
        for c in cookies.iter() {
            let dom = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
            if !domain_match(dom, suffixes) || !c.get("session").and_then(|s| s.as_bool()).unwrap_or(false) {
                continue;
            }
            let secure = c.get("secure").and_then(|s| s.as_bool()).unwrap_or(false);
            let path = c.get("path").and_then(|s| s.as_str()).unwrap_or("/");
            let mut p = json!({
                "name": c.get("name"), "value": c.get("value"), "path": path, "secure": secure,
                "httpOnly": c.get("httpOnly"), "expires": expiry,
            });
            if let Some(ss) = c.get("sameSite") {
                p["sameSite"] = ss.clone();
            }
            if let Some(pk) = c.get("partitionKey") {
                p["partitionKey"] = pk.clone();
            }
            if dom.starts_with('.') {
                p["domain"] = json!(dom);
            } else {
                p["url"] = json!(format!("{}://{}{}", if secure { "https" } else { "http" }, dom, path));
            }
            fixed.push(p);
        }
        if !fixed.is_empty() {
            let conn = self.ensure().await?;
            conn.send("Storage.setCookies", json!({"cookies": fixed}), Duration::from_secs(15)).await?;
        }
        Ok(fixed.len())
    }

    /// 只删除这些域名下的 Cookie（逐条 Network.deleteCookies），其他平台的登录不受影响。
    pub async fn delete_site_cookies(&self, suffixes: &[String]) -> Result<usize, String> {
        let cookies = self.cookies().await?;
        let targets: Vec<&Value> = cookies
            .iter()
            .filter(|c| domain_match(c.get("domain").and_then(|d| d.as_str()).unwrap_or(""), suffixes))
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        let tab = self.new_tab("about:blank", false).await?;
        let mut n = 0;
        for c in targets.iter() {
            let mut p = json!({
                "name": c.get("name"), "domain": c.get("domain"), "path": c.get("path").and_then(|p| p.as_str()).unwrap_or("/"),
            });
            if let Some(pk) = c.get("partitionKey") {
                p["partitionKey"] = pk.clone();
            }
            if tab.send("Network.deleteCookies", p).await.is_ok() {
                n += 1;
            }
        }
        self.close_tab(tab).await;
        Ok(n)
    }

    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(conn) = inner.conn.take() {
            let _ = conn.send("Browser.close", json!({}), Duration::from_secs(5)).await;
            conn.close();
        }
        if let Some(mut child) = inner.child.take() {
            for _ in 0..50 {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        inner.port = None;
        if let Ok(mut v) = self.visible.lock() {
            v.clear();
        }
    }

    pub async fn wipe_profile(&self) -> Result<(), String> {
        self.wiping.store(true, Ordering::SeqCst);
        let res = async {
            self.shutdown().await;
            if !self.profile.exists() {
                return Ok(());
            }
            // Windows 上浏览器子进程退出需要一点时间，文件可能仍被占用：重试，最后改名留待下次启动清理
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if std::fs::remove_dir_all(&self.profile).is_ok() || !self.profile.exists() {
                    return Ok(());
                }
            }
            let trash = self.profile.with_file_name(format!("browser-profile-trash-{}", chrono::Utc::now().timestamp()));
            std::fs::rename(&self.profile, &trash).map_err(|e| format!("删除浏览器目录失败（文件被占用）：{e}"))?;
            let _ = std::fs::remove_dir_all(&trash);
            Ok(())
        }
        .await;
        self.wiping.store(false, Ordering::SeqCst);
        res
    }

    /// 启动时清理上次未删干净的旧目录。
    pub fn cleanup_trash(&self) {
        if let (Some(parent), Some(_)) = (self.profile.parent(), self.profile.file_name()) {
            if let Ok(rd) = std::fs::read_dir(parent) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with("browser-profile-trash-") {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        }
    }
}
