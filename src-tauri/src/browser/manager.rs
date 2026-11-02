//! 自动化浏览器：启动 / 复用本机 Edge 或 Chrome（独立用户目录，登录会话只保存在本机）、标签页、窗口显示、Cookie。
//!
//! v26.02：
//! - 默认通过 `--remote-debugging-pipe` 通信，不开端口；管道启动失败时才回退到 127.0.0.1 随机端口。
//! - 所有标签页通过浏览器级连接上的 flatten session 访问（Target.createTarget / attachToTarget），不再依赖 HTTP 端点。
//! - 浏览器与本程序同生共死（Windows 作业对象 / Linux PDEATHSIG），不会遗留进程。
//!
//! 沿用：窗口显示用"可见标签集合"；退出单个平台只删除该平台域名下的 Cookie；登录页不接入、不执行脚本。

use super::cdp::Conn;
use super::launch::{self, Proc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

pub const OFFSCREEN: (i64, i64) = (-32000, -32000);

/// 浏览器启动后的钩子：输入当前全部 Cookie，返回需要写回的 Cookie（`Storage.setCookies` 参数）。
/// v26.03 用于从加密会话备份恢复丢失的登录（见 vault.rs）。
pub type LaunchHook = Arc<dyn Fn(&[Value]) -> Vec<Value> + Send + Sync>;
const PERSIST_DAYS: f64 = 30.0;
const T: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Inner {
    proc: Option<Proc>,
    conn: Option<Arc<Conn>>,
    port: Option<u16>,
}

pub struct Browser {
    inner: Mutex<Inner>,
    profile: PathBuf,
    background: RwLock<bool>,
    headless: bool,
    lang: RwLock<String>,
    sem: RwLock<Arc<Semaphore>>,
    last_nav: StdMutex<HashMap<String, Instant>>,
    visible: StdMutex<HashSet<String>>,
    fail: StdMutex<Option<(Instant, String)>>,
    executable: RwLock<Option<String>>,
    wiping: AtomicBool,
    transport: RwLock<&'static str>,
    launch_hook: StdMutex<Option<LaunchHook>>,
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

/// new_tab 内部使用：标签已创建、尚未交给调用方时被丢弃 → 关闭标签。
struct PendingTarget {
    conn: Arc<Conn>,
    id: String,
    armed: bool,
}

impl Drop for PendingTarget {
    fn drop(&mut self) {
        if self.armed {
            let c = self.conn.clone();
            let id = std::mem::take(&mut self.id);
            tauri::async_runtime::spawn(async move {
                let _ = c.send("Target.closeTarget", json!({"targetId": id}), Duration::from_secs(5)).await;
            });
        }
    }
}

pub struct Tab {
    pub id: String,
    session: String,
    conn: Arc<Conn>,
}

impl Tab {
    pub async fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        self.conn.send_to(Some(&self.session), method, params, Duration::from_secs(20)).await
    }

    pub async fn eval(&self, expr: &str, timeout: Duration) -> Result<Value, String> {
        let res = self
            .conn
            .send_to(Some(&self.session), "Runtime.evaluate", json!({"expression": expr, "returnByValue": true, "awaitPromise": true}), timeout)
            .await?;
        if let Some(ex) = res.get("exceptionDetails") {
            let text = ex
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .or_else(|| ex.get("text").and_then(|t| t.as_str()))
                .unwrap_or("页面脚本执行失败");
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

async fn http_json(port: u16, path: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().map_err(|e| e.to_string())?;
    let resp = client.get(format!("http://127.0.0.1:{port}{path}")).send().await.map_err(|e| e.to_string())?;
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

impl Browser {
    pub fn new(profile: PathBuf) -> Self {
        let headless = std::env::var("HAWWWKER_HEADLESS").map(|v| v == "1").unwrap_or(false);
        Browser {
            inner: Mutex::new(Inner::default()),
            profile,
            background: RwLock::new(true),
            headless,
            lang: RwLock::new("zh-CN".into()),
            sem: RwLock::new(Arc::new(Semaphore::new(3))),
            last_nav: StdMutex::new(HashMap::new()),
            visible: StdMutex::new(HashSet::new()),
            fail: StdMutex::new(None),
            executable: RwLock::new(None),
            wiping: AtomicBool::new(false),
            transport: RwLock::new("pipe"),
            launch_hook: StdMutex::new(None),
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

    /// 应用数据目录（浏览器目录的上一级），会话备份密钥放在这里。
    pub fn data_dir(&self) -> Option<PathBuf> {
        self.profile.parent().map(|p| p.to_path_buf())
    }

    /// 设置启动钩子（每次启动新浏览器进程后执行一次）。
    pub fn set_launch_hook(&self, hook: LaunchHook) {
        if let Ok(mut h) = self.launch_hook.lock() {
            *h = Some(hook);
        }
    }

    async fn after_launch(&self, conn: &Arc<Conn>) {
        let hook = self.launch_hook.lock().ok().and_then(|h| h.clone());
        let Some(hook) = hook else { return };
        let Ok(r) = conn.send("Storage.getCookies", json!({}), T).await else { return };
        let current = r.get("cookies").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        let params = hook(&current);
        if !params.is_empty() {
            match conn.send("Storage.setCookies", json!({"cookies": params}), T).await {
                Ok(_) => log::info!("restored {} cookies from session vault after launch", params.len()),
                Err(e) => log::warn!("restore cookies after launch failed: {e}"),
            }
        }
    }

    /// 当前通信方式：pipe（管道，无端口）或 port（回退）。
    pub fn transport(&self) -> &'static str {
        self.transport.read().map(|t| *t).unwrap_or("pipe")
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

    pub async fn alive(&self) -> bool {
        let conn = self.inner.lock().await.conn.clone();
        match conn {
            Some(c) if !c.is_closed() => c.send("Browser.getVersion", json!({}), Duration::from_secs(5)).await.is_ok(),
            _ => false,
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
        if let Some(conn) = inner.conn.clone() {
            if !conn.is_closed() && conn.send("Browser.getVersion", json!({}), Duration::from_secs(5)).await.is_ok() {
                return Ok(conn);
            }
        }
        if let Some((t, msg)) = self.fail.lock().ok().and_then(|f| f.clone()) {
            if t.elapsed() < Duration::from_secs(60) {
                return Err(msg);
            }
        }
        if let Some(mut p) = inner.proc.take() {
            p.kill();
        }
        inner.conn = None;
        inner.port = None;
        // 端口模式下遗留的浏览器（旧版本 / 回退模式）：按 DevToolsActivePort 重新接入
        if let Some(conn) = self.reattach_port(&mut inner).await {
            return Ok(conn);
        }
        let res = match self.launch(&mut inner, std::env::var("HAWWWKER_CDP").map(|v| v != "port").unwrap_or(true)).await {
            Ok(c) => Ok(c),
            Err(e) => {
                log::warn!("pipe launch failed ({e}), falling back to port mode");
                if let Some(mut p) = inner.proc.take() {
                    p.kill();
                }
                self.launch(&mut inner, false).await
            }
        };
        match &res {
            Ok(conn) => {
                if let Ok(mut f) = self.fail.lock() {
                    *f = None;
                }
                self.after_launch(conn).await;
            }
            Err(e) => {
                if let Ok(mut f) = self.fail.lock() {
                    *f = Some((Instant::now(), e.clone()));
                }
            }
        }
        res
    }

    async fn reattach_port(&self, inner: &mut Inner) -> Option<Arc<Conn>> {
        let port = std::fs::read_to_string(self.profile.join("DevToolsActivePort")).ok()?.lines().next()?.trim().parse::<u16>().ok()?;
        let info = http_json(port, "/json/version").await.ok()?;
        let ws = info.get("webSocketDebuggerUrl")?.as_str()?;
        let conn = Conn::connect_ws(ws).await.ok()?;
        log::info!("reattached to running browser on port {port}");
        inner.port = Some(port);
        inner.conn = Some(conn.clone());
        if let Ok(mut t) = self.transport.write() {
            *t = "port";
        }
        Some(conn)
    }

    async fn launch(&self, inner: &mut Inner, pipe: bool) -> Result<Arc<Conn>, String> {
        let exe = self.executable().ok_or("没有找到 Edge 或 Chrome 浏览器，请先安装其中之一")?;
        std::fs::create_dir_all(&self.profile).map_err(|e| format!("无法创建浏览器目录：{e}"))?;
        self.prepare_profile();
        let _ = std::fs::remove_file(self.profile.join("DevToolsActivePort"));
        let lang = self.lang.read().map(|l| l.clone()).unwrap_or_else(|_| "zh-CN".into());
        let port = if pipe { None } else { Some(free_port().ok_or("没有可用的本地端口")?) };
        let mut args = vec![
            format!("--user-data-dir={}", self.profile.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--disable-session-crashed-bubble".into(),
            "--hide-crash-restore-bubble".into(),
            "--window-size=1280,900".into(),
            format!("--lang={lang}"),
            // 窗口在屏幕外时不要被系统判定为"被遮挡"而降频（否则懒加载与无限滚动不触发）
            "--disable-backgrounding-occluded-windows".into(),
            "--disable-renderer-backgrounding".into(),
            "--disable-background-timer-throttling".into(),
            "--disable-features=CalculateNativeWinOcclusion".into(),
        ];
        match port {
            None => args.push("--remote-debugging-pipe".into()),
            Some(p) => {
                args.push(format!("--remote-debugging-port={p}"));
                args.push("--remote-debugging-address=127.0.0.1".into());
            }
        }
        let background = self.background.read().map(|b| *b).unwrap_or(true);
        if self.headless {
            args.push("--headless=new".into());
            args.push("--no-sandbox".into());
        } else if background {
            args.push(format!("--window-position={},{}", OFFSCREEN.0, OFFSCREEN.1));
        }
        args.push("about:blank".into());
        log::info!("launch browser {exe} (transport={}, background={background})", if pipe { "pipe" } else { "port" });
        let (proc, pipe_conn) = launch::spawn(&exe, &args, pipe)?;
        inner.proc = Some(proc);
        let deadline = Instant::now() + Duration::from_secs(20);
        let conn = match pipe_conn {
            Some(conn) => {
                // 管道模式：等待浏览器在管道上响应
                loop {
                    if conn.send("Browser.getVersion", json!({}), Duration::from_secs(3)).await.is_ok() {
                        break conn;
                    }
                    let exited = inner.proc.as_mut().map(|p| p.exited()).unwrap_or(true);
                    if exited || conn.is_closed() || Instant::now() > deadline {
                        return Err(if exited {
                            "浏览器启动后立即退出（可能已有同一目录的浏览器在运行）".into()
                        } else {
                            "浏览器没有在管道上响应".into()
                        });
                    }
                }
            }
            None => {
                let port = port.unwrap_or_default();
                loop {
                    if let Ok(info) = http_json(port, "/json/version").await {
                        if let Some(ws) = info.get("webSocketDebuggerUrl").and_then(|w| w.as_str()) {
                            break Conn::connect_ws(ws).await?;
                        }
                    }
                    if Instant::now() > deadline {
                        if let Some(mut p) = inner.proc.take() {
                            p.kill();
                        }
                        return Err("浏览器启动超时（20 秒），请关闭所有 Edge/Chrome 窗口后重试".into());
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        };
        inner.conn = Some(conn.clone());
        inner.port = port;
        if let Ok(mut t) = self.transport.write() {
            *t = if pipe { "pipe" } else { "port" };
        }
        if let Ok(mut v) = self.visible.lock() {
            v.clear();
        }
        Ok(conn)
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

    async fn create_target(&self, url: &str) -> Result<String, String> {
        let conn = self.ensure().await?;
        let r = conn.send("Target.createTarget", json!({"url": url}), T).await?;
        r.get("targetId").and_then(|x| x.as_str()).map(|s| s.to_string()).ok_or_else(|| "无法创建标签页".into())
    }

    pub async fn new_tab(&self, url: &str, throttle: bool) -> Result<Tab, String> {
        if throttle {
            self.pace(url, Duration::from_millis(3000)).await;
        }
        let conn = self.ensure().await?;
        let r = conn.send("Target.createTarget", json!({"url": url}), T).await?;
        let id = r.get("targetId").and_then(|x| x.as_str()).map(|s| s.to_string()).ok_or("无法创建标签页")?;
        // 标签一创建就挂上守卫：接入 / 等待加载期间 future 被外层超时丢弃，也会关闭这个标签（修复标签泄漏）
        let mut pending = PendingTarget { conn: conn.clone(), id: id.clone(), armed: true };
        let r = conn.send("Target.attachToTarget", json!({"targetId": id, "flatten": true}), T).await?;
        let session = r.get("sessionId").and_then(|s| s.as_str()).unwrap_or("").to_string();
        if session.is_empty() {
            return Err("无法接入标签页".into());
        }
        let tab = Tab { id, session, conn };
        tab.ready(Duration::from_secs(12)).await;
        pending.armed = false;
        Ok(tab)
    }

    /// 打开官网登录页：普通标签页，不接入、不执行任何脚本，只把窗口移到屏幕上。
    pub async fn open_plain(&self, url: &str) -> Result<String, String> {
        let id = self.create_target(url).await?;
        self.show(&id).await;
        Ok(id)
    }

    pub async fn close_tab(&self, tab: Tab) {
        let _ = tab.conn.send("Target.detachFromTarget", json!({"sessionId": tab.session}), Duration::from_secs(3)).await;
        self.close_target(&tab.id).await;
    }

    pub async fn close_target(&self, id: &str) {
        self.hide(id).await;
        let conn = self.inner.lock().await.conn.clone();
        if let Some(c) = conn.filter(|c| !c.is_closed()) {
            let _ = c.send("Target.closeTarget", json!({"targetId": id}), Duration::from_secs(5)).await;
        }
    }

    /// 只读列出标签页（不接入页面）：[(id, url)]
    pub async fn targets(&self) -> Vec<(String, String)> {
        let conn = self.inner.lock().await.conn.clone();
        let Some(c) = conn.filter(|c| !c.is_closed()) else { return vec![] };
        match c.send("Target.getTargets", json!({}), Duration::from_secs(5)).await {
            Ok(r) => r
                .get("targetInfos")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter(|t| t.get("type").and_then(|x| x.as_str()) == Some("page"))
                        .filter_map(|t| Some((t.get("targetId")?.as_str()?.to_string(), t.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string())))
                        .collect()
                })
                .unwrap_or_default(),
            Err(_) => vec![],
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
            None => self.create_target("about:blank").await?,
        };
        self.show(&id).await;
        Ok(())
    }

    /* ------------------------------------------------------------ Cookie（只在本机浏览器内部处理） */
    pub async fn cookies(&self) -> Result<Vec<Value>, String> {
        let conn = self.ensure().await?;
        let r = conn.send("Storage.getCookies", json!({}), T).await?;
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
            conn.send("Storage.setCookies", json!({"cookies": fixed}), T).await?;
        }
        Ok(fixed.len())
    }

    /// 写入 Cookie（`Storage.setCookies` 参数格式）。
    pub async fn set_cookies(&self, params: Vec<Value>) -> Result<usize, String> {
        if params.is_empty() {
            return Ok(0);
        }
        let n = params.len();
        let conn = self.ensure().await?;
        conn.send("Storage.setCookies", json!({"cookies": params}), T).await?;
        Ok(n)
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

    /// 正常关闭浏览器：先发 `Browser.close` 让浏览器把 Cookie 数据库写盘，最多等 5 秒再强制结束
    /// （强制结束可能丢掉刚登录的会话——这是"明明登录了，重启后却搜不出来"的原因之一）。
    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(conn) = inner.conn.take() {
            let _ = conn.send("Browser.close", json!({}), Duration::from_secs(5)).await;
        }
        if let Some(mut p) = inner.proc.take() {
            for _ in 0..50 {
                if p.exited() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            p.kill();
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
        if let Some(parent) = self.profile.parent() {
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
