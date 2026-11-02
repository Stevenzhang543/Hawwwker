//! 登录状态机（v26.03 重做：严格判定 + 主动验证 + 会话备份）。
//!
//! status：unknown | logged_out | logged_in | expired | not_needed
//! flow  ：idle → opening → waiting → verifying → success | cancelled | timeout | failed
//!
//! 判定"登录成功"必须同时满足（修复"还在登录就提示检测到登录、自动关窗"）：
//! 1. 平台的**强登录 Cookie**（只有登录完成后才出现，游客 Cookie 不算）存在，且相对打开时的基线发生变化；
//! 2. 登录标签页已经离开所有登录 / 验证 / 验证码 / 安全检查 / 二次验证地址；
//! 3. 以上条件连续 3 次轮询（约 6 秒）保持不变；
//! 4. **主动验证**通过：在另一个隐藏标签打开平台账号页，没有被要求登录。
//!
//! 验证不通过就继续等待（"还在登录中…"），绝不在登录 / 验证页面上关闭窗口。成功后提示 3 秒再关窗。
//! 登录页是普通标签页，程序不接入、不执行脚本；只读取 Cookie 名与值哈希判断变化，原值不外传。
//! v26.03 起平台之间不再联动：每个平台只看自己的 Cookie 与验证结果，登录 / 退出互不影响。

use super::manager::{domain_match, Browser, TabGuard};
use super::vault;
use crate::db::{now_iso, parse_iso, Db};
use crate::models::{LoginCheck, LoginState};
use crate::platforms::{self, Platform};
use futures_util::stream::{self, StreamExt};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

/// 主动验证结果较新时（分钟内）预检不再重复验证。
const FRESH_MINUTES: i64 = 20;
/// 连续满足条件的轮询次数。
const STABLE_POLLS: u32 = 3;
/// 两次主动验证之间的最短间隔。
const VERIFY_GAP: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct Flow {
    flow: String,
    message: Option<String>,
    cancel: CancellationToken,
    confirm: Arc<tokio::sync::Notify>,
    seq: u64,
}

pub struct LoginManager {
    flows: Mutex<HashMap<String, Flow>>,
    seq: Mutex<u64>,
}

#[derive(Default, Clone)]
struct Meta {
    verified_at: Option<String>,
    last_wall_at: Option<String>,
    has_auth: Option<bool>,
    cookie_expires: Option<String>,
    checked_at: Option<String>,
    probe_ok: Option<bool>,
}

fn load_meta(db: &Db, id: &str) -> Meta {
    db.conn()
        .query_row(
            "SELECT verified_at, last_wall_at, has_auth, cookie_expires, checked_at, probe_ok FROM login_meta WHERE platform_id=?1",
            [id],
            |r| {
                Ok(Meta {
                    verified_at: r.get(0)?,
                    last_wall_at: r.get(1)?,
                    has_auth: r.get::<_, Option<i64>>(2)?.map(|x| x != 0),
                    cookie_expires: r.get(3)?,
                    checked_at: r.get(4)?,
                    probe_ok: r.get::<_, Option<i64>>(5)?.map(|x| x != 0),
                })
            },
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn upsert_meta(db: &Db, id: &str, f: impl FnOnce(&mut Meta)) {
    let mut m = load_meta(db, id);
    f(&mut m);
    let _ = db.conn().execute(
        "INSERT INTO login_meta(platform_id, verified_at, last_wall_at, has_auth, cookie_expires, checked_at, probe_ok, updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(platform_id) DO UPDATE SET verified_at=excluded.verified_at, last_wall_at=excluded.last_wall_at,
         has_auth=excluded.has_auth, cookie_expires=excluded.cookie_expires, checked_at=excluded.checked_at,
         probe_ok=excluded.probe_ok, updated_at=excluded.updated_at",
        params![id, m.verified_at, m.last_wall_at, m.has_auth.map(|b| b as i64), m.cookie_expires, m.checked_at, m.probe_ok.map(|b| b as i64), now_iso()],
    );
}

/// 搜索成功（没有撞到登录墙）→ 已验证。
pub fn mark_verified(db: &Db, id: &str) {
    upsert_meta(db, id, |m| {
        m.verified_at = Some(now_iso());
    });
}

/// 搜索撞到登录墙 → 已过期 / 未登录。
pub fn mark_wall(db: &Db, id: &str) {
    upsert_meta(db, id, |m| {
        m.last_wall_at = Some(now_iso());
    });
}

/// 会话需要重新确认（刚从备份恢复、登录 Cookie 从无到有）：清除验证记录，状态变为"未检测"，下次预检会主动验证。
pub fn mark_unverified(db: &Db, id: &str) {
    upsert_meta(db, id, |m| {
        m.verified_at = None;
        m.last_wall_at = None;
        m.checked_at = None;
        m.probe_ok = None;
    });
}

/// 主动验证结果写入。
fn mark_probe(db: &Db, id: &str, ok: bool) {
    upsert_meta(db, id, |m| {
        let now = now_iso();
        m.checked_at = Some(now.clone());
        m.probe_ok = Some(ok);
        if ok {
            m.verified_at = Some(now);
        } else {
            m.last_wall_at = Some(now);
        }
    });
}

fn status_of(p: &Platform, m: &Meta) -> String {
    let wall_after_verify = match (&m.last_wall_at, &m.verified_at) {
        (Some(w), Some(v)) => w > v,
        (Some(_), None) => true,
        _ => false,
    };
    if p.auth_cookies.is_empty() {
        // 没有可识别的登录 Cookie（自定义平台等）：只看验证结果
        return match (p.login.as_str(), wall_after_verify, m.verified_at.is_some()) {
            ("optional", false, _) => "not_needed".into(),
            (_, true, _) => "logged_out".into(),
            (_, false, true) => "logged_in".into(),
            _ => "unknown".into(),
        };
    }
    match m.has_auth {
        Some(true) if wall_after_verify => "expired".into(),
        // 有强登录 Cookie 且验证过（账号页确认 / 搜索成功），最近一次检查没有失败 → 已登录
        Some(true) if m.verified_at.is_some() => "logged_in".into(),
        // 有 Cookie 但从未验证（例如登录进行到一半、刚从备份恢复）→ 未检测，等主动验证
        Some(true) => "unknown".into(),
        Some(false) | None if p.login == "optional" && !wall_after_verify => "not_needed".into(),
        Some(false) => "logged_out".into(),
        None => {
            if wall_after_verify {
                "logged_out".into()
            } else {
                "unknown".into()
            }
        }
    }
}

fn strong_hits<'a>(cookies: &'a [serde_json::Value], p: &'a Platform) -> impl Iterator<Item = &'a serde_json::Value> + 'a {
    cookies.iter().filter(move |c| {
        domain_match(c["domain"].as_str().unwrap_or(""), &p.cookie_domains)
            && p.auth_cookies.iter().any(|n| Some(n.as_str()) == c["name"].as_str())
            && !c["value"].as_str().unwrap_or("").is_empty()
    })
}

fn fingerprint(cookies: &[serde_json::Value], p: &Platform) -> BTreeMap<String, String> {
    strong_hits(cookies, p)
        .map(|c| {
            let h = Sha256::digest(c["value"].as_str().unwrap_or("").as_bytes());
            (format!("{}|{}", c["domain"].as_str().unwrap_or(""), c["name"].as_str().unwrap_or("")), hex::encode(&h[..8]))
        })
        .collect()
}

fn auth_summary(cookies: &[serde_json::Value], p: &Platform) -> (bool, Option<String>) {
    if p.auth_cookies.is_empty() {
        return (false, None);
    }
    let hits: Vec<&serde_json::Value> = strong_hits(cookies, p).collect();
    let exp = hits.iter().filter_map(|c| c["expires"].as_f64()).filter(|e| *e > 0.0).fold(None, |a: Option<f64>, e| Some(a.map_or(e, |x| x.max(e))));
    let exp = exp.and_then(|e| chrono::DateTime::from_timestamp(e as i64, 0)).map(crate::db::iso);
    (!hits.is_empty(), exp)
}

/// 登录 / 注册 / 验证 / 验证码 / 安全检查 / 二次验证页面（任何平台）。
const AUTH_HOST_PARTS: &[&str] = &["login.", "passport.", "signin.", "sso.", "auth.", "account.login", "verify.", "captcha.", "sec.", "iv.", "havanaone.", "accounts."];
const AUTH_PATH_PARTS: &[&str] = &[
    "login", "signin", "sign-in", "sign_in", "logon", "passport", "/sso", "verify", "verification", "captcha", "punish", "security",
    "/sec/", "havana", "mfa", "/otp", "2fa", "two-factor", "challenge", "/ap/signin", "/ap/mfa", "/ap/cvf", "/ap/challenge", "checkpoint",
    "/risk", "slide", "slider", "nocaptcha", "_____tmd_____", "identity", "authenticate", "register", "signup", "sign-up", "qrcode",
];

pub fn is_auth_url(url: &str, p: &Platform) -> bool {
    let u = url.to_lowercase();
    if u.is_empty() || u == "about:blank" {
        return false;
    }
    let after = u.split("://").nth(1).unwrap_or(&u);
    // 只看主机 + 路径：查询串 / 锚点里常有 ref_=nav_signin、redirect=/login 之类，登录完成后的落地页也会带
    let after = after.split(['?', '#']).next().unwrap_or(after);
    let (host, rest) = after.split_once('/').unwrap_or((after, ""));
    let rest = format!("/{rest}");
    p.wall_url.iter().any(|h| after.contains(&h.to_lowercase()))
        || AUTH_HOST_PARTS.iter().any(|h| host.starts_with(h) || host.contains(&format!(".{h}")))
        || AUTH_PATH_PARTS.iter().any(|x| rest.contains(x))
}

/// 登录流程每次轮询的判断（纯函数，便于测试）。
#[derive(Debug, Clone, PartialEq)]
pub enum FlowStep {
    /// 还没有强登录 Cookie
    Waiting,
    /// 有登录 Cookie，但窗口还在登录 / 验证页面
    OnAuthPage,
    /// 条件满足，正在等待稳定（第 n 次）
    Settling(u32),
    /// 条件稳定，可以做主动验证
    Verify,
}

pub fn flow_step(cookies_ready: bool, on_auth_page: bool, stable_polls: u32) -> FlowStep {
    if !cookies_ready {
        FlowStep::Waiting
    } else if on_auth_page {
        FlowStep::OnAuthPage
    } else if stable_polls < STABLE_POLLS {
        FlowStep::Settling(stable_polls)
    } else {
        FlowStep::Verify
    }
}

/// 强登录 Cookie 已就绪：存在，并且相对基线有变化（或基线里没有）。
pub fn cookies_ready(now: &BTreeMap<String, String>, baseline: &BTreeMap<String, String>) -> bool {
    !now.is_empty() && now != baseline
}

/// 主动验证结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Verify {
    Ok,
    /// 账号页要求登录（原因）
    Fail(String),
    /// 打不开 / 超时，无法判断（原因）
    Unknown(String),
}

const VERIFY_JS: &str = r#"(() => {
  const vis = (el) => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el); return r.width > 4 && r.height > 4 && s.visibility !== 'hidden' && s.display !== 'none'; };
  const pw = [...document.querySelectorAll('input[type=password]')].some(vis);
  const text = (document.body ? document.body.innerText : '').slice(0, 6000);
  return { url: location.href, title: document.title || '', text, password: pw, ready: document.readyState };
})()"#;

/// 根据账号页的状态判断（纯函数）。
pub fn judge_probe(p: &Platform, url: &str, title: &str, text: &str, password: bool) -> Verify {
    if is_auth_url(url, p) {
        return Verify::Fail("账号页跳转到了登录 / 验证页面".into());
    }
    if password {
        return Verify::Fail("账号页显示了密码登录框".into());
    }
    let hay = format!("{title}\n{text}");
    if let Some(w) = p.wall_text.iter().find(|w| hay.contains(w.as_str())) {
        return Verify::Fail(format!("账号页提示「{w}」"));
    }
    Verify::Ok
}

/// 主动验证：在隐藏标签中打开平台账号页（只读页面状态），看是否被要求登录。
pub async fn verify(browser: &Arc<Browser>, p: &Platform) -> Verify {
    let url = [&p.probe_url, &p.keepalive, &p.home].into_iter().find(|u| !u.is_empty()).cloned().unwrap_or_default();
    if url.is_empty() {
        return Verify::Unknown("没有可用于验证的账号页".into());
    }
    let Ok(_slot) = tokio::time::timeout(Duration::from_secs(30), browser.slot()).await else {
        return Verify::Unknown("浏览器正忙（正在搜索），稍后再验证".into());
    };
    // new_tab 内部在创建标签后就挂上守卫：这里即使被外层超时丢弃也不会遗留标签
    let tab = match tokio::time::timeout(Duration::from_secs(25), browser.new_tab(&url, true)).await {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return Verify::Unknown(format!("无法打开账号页：{e}")),
        Err(_) => return Verify::Unknown("账号页打开超时".into()),
    };
    let mut guard = TabGuard::new(browser, &tab.id);
    // 等待跳转（部分平台用脚本跳转到登录页）
    let mut last = Verify::Unknown("账号页没有加载完成".into());
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut seen_complete = 0;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        match tab.eval(VERIFY_JS, Duration::from_secs(5)).await {
            Ok(v) => {
                let cur = v["url"].as_str().unwrap_or("");
                last = judge_probe(p, cur, v["title"].as_str().unwrap_or(""), v["text"].as_str().unwrap_or(""), v["password"].as_bool().unwrap_or(false));
                if matches!(last, Verify::Fail(_)) {
                    break;
                }
                if v["ready"].as_str() == Some("complete") && !cur.is_empty() && cur != "about:blank" {
                    seen_complete += 1;
                    // 页面加载完成后再观察一轮，给脚本跳转留时间
                    if seen_complete >= 2 {
                        break;
                    }
                }
            }
            Err(e) => last = Verify::Unknown(format!("读取账号页失败：{e}")),
        }
    }
    if matches!(last, Verify::Ok) && seen_complete == 0 {
        last = Verify::Unknown("账号页加载超时".into());
    }
    browser.close_tab(tab).await;
    guard.disarm();
    last
}

impl Default for LoginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 登录流程的进度回调：(flow, message)
type Report<'a> = &'a (dyn Fn(&str, String) + Send + Sync);

impl LoginManager {
    pub fn new() -> Self {
        LoginManager { flows: Mutex::new(HashMap::new()), seq: Mutex::new(0) }
    }

    fn flow_of(&self, id: &str) -> Option<Flow> {
        self.flows.lock().ok()?.get(id).cloned()
    }

    /// 正在进行登录流程（打开 / 等待 / 确认中）的平台。
    pub fn active_flows(&self) -> Vec<String> {
        self.flows
            .lock()
            .map(|m| m.iter().filter(|(_, f)| matches!(f.flow.as_str(), "opening" | "waiting" | "verifying")).map(|(k, _)| k.clone()).collect())
            .unwrap_or_default()
    }

    /// 可以备份会话的平台：已登录（验证过）且没有正在进行的登录流程。
    pub fn backup_candidates(&self, db: &Db) -> Vec<Platform> {
        let active = self.active_flows();
        let all = platforms::all(db);
        all.iter().filter(|p| !active.contains(&p.id) && self.state(db, &all, p).status == "logged_in").cloned().collect()
    }

    fn set_flow(&self, id: &str, flow: &str, message: Option<String>, seq: u64) -> bool {
        if let Ok(mut m) = self.flows.lock() {
            if let Some(f) = m.get_mut(id) {
                if f.seq != seq {
                    return false; // 已被新的登录流程替换
                }
                if f.flow == flow && f.message == message {
                    return false;
                }
                f.flow = flow.into();
                f.message = message;
                return true;
            }
        }
        false
    }

    pub fn state(&self, db: &Db, _all: &[Platform], p: &Platform) -> LoginState {
        let m = load_meta(db, &p.id);
        let f = self.flow_of(&p.id);
        LoginState {
            platform_id: p.id.clone(),
            status: status_of(p, &m),
            flow: f.as_ref().map(|f| f.flow.clone()).unwrap_or_else(|| "idle".into()),
            verified_at: m.verified_at.clone(),
            last_wall_at: m.last_wall_at.clone(),
            cookie_expires: m.cookie_expires.clone(),
            message: f.and_then(|f| f.message),
            checked_at: m.checked_at.clone(),
            vaulted: !p.auth_cookies.is_empty() && vault::has(db, &p.id),
        }
    }

    fn emit(&self, app: &AppHandle, db: &Db, id: &str) {
        let _ = app.emit("login://state", self.current(db, id));
    }

    pub async fn refresh(&self, db: &Db, browser: &Browser, force: bool) -> Vec<LoginState> {
        let all = platforms::all(db);
        let can = browser.alive().await || (force && browser.profile_dir().exists());
        if can {
            if let Ok(cookies) = browser.cookies().await {
                apply_cookies(db, &all, &cookies, &self.active_flows());
            }
        }
        all.iter().map(|p| self.state(db, &all, p)).collect()
    }

    pub fn states(&self, db: &Db) -> Vec<LoginState> {
        let all = platforms::all(db);
        all.iter().map(|p| self.state(db, &all, p)).collect()
    }

    /// 发起登录：打开官网（普通标签），后台观察 Cookie 与页面地址，满足条件后主动验证。
    pub fn start(self: &Arc<Self>, app: AppHandle, db: Db, browser: Arc<Browser>, id: &str) -> LoginState {
        let all = platforms::all(&db);
        let Some(p) = all.iter().find(|x| x.id == id).cloned() else {
            return LoginState { platform_id: id.into(), status: "unknown".into(), flow: "failed".into(), message: Some("未知平台".into()), ..Default::default() };
        };
        // 替换旧流程（同一平台）
        let seq = {
            let mut s = self.seq.lock().unwrap_or_else(|e| e.into_inner());
            *s += 1;
            *s
        };
        let flow = Flow { flow: "opening".into(), message: Some(format!("正在打开 {} 官网…", p.name)), cancel: CancellationToken::new(), confirm: Arc::new(tokio::sync::Notify::new()), seq };
        if let Ok(mut m) = self.flows.lock() {
            if let Some(old) = m.insert(p.id.clone(), flow.clone()) {
                old.cancel.cancel();
            }
        }
        self.emit(&app, &db, &p.id);
        let current = self.state(&db, &all, &p);
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            me.run_flow(app, db, browser, p, flow).await;
        });
        current
    }

    async fn run_flow(self: Arc<Self>, app: AppHandle, db: Db, browser: Arc<Browser>, p: Platform, flow: Flow) {
        let seq = flow.seq;
        let me = self.clone();
        let (app2, db2, id2) = (app.clone(), db.clone(), p.id.clone());
        let report = move |state: &str, msg: String| {
            if me.set_flow(&id2, state, Some(msg), seq) {
                me.emit(&app2, &db2, &id2);
            }
        };
        let target = match browser.open_plain(&p.home).await {
            Ok(t) => t,
            Err(e) => {
                report("failed", format!("无法打开浏览器：{e}"));
                self.reset_later(app, db, p.id.clone(), seq);
                return;
            }
        };
        let outcome = watch_login(&browser, Some(&db), &p, &target, &flow.cancel, &flow.confirm, Duration::from_secs(5), Duration::from_secs(600), &report).await;
        match outcome.state {
            "success" => {
                self.save_success(&db, &browser, &p).await;
                if outcome.close_window {
                    report("success", format!("{} 登录成功，3 秒后自动关闭窗口", p.name));
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    // 关窗前再看一眼：如果窗口又进入了验证页面，就不关
                    let url = browser.targets().await.into_iter().find(|(id, _)| id == &target).map(|(_, u)| u);
                    if url.as_deref().is_some_and(|u| !is_auth_url(u, &p)) {
                        browser.close_target(&target).await;
                    }
                }
                crate::notify::push(&app, &db, "login", "success", &format!("{} 已登录", p.name), "登录会话保存在本机（含加密备份），之后搜索会自动使用。", "/platforms/accounts", "").await;
            }
            "cancelled" | "timeout" => browser.close_target(&target).await,
            _ => {}
        }
        report(outcome.state, outcome.message);
        self.reset_later(app, db, p.id.clone(), seq);
    }

    /// 登录成功后：持久化会话 Cookie、更新状态、加密备份。
    async fn save_success(&self, db: &Db, browser: &Arc<Browser>, p: &Platform) {
        let _ = browser.persist_session_cookies(&p.cookie_domains).await;
        let cookies = browser.cookies().await.unwrap_or_default();
        let (has, exp) = auth_summary(&cookies, p);
        upsert_meta(db, &p.id, |m| {
            let now = now_iso();
            m.has_auth = Some(has || p.auth_cookies.is_empty());
            m.cookie_expires = exp;
            m.verified_at = Some(now.clone());
            m.checked_at = Some(now);
            m.probe_ok = Some(true);
            m.last_wall_at = None;
        });
        vault::snapshot(db, browser, &[p]).await;
    }

    fn reset_later(self: &Arc<Self>, app: AppHandle, db: Db, id: String, seq: u64) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(8)).await;
            let removed = {
                let mut m = me.flows.lock().unwrap_or_else(|e| e.into_inner());
                if m.get(&id).map(|f| f.seq) == Some(seq) {
                    m.remove(&id);
                    true
                } else {
                    false
                }
            };
            if removed {
                me.emit(&app, &db, &id);
            }
        });
    }

    /// 用户点「我已登录」/「验证」：流程中 → 让流程立即做主动验证；没有流程 → 直接主动验证一次。
    pub async fn confirm(self: &Arc<Self>, app: &AppHandle, db: &Db, browser: &Arc<Browser>, id: &str) -> LoginState {
        if let Some(f) = self.flow_of(id) {
            if f.flow == "waiting" || f.flow == "opening" || f.flow == "verifying" {
                f.confirm.notify_one();
                return self.current(db, id);
            }
        }
        let Some(p) = platforms::get(db, id) else { return self.current(db, id) };
        for rid in vault::restore_missing(db, browser, std::slice::from_ref(&p)).await {
            mark_unverified(db, &rid);
        }
        if let Ok(cookies) = browser.cookies().await {
            apply_cookies(db, std::slice::from_ref(&p), &cookies, &[]);
        }
        let message = match verify(browser, &p).await {
            Verify::Ok => {
                mark_probe(db, id, true);
                self.save_success(db, browser, &p).await;
                format!("{} 登录有效（已打开账号页确认）", p.name)
            }
            Verify::Fail(r) => {
                mark_probe(db, id, false);
                format!("{} 需要重新登录：{r}", p.name)
            }
            Verify::Unknown(r) => format!("暂时无法确认 {} 的登录：{r}", p.name),
        };
        self.emit(app, db, id);
        let mut st = self.current(db, id);
        st.message = Some(message);
        st
    }

    pub fn cancel(&self, app: &AppHandle, db: &Db, id: &str) -> LoginState {
        if let Some(f) = self.flow_of(id) {
            f.cancel.cancel();
        }
        self.emit(app, db, id);
        self.current(db, id)
    }

    /// 退出：只删除本平台域名下的 Cookie 与会话备份，不影响其他平台（v26.03 取消平台互联）。
    pub async fn logout(&self, app: &AppHandle, db: &Db, browser: &Browser, id: &str) -> Result<LoginState, String> {
        let all = platforms::all(db);
        let p = all.iter().find(|x| x.id == id).ok_or("未知平台")?;
        if let Some(f) = self.flow_of(id) {
            f.cancel.cancel();
        }
        let states = self.states(db);
        let (delete, kept) = logout_domains(p, &all, &states);
        {
            // 与会话备份互斥：避免备份在删除 Cookie 之前读取、在删除备份之后写入（退出后又被恢复）
            let _g = vault::lock().await;
            browser.delete_site_cookies(&delete).await?;
            vault::remove(db, id);
        }
        upsert_meta(db, id, |m| {
            m.has_auth = Some(false);
            m.verified_at = None;
            m.cookie_expires = None;
            m.checked_at = None;
            m.probe_ok = None;
        });
        self.emit(app, db, id);
        let mut st = self.current(db, id);
        st.message = Some(if kept.is_empty() {
            "已退出，本机会话与备份已删除，其他平台不受影响".into()
        } else {
            let (doms, users): (Vec<String>, Vec<String>) = kept.into_iter().unzip();
            format!("已退出。{} 的 Cookie 仍被 {} 使用，已保留", doms.join("、"), users.join("、"))
        });
        Ok(st)
    }

    pub fn current(&self, db: &Db, id: &str) -> LoginState {
        let all = platforms::all(db);
        match all.iter().find(|p| p.id == id) {
            Some(p) => self.state(db, &all, p),
            None => LoginState { platform_id: id.into(), status: "unknown".into(), flow: "idle".into(), ..Default::default() },
        }
    }

    /// 搜索前预检（v26.03）。verify：对较久未验证（或 force）的平台做主动验证；会先尝试从会话备份恢复。
    pub async fn preflight(&self, app: Option<&AppHandle>, db: &Db, browser: &Arc<Browser>, ids: &[String], verify_now: bool, force: bool) -> Vec<LoginCheck> {
        let all = platforms::all(db);
        let targets: Vec<Platform> = ids.iter().filter_map(|id| all.iter().find(|p| &p.id == id).cloned()).collect();
        let needs: Vec<&Platform> = targets.iter().filter(|p| p.login != "optional").collect();
        let mut verified: HashMap<String, bool> = HashMap::new();
        if verify_now && !needs.is_empty() && browser.executable().is_some() {
            let active = self.active_flows();
            let owned: Vec<Platform> = needs.iter().filter(|p| !active.contains(&p.id)).map(|p| (*p).clone()).collect();
            // 预检时恢复的 + 浏览器启动钩子恢复的会话都要重新确认
            for rid in vault::restore_missing(db, browser, &owned).await.into_iter().chain(vault::take_restored()) {
                mark_unverified(db, &rid);
            }
            if let Ok(cookies) = browser.cookies().await {
                apply_cookies(db, &owned, &cookies, &active);
            }
            let todo: Vec<Platform> = owned
                .into_iter()
                .filter(|p| {
                    let m = load_meta(db, &p.id);
                    let st = status_of(p, &m);
                    let stale = m.checked_at.as_deref().and_then(parse_iso).is_none_or(|t| chrono::Utc::now() - t > chrono::Duration::minutes(FRESH_MINUTES));
                    // 未确认的会话（unknown = 有 Cookie 未验证）总是验证；已登录 / 已过期的在验证过期或强制体检时验证；
                    // 明确未登录的不浪费时间
                    st == "unknown" && m.has_auth == Some(true) || (st == "logged_in" || st == "expired") && (force || stale)
                })
                .collect();
            let results: Vec<(String, Verify)> = stream::iter(todo.into_iter().map(|p| {
                let b = browser.clone();
                async move {
                    // verify 自身有时限（等名额 ≤30 秒 + 打开账号页 ≤30 秒）；这里只是兜底
                    let r = tokio::time::timeout(Duration::from_secs(75), verify(&b, &p)).await.unwrap_or(Verify::Unknown("验证超时".into()));
                    (p.id.clone(), r)
                }
            }))
            .buffer_unordered(3)
            .collect()
            .await;
            for (id, r) in results {
                match r {
                    Verify::Ok => {
                        mark_probe(db, &id, true);
                        verified.insert(id.clone(), true);
                        if let Some(p) = all.iter().find(|p| p.id == id) {
                            vault::snapshot(db, browser, &[p]).await;
                        }
                    }
                    Verify::Fail(_) => {
                        mark_probe(db, &id, false);
                        verified.insert(id.clone(), true);
                    }
                    Verify::Unknown(e) => log::info!("preflight verify {id}: {e}"),
                }
                if let Some(app) = app {
                    self.emit(app, db, &id);
                }
            }
        }
        targets
            .iter()
            .map(|p| {
                let st = self.state(db, &all, p);
                let ok = p.login == "optional" || st.status == "logged_in" || st.status == "not_needed";
                let message = match (ok, p.login.as_str(), st.status.as_str()) {
                    (true, _, _) => None,
                    (false, _, "expired") => Some(format!("{} 的登录已过期，请重新登录", p.name)),
                    (false, "recommended", _) => Some(format!("建议先登录 {}（不登录也能搜，结果可能较少）", p.name)),
                    (false, _, _) => Some(format!("{} 需要先登录才能搜索", p.name)),
                };
                LoginCheck {
                    platform_id: p.id.clone(),
                    need: p.login.clone(),
                    status: st.status,
                    ok,
                    verified_now: verified.contains_key(&p.id),
                    message,
                }
            })
            .collect()
    }
}

/// 用浏览器当前 Cookie 刷新各平台的"是否有强登录 Cookie"。
/// - 正在走登录流程的平台跳过（登录到一半时 Cookie 可能已经写入，不能据此改状态）；
/// - 登录 Cookie 从无到有时清除旧的验证记录（新会话必须重新确认）。
fn apply_cookies(db: &Db, platforms: &[Platform], cookies: &[serde_json::Value], skip: &[String]) {
    for p in platforms.iter().filter(|p| !p.auth_cookies.is_empty() && !skip.contains(&p.id)) {
        let (has, exp) = auth_summary(cookies, p);
        upsert_meta(db, &p.id, |m| {
            if has && m.has_auth != Some(true) {
                m.verified_at = None;
                m.checked_at = None;
                m.probe_ok = None;
            }
            m.has_auth = Some(has);
            m.cookie_expires = exp;
        });
    }
}

/// 退出时要删除的域名，以及因仍被其他已登录平台使用而保留的 (域名, 平台名)。
pub fn logout_domains(p: &Platform, all: &[Platform], states: &[LoginState]) -> (Vec<String>, Vec<(String, String)>) {
    let mut delete = vec![];
    let mut kept = vec![];
    for d in p.cookie_domains.iter() {
        let user = all.iter().find(|o| {
            o.id != p.id && o.cookie_domains.contains(d) && states.iter().any(|s| s.platform_id == o.id && s.status == "logged_in")
        });
        match user {
            Some(o) => kept.push((d.clone(), o.name.clone())),
            None => delete.push(d.clone()),
        }
    }
    (delete, kept)
}

/// 登录流程结果。
pub struct WatchOutcome {
    pub state: &'static str,
    pub message: String,
    /// 是否可以自动关闭登录窗口（"已经登录"的情况保持窗口，方便切换账号）
    pub close_window: bool,
}

fn outcome(state: &'static str, message: String, close_window: bool) -> WatchOutcome {
    WatchOutcome { state, message, close_window }
}

/// 观察登录窗口直到成功 / 取消 / 超时。单独成函数，便于端到端测试（不依赖界面）。
#[allow(clippy::too_many_arguments)]
pub async fn watch_login(
    browser: &Arc<Browser>, db: Option<&Db>, p: &Platform, target: &str, cancel: &CancellationToken, confirm: &tokio::sync::Notify, settle: Duration,
    timeout: Duration, report: Report<'_>,
) -> WatchOutcome {
    // 主动验证失败要记下来（状态显示为需要登录，而不是沿用旧的"已登录"）
    let record_fail = || {
        if let Some(db) = db {
            mark_probe(db, &p.id, false);
        }
    };
    // 等页面稳定后取基线
    tokio::select! {
        _ = tokio::time::sleep(settle) => {}
        _ = cancel.cancelled() => return outcome("cancelled", "已取消登录".into(), false),
    }
    let baseline = browser.cookies().await.map(|c| fingerprint(&c, p)).unwrap_or_default();
    // 已经登录（基线里就有强 Cookie）→ 先确认一次
    if !baseline.is_empty() {
        report("verifying", "检测到已有登录信息，正在打开账号页确认…".into());
        match verify(browser, p).await {
            Verify::Ok => {
                return outcome("success", format!("{} 已经是登录状态（会话仍有效）；如需切换账号，可在窗口中操作后点「我已登录」", p.name), false);
            }
            Verify::Fail(_) => record_fail(),
            Verify::Unknown(_) => {}
        }
    }
    let manual = p.auth_cookies.is_empty();
    let hint = if manual { "登录完成后点「我已登录」" } else { "登录成功后会自动识别（会打开账号页确认），请勿手动关闭窗口" };
    report("waiting", format!("请在弹出的浏览器窗口中登录 {}，{hint}", p.name));

    let deadline = Instant::now() + timeout;
    let mut prev: Option<BTreeMap<String, String>> = None;
    let mut stable = 0u32;
    let mut last_verify: Option<Instant> = None;
    let mut inconclusive = 0u32;
    loop {
        let confirmed = tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => false,
            _ = confirm.notified() => true,
            _ = cancel.cancelled() => return outcome("cancelled", "已取消登录".into(), false),
        };
        if Instant::now() > deadline {
            return outcome("timeout", "10 分钟内没有完成登录，可以重新发起".into(), false);
        }
        let tab_url = browser.targets().await.into_iter().find(|(id, _)| id == target).map(|(_, u)| u);
        let cookies = browser.cookies().await.unwrap_or_default();
        let now = fingerprint(&cookies, p);
        let ready = cookies_ready(&now, &baseline);

        if confirmed {
            report("verifying", "正在打开账号页确认登录…".into());
            // 账号页打不开时，只有"登录 Cookie 相对开始时有变化、且窗口已离开登录 / 验证页"才按登录信息判断
            let off_auth = tab_url.as_deref().is_some_and(|u| !is_auth_url(u, p));
            match verify(browser, p).await {
                Verify::Ok => return outcome("success", format!("{} 已登录，之后的搜索会自动使用这个会话", p.name), true),
                Verify::Unknown(r) if off_auth && (ready || manual) => {
                    return outcome("success", format!("{} 已登录（{r}，已按登录信息判断）", p.name), true);
                }
                Verify::Unknown(r) => report("waiting", format!("暂时无法确认登录（{r}）。请在窗口中完成登录（包括短信 / 扫码 / 滑块验证）后再点「我已登录」")),
                Verify::Fail(r) => {
                    record_fail();
                    report("waiting", format!("账号页仍要求登录（{r}）。请在窗口中完成登录（包括短信 / 扫码 / 滑块验证）后再点「我已登录」"));
                }
            }
            continue;
        }
        let Some(url) = tab_url else {
            // 用户关闭了登录窗口：有新的登录信息就确认一次
            if ready {
                report("verifying", "登录窗口已关闭，正在确认登录…".into());
                return match verify(browser, p).await {
                    Verify::Fail(r) => {
                        record_fail();
                        outcome("cancelled", format!("登录窗口已关闭，但账号页仍要求登录（{r}）"), false)
                    }
                    _ => outcome("success", format!("{} 已登录", p.name), false),
                };
            }
            return outcome("cancelled", "登录窗口已关闭".into(), false);
        };
        let on_auth = is_auth_url(&url, p);
        stable = if ready && !on_auth { if prev.as_ref() == Some(&now) { stable + 1 } else { 1 } } else { 0 };
        prev = Some(now.clone());
        match flow_step(ready, on_auth, stable) {
            FlowStep::Waiting => report("waiting", format!("请在弹出的浏览器窗口中登录 {}，{hint}", p.name)),
            FlowStep::OnAuthPage => report("waiting", "正在登录 / 身份验证中…完成后会自动识别（请勿关闭窗口）".into()),
            FlowStep::Settling(_) => report("verifying", "检测到登录信息，正在确认…（请勿关闭窗口）".into()),
            FlowStep::Verify => {
                if last_verify.is_some_and(|t| t.elapsed() < VERIFY_GAP) {
                    continue;
                }
                last_verify = Some(Instant::now());
                report("verifying", "正在打开账号页确认登录…（请勿关闭窗口）".into());
                match verify(browser, p).await {
                    Verify::Ok => return outcome("success", format!("{} 已登录，之后的搜索会自动使用这个会话", p.name), true),
                    Verify::Fail(_) => {
                        stable = 0;
                        record_fail();
                        report("waiting", "还在登录中…完成后会自动识别（如有短信 / 扫码 / 滑块验证，请在窗口中完成）".into());
                    }
                    Verify::Unknown(r) => {
                        inconclusive += 1;
                        if inconclusive >= 3 {
                            return outcome("success", format!("{} 已登录（账号页暂时打不开：{r}，已按登录信息判断）", p.name), true);
                        }
                        report("verifying", format!("账号页暂时打不开（{r}），稍后重试确认…"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::BUILTIN;

    fn plat(id: &str) -> Platform {
        BUILTIN.iter().find(|p| p.id == id).unwrap().clone()
    }

    #[test]
    fn status_rules() {
        let tb = plat("taobao");
        assert_eq!(status_of(&tb, &Meta::default()), "unknown");
        assert_eq!(status_of(&tb, &Meta { has_auth: Some(false), ..Default::default() }), "logged_out");
        // 有登录 Cookie 但从未验证（登录进行到一半 / 刚恢复）→ 未检测，不是已登录
        assert_eq!(status_of(&tb, &Meta { has_auth: Some(true), ..Default::default() }), "unknown");
        let ok = Meta { has_auth: Some(true), verified_at: Some("2026-11-01T10:00:00.000Z".into()), ..Default::default() };
        assert_eq!(status_of(&tb, &ok), "logged_in");
        let expired = Meta { last_wall_at: Some("2026-11-01T11:00:00.000Z".into()), ..ok.clone() };
        assert_eq!(status_of(&tb, &expired), "expired");
        let reverified = Meta { verified_at: Some("2026-11-01T12:00:00.000Z".into()), ..expired };
        assert_eq!(status_of(&tb, &reverified), "logged_in");
        let amazon = plat("amazon_de");
        assert_eq!(status_of(&amazon, &Meta::default()), "not_needed");
        let custom = Platform { id: "c".into(), login: "required".into(), ..Default::default() };
        assert_eq!(status_of(&custom, &Meta::default()), "unknown");
        assert_eq!(status_of(&custom, &Meta { verified_at: Some("2026-11-01T10:00:00.000Z".into()), ..Default::default() }), "logged_in");
    }

    #[test]
    fn cookies_appearing_require_fresh_verification() {
        let db = Db::memory().unwrap();
        let tb = plat("taobao");
        let logged = vec![serde_json::json!({"domain": ".taobao.com", "name": "unb", "value": "2200"})];
        // 曾经验证过，之后 Cookie 消失（已退出）
        upsert_meta(&db, "taobao", |m| {
            m.has_auth = Some(false);
            m.verified_at = Some("2026-11-01T10:00:00.000Z".into());
        });
        // 登录到一半：Cookie 出现 → 不能沿用旧的验证记录
        apply_cookies(&db, std::slice::from_ref(&tb), &logged, &[]);
        assert_eq!(status_of(&tb, &load_meta(&db, "taobao")), "unknown");
        mark_probe(&db, "taobao", true);
        assert_eq!(status_of(&tb, &load_meta(&db, "taobao")), "logged_in");
        // 正在登录流程中的平台不刷新
        apply_cookies(&db, std::slice::from_ref(&tb), &[], &["taobao".to_string()]);
        assert_eq!(load_meta(&db, "taobao").has_auth, Some(true));
        // 从备份恢复 → 需要重新确认
        mark_unverified(&db, "taobao");
        assert_eq!(status_of(&tb, &load_meta(&db, "taobao")), "unknown");
    }

    #[test]
    fn probe_result_drives_status() {
        let db = Db::memory().unwrap();
        let tb = plat("taobao");
        upsert_meta(&db, "taobao", |m| m.has_auth = Some(true));
        mark_probe(&db, "taobao", true);
        assert_eq!(status_of(&tb, &load_meta(&db, "taobao")), "logged_in");
        std::thread::sleep(Duration::from_millis(5));
        mark_probe(&db, "taobao", false);
        let m = load_meta(&db, "taobao");
        assert_eq!(status_of(&tb, &m), "expired");
        assert_eq!(m.probe_ok, Some(false));
        assert!(m.checked_at.is_some());
    }

    #[test]
    fn fingerprint_ignores_guest_and_other_domains() {
        let tb = plat("taobao");
        let cookies = vec![
            serde_json::json!({"domain": ".taobao.com", "name": "cna", "value": "guest"}),
            serde_json::json!({"domain": ".taobao.com", "name": "cookie2", "value": "guest-session"}),
            serde_json::json!({"domain": ".jd.com", "name": "unb", "value": "x"}),
            serde_json::json!({"domain": ".goofish.com", "name": "unb", "value": "x"}),
        ];
        assert!(fingerprint(&cookies, &tb).is_empty());
        let logged = vec![serde_json::json!({"domain": ".taobao.com", "name": "unb", "value": "2200", "expires": 1893456000.0})];
        assert_eq!(fingerprint(&logged, &tb).len(), 1);
        let (has, exp) = auth_summary(&logged, &tb);
        assert!(has && exp.is_some());
        // 闲鱼不再认淘宝的 Cookie
        assert!(fingerprint(&logged, &plat("xianyu")).is_empty());
    }

    #[test]
    fn auth_urls() {
        let tb = plat("taobao");
        for u in [
            "https://login.taobao.com/member/login.jhtml?redirectURL=x",
            "https://login.taobao.com/member/login_unusual.htm",
            "https://passport.jd.com/new/login.aspx",
            "https://sec.taobao.com/query.htm?action=QueryAction",
            "https://h5api.m.taobao.com/_____tmd_____/punish?x=1",
            "https://www.amazon.de/ap/signin?openid=1",
            "https://www.amazon.de/ap/mfa?arb=1",
            "https://www.amazon.com/ap/cvf/request",
            "https://passport.goofish.com/mini_login.htm",
            "https://www.xiaohongshu.com/website-login/captcha?redirectPath=x",
            "https://verify.example.com/check",
            "https://shop.test/account/verify?step=sms",
            "https://shop.test/2fa",
        ] {
            assert!(is_auth_url(u, &tb), "{u} should be an auth page");
        }
        for u in [
            "https://www.taobao.com/",
            "https://i.taobao.com/my_taobao.htm",
            "https://s.taobao.com/search?q=耳机",
            "https://home.jd.com/",
            "https://www.amazon.de/gp/css/homepage.html",
            "https://www.goofish.com/personal",
            "https://www.amazon.de/?ref_=nav_signin&",
            "https://www.amazon.com/gp/css/homepage.html?ref_=nav_youraccount_btn#signin",
            "https://www.taobao.com/?redirect=/member/login.jhtml",
            "about:blank",
        ] {
            assert!(!is_auth_url(u, &tb), "{u} should not be an auth page");
        }
    }

    #[test]
    fn flow_decisions() {
        assert_eq!(flow_step(false, false, 0), FlowStep::Waiting);
        assert_eq!(flow_step(false, true, 0), FlowStep::Waiting);
        // 有登录 Cookie 但还在验证页（短信 / 滑块）→ 绝不判成功
        assert_eq!(flow_step(true, true, 5), FlowStep::OnAuthPage);
        assert_eq!(flow_step(true, false, 1), FlowStep::Settling(1));
        assert_eq!(flow_step(true, false, 2), FlowStep::Settling(2));
        assert_eq!(flow_step(true, false, 3), FlowStep::Verify);
        let base: BTreeMap<String, String> = [("a|sid".to_string(), "guest".to_string())].into();
        assert!(!cookies_ready(&BTreeMap::new(), &BTreeMap::new()));
        assert!(!cookies_ready(&base, &base));
        let changed: BTreeMap<String, String> = [("a|sid".to_string(), "user".to_string())].into();
        assert!(cookies_ready(&changed, &base));
        assert!(cookies_ready(&changed, &BTreeMap::new()));
    }

    #[test]
    fn probe_judgement() {
        let tb = plat("taobao");
        assert_eq!(judge_probe(&tb, "https://i.taobao.com/my_taobao.htm", "我的淘宝", "已买到的宝贝", false), Verify::Ok);
        assert!(matches!(judge_probe(&tb, "https://login.taobao.com/member/login.jhtml", "登录", "", false), Verify::Fail(_)));
        assert!(matches!(judge_probe(&tb, "https://i.taobao.com/my_taobao.htm", "我的淘宝", "亲，请登录", false), Verify::Fail(_)));
        assert!(matches!(judge_probe(&tb, "https://i.taobao.com/my_taobao.htm", "", "", true), Verify::Fail(_)));
    }

    #[test]
    fn logout_keeps_domains_used_by_logged_in_platforms() {
        let a = Platform { id: "a".into(), name: "A".into(), cookie_domains: vec!["a.test".into(), "shared.test".into()], ..Default::default() };
        let b = Platform { id: "b".into(), name: "B".into(), cookie_domains: vec!["shared.test".into()], ..Default::default() };
        let all = vec![a.clone(), b];
        let st = |id: &str, s: &str| LoginState { platform_id: id.into(), status: s.into(), ..Default::default() };
        let (del, kept) = logout_domains(&a, &all, &[st("a", "logged_in"), st("b", "logged_in")]);
        assert_eq!(del, vec!["a.test".to_string()]);
        assert_eq!(kept, vec![("shared.test".to_string(), "B".to_string())]);
        let (del, kept) = logout_domains(&a, &all, &[st("a", "logged_in"), st("b", "logged_out")]);
        assert_eq!(del.len(), 2);
        assert!(kept.is_empty());
    }
}
