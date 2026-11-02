//! 浏览器自动化：在用户本机的 Edge/Chrome 中，以用户自己的登录会话执行搜索与读价。
//! 边界：登录与验证由用户本人完成；不做指纹伪装、不识别验证码、不破解签名；节奏接近真人（同站间隔 ≥3 秒）。

pub mod cdp;
pub mod extract;
pub mod launch;
pub mod login;
pub mod manager;

use crate::db::Db;
use crate::platforms;
use manager::Browser;
use std::time::Duration;

/// 保活：低频访问已登录平台首页（不搜索、不采集），并把会话 Cookie 持久化。
pub async fn keepalive(db: &Db, browser: &Browser) -> Vec<String> {
    if !browser.profile_dir().exists() {
        return vec![];
    }
    let all = platforms::all(db);
    let states = login::LoginManager::new().refresh(db, browser, true).await;
    let mut touched = vec![];
    for st in states.iter().filter(|s| s.status == "logged_in") {
        let Some(p) = all.iter().find(|p| p.id == st.platform_id && p.enabled && !p.keepalive.is_empty()) else { continue };
        let _slot = browser.slot().await;
        if let Ok(tab) = browser.new_tab(&p.keepalive, true).await {
            tokio::time::sleep(Duration::from_millis(4000 + (chrono::Utc::now().timestamp_subsec_millis() as u64 % 3000))).await;
            browser.close_tab(tab).await;
            let _ = browser.persist_session_cookies(&p.cookie_domains).await;
            touched.push(p.id.clone());
        }
    }
    touched
}
