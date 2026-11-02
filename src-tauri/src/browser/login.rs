//! 登录状态机（修复 v3.4 的"卡死"与误判）。
//!
//! status：unknown | logged_out | logged_in | expired | not_needed
//! flow  ：idle → opening → waiting → verifying → success | cancelled | timeout | failed
//!
//! - 登录页是普通标签页，程序不注入、不执行脚本；只读取 Cookie 名与值哈希判断变化，原值不保存、不外传。
//! - 基线在登录页打开 5 秒后采集（避免首页加载时刷新的 Cookie 被误判为"已登录"）。
//! - 账号 Cookie 变化需连续两次一致，且标签页已离开登录地址，才判定成功。
//! - 用户关闭登录窗口 → 立刻检查一次：有变化判成功，否则"已取消"；任何时候都可以重新发起。
//! - 结果验证：搜索成功刷新 verified_at；撞登录墙写 last_wall_at → "已过期"。

use super::manager::{domain_match, Browser};
use crate::db::{now_iso, Db};
use crate::models::LoginState;
use crate::platforms::{self, Platform};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

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
}

fn load_meta(db: &Db, id: &str) -> Meta {
    db.conn()
        .query_row(
            "SELECT verified_at, last_wall_at, has_auth, cookie_expires FROM login_meta WHERE platform_id=?1",
            [id],
            |r| Ok(Meta { verified_at: r.get(0)?, last_wall_at: r.get(1)?, has_auth: r.get::<_, Option<i64>>(2)?.map(|x| x != 0), cookie_expires: r.get(3)? }),
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
        "INSERT INTO login_meta(platform_id, verified_at, last_wall_at, has_auth, cookie_expires, updated_at) VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(platform_id) DO UPDATE SET verified_at=excluded.verified_at, last_wall_at=excluded.last_wall_at,
         has_auth=excluded.has_auth, cookie_expires=excluded.cookie_expires, updated_at=excluded.updated_at",
        params![id, m.verified_at, m.last_wall_at, m.has_auth.map(|b| b as i64), m.cookie_expires, now_iso()],
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

fn status_of(p: &Platform, m: &Meta) -> String {
    let wall_after_verify = match (&m.last_wall_at, &m.verified_at) {
        (Some(w), Some(v)) => w > v,
        (Some(_), None) => true,
        _ => false,
    };
    match m.has_auth {
        Some(true) if wall_after_verify => "expired".into(),
        Some(true) => "logged_in".into(),
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

fn fingerprint(cookies: &[serde_json::Value], p: &Platform) -> BTreeMap<String, String> {
    cookies
        .iter()
        .filter(|c| {
            domain_match(c["domain"].as_str().unwrap_or(""), &p.cookie_domains)
                && p.auth_cookies.iter().any(|n| Some(n.as_str()) == c["name"].as_str())
                && !c["value"].as_str().unwrap_or("").is_empty()
        })
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
    let hits: Vec<&serde_json::Value> = cookies
        .iter()
        .filter(|c| {
            domain_match(c["domain"].as_str().unwrap_or(""), &p.cookie_domains)
                && p.auth_cookies.iter().any(|n| Some(n.as_str()) == c["name"].as_str())
                && !c["value"].as_str().unwrap_or("").is_empty()
        })
        .collect();
    let exp = hits.iter().filter_map(|c| c["expires"].as_f64()).filter(|e| *e > 0.0).fold(None, |a: Option<f64>, e| Some(a.map_or(e, |x| x.max(e))));
    let exp = exp.and_then(|e| chrono::DateTime::from_timestamp(e as i64, 0)).map(crate::db::iso);
    (!hits.is_empty(), exp)
}

fn is_login_url(url: &str, p: &Platform) -> bool {
    let u = url.to_lowercase();
    p.wall_url.iter().any(|h| u.contains(&h.to_lowercase())) || u.contains("login") || u.contains("signin")
}

impl Default for LoginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginManager {
    pub fn new() -> Self {
        LoginManager { flows: Mutex::new(HashMap::new()), seq: Mutex::new(0) }
    }

    fn flow_of(&self, id: &str) -> Option<Flow> {
        self.flows.lock().ok()?.get(id).cloned()
    }

    fn set_flow(&self, id: &str, flow: &str, message: Option<String>, seq: u64) -> bool {
        if let Ok(mut m) = self.flows.lock() {
            if let Some(f) = m.get_mut(id) {
                if f.seq != seq {
                    return false; // 已被新的登录流程替换
                }
                f.flow = flow.into();
                f.message = message;
                return true;
            }
        }
        false
    }

    pub fn state(&self, db: &Db, all: &[Platform], p: &Platform) -> LoginState {
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
            shared_with: platforms::shared_with(all, p),
        }
    }

    fn emit(&self, app: &AppHandle, db: &Db, id: &str) {
        let all = platforms::all(db);
        if let Some(p) = all.iter().find(|p| p.id == id) {
            let _ = app.emit("login://state", self.state(db, &all, p));
        }
    }

    /// 从浏览器 Cookie 刷新"是否有账号 Cookie"（浏览器未运行且不强制时不启动浏览器）。
    pub async fn refresh(&self, db: &Db, browser: &Browser, force: bool) -> Vec<LoginState> {
        let all = platforms::all(db);
        let can = browser.alive().await || (force && browser.profile_dir().exists());
        if can {
            if let Ok(cookies) = browser.cookies().await {
                for p in all.iter() {
                    let (has, exp) = auth_summary(&cookies, p);
                    if p.auth_cookies.is_empty() {
                        continue;
                    }
                    upsert_meta(db, &p.id, |m| {
                        m.has_auth = Some(has);
                        m.cookie_expires = exp;
                    });
                }
            }
        }
        all.iter().map(|p| self.state(db, &all, p)).collect()
    }

    pub fn states(&self, db: &Db) -> Vec<LoginState> {
        let all = platforms::all(db);
        all.iter().map(|p| self.state(db, &all, p)).collect()
    }

    /// 发起登录：打开官网（普通标签），后台观察 Cookie 变化。
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
        let finish = |me: &Arc<Self>, app: &AppHandle, db: &Db, state: &str, msg: String| {
            if me.set_flow(&p.id, state, Some(msg), seq) {
                me.emit(app, db, &p.id);
            }
        };
        let target = match browser.open_plain(&p.home).await {
            Ok(t) => t,
            Err(e) => {
                finish(&self, &app, &db, "failed", format!("无法打开浏览器：{e}"));
                self.reset_later(app, db, p.id.clone(), seq);
                return;
            }
        };
        // 等页面稳定后取基线
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = flow.cancel.cancelled() => {}
        }
        let baseline = browser.cookies().await.map(|c| fingerprint(&c, &p)).unwrap_or_default();
        let hint = if p.auth_cookies.is_empty() { "登录完成后点「我已登录」" } else { "登录成功后会自动识别并保存" };
        finish(&self, &app, &db, "waiting", format!("请在弹出的浏览器窗口中登录 {}，{hint}", p.name));

        let deadline = Instant::now() + Duration::from_secs(600);
        let mut candidate: Option<BTreeMap<String, String>> = None;
        let outcome: (&str, String) = loop {
            let confirmed = tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => false,
                _ = flow.confirm.notified() => true,
                _ = flow.cancel.cancelled() => break ("cancelled", "已取消登录".into()),
            };
            if Instant::now() > deadline {
                break ("timeout", "10 分钟内没有检测到登录，可以重新发起".into());
            }
            let targets = browser.targets().await;
            let tab = targets.iter().find(|(id, _)| id == &target);
            let cookies = browser.cookies().await.unwrap_or_default();
            let now = fingerprint(&cookies, &p);
            let changed = !now.is_empty() && now != baseline;
            if confirmed {
                finish(&self, &app, &db, "verifying", "正在确认登录状态…".into());
                let (has, _) = auth_summary(&cookies, &p);
                if has || p.auth_cookies.is_empty() {
                    break ("success", format!("{} 已登录", p.name));
                }
                finish(&self, &app, &db, "waiting", "还没有检测到登录信息，请在窗口中完成登录后再点「我已登录」".into());
                continue;
            }
            if tab.is_none() {
                // 用户关闭了登录窗口
                break if changed { ("success", format!("{} 已登录", p.name)) } else { ("cancelled", "登录窗口已关闭".into()) };
            }
            if changed {
                let still_login_page = tab.map(|(_, u)| is_login_url(u, &p)).unwrap_or(false);
                if candidate.as_ref() == Some(&now) && !still_login_page {
                    finish(&self, &app, &db, "verifying", "检测到登录，正在保存会话…".into());
                    tokio::time::sleep(Duration::from_secs(3)).await; // 等平台写完全部 Cookie
                    break ("success", format!("{} 已登录，之后的搜索会自动使用这个会话", p.name));
                }
                candidate = Some(now);
            } else {
                candidate = None;
            }
        };
        if outcome.0 == "success" {
            let _ = browser.persist_session_cookies(&p.cookie_domains).await;
            let cookies = browser.cookies().await.unwrap_or_default();
            let (has, exp) = auth_summary(&cookies, &p);
            // 同账号组的平台一并更新
            for o in platforms::all(&db).iter().filter(|o| o.account_group == p.account_group) {
                upsert_meta(&db, &o.id, |m| {
                    m.has_auth = Some(has || o.auth_cookies.is_empty());
                    m.cookie_expires = exp.clone();
                    m.verified_at = Some(now_iso());
                    m.last_wall_at = None;
                });
                if o.id != p.id {
                    self.emit(&app, &db, &o.id);
                }
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
            browser.close_target(&target).await;
            crate::notify::push(&app, &db, "login", "success", &format!("{} 已登录", p.name), "登录会话保存在本机，之后搜索会自动使用。", "/platforms/accounts", "").await;
        } else if outcome.0 == "cancelled" || outcome.0 == "timeout" {
            browser.close_target(&target).await;
        }
        finish(&self, &app, &db, outcome.0, outcome.1);
        self.reset_later(app, db, p.id.clone(), seq);
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

    /// 用户点「我已登录」：流程中 → 立即确认；没有流程 → 主动验证一次（打开首页看是否撞登录墙）。
    pub async fn confirm(self: &Arc<Self>, app: &AppHandle, db: &Db, browser: &Arc<Browser>, id: &str) -> LoginState {
        if let Some(f) = self.flow_of(id) {
            if f.flow == "waiting" || f.flow == "opening" {
                f.confirm.notify_one();
                return self.current(db, id);
            }
        }
        let Some(p) = platforms::get(db, id) else { return self.current(db, id) };
        let url = if p.keepalive.is_empty() { p.home.clone() } else { p.keepalive.clone() };
        if let Ok(pr) = super::extract::probe(browser, Some(&p), &url).await {
            if pr.login_wall {
                mark_wall(db, id);
            }
        }
        if let Ok(cookies) = browser.cookies().await {
            let (has, exp) = auth_summary(&cookies, &p);
            if !p.auth_cookies.is_empty() {
                upsert_meta(db, id, |m| {
                    m.has_auth = Some(has);
                    m.cookie_expires = exp;
                    if has && m.last_wall_at.as_deref().is_none_or(|w| m.verified_at.as_deref().is_some_and(|v| v > w)) {
                        m.verified_at = Some(now_iso());
                    }
                });
            }
        }
        self.emit(app, db, id);
        self.current(db, id)
    }

    pub fn cancel(&self, app: &AppHandle, db: &Db, id: &str) -> LoginState {
        if let Some(f) = self.flow_of(id) {
            f.cancel.cancel();
        }
        self.emit(app, db, id);
        self.current(db, id)
    }

    pub async fn logout(&self, app: &AppHandle, db: &Db, browser: &Browser, id: &str) -> Result<LoginState, String> {
        let all = platforms::all(db);
        let p = all.iter().find(|x| x.id == id).ok_or("未知平台")?;
        if let Some(f) = self.flow_of(id) {
            f.cancel.cancel();
        }
        browser.delete_site_cookies(&p.cookie_domains).await?;
        for o in all.iter().filter(|o| o.account_group == p.account_group) {
            upsert_meta(db, &o.id, |m| {
                m.has_auth = Some(false);
                m.verified_at = None;
                m.cookie_expires = None;
            });
            self.emit(app, db, &o.id);
        }
        Ok(self.current(db, id))
    }

    pub fn current(&self, db: &Db, id: &str) -> LoginState {
        let all = platforms::all(db);
        match all.iter().find(|p| p.id == id) {
            Some(p) => self.state(db, &all, p),
            None => LoginState { platform_id: id.into(), status: "unknown".into(), flow: "idle".into(), ..Default::default() },
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
        let ok = Meta { has_auth: Some(true), verified_at: Some("2026-11-01T10:00:00.000Z".into()), ..Default::default() };
        assert_eq!(status_of(&tb, &ok), "logged_in");
        let expired = Meta { last_wall_at: Some("2026-11-01T11:00:00.000Z".into()), ..ok.clone() };
        assert_eq!(status_of(&tb, &expired), "expired");
        let reverified = Meta { verified_at: Some("2026-11-01T12:00:00.000Z".into()), ..expired };
        assert_eq!(status_of(&tb, &reverified), "logged_in");
        let amazon = plat("amazon_de");
        assert_eq!(status_of(&amazon, &Meta::default()), "not_needed");
    }

    #[test]
    fn fingerprint_ignores_guest_and_other_domains() {
        let tb = plat("taobao");
        let cookies = vec![
            serde_json::json!({"domain": ".taobao.com", "name": "cna", "value": "guest"}),
            serde_json::json!({"domain": ".jd.com", "name": "unb", "value": "x"}),
        ];
        assert!(fingerprint(&cookies, &tb).is_empty());
        let logged = vec![serde_json::json!({"domain": ".taobao.com", "name": "unb", "value": "2200", "expires": 1893456000.0})];
        assert_eq!(fingerprint(&logged, &tb).len(), 1);
        let (has, exp) = auth_summary(&logged, &tb);
        assert!(has && exp.is_some());
    }
}
