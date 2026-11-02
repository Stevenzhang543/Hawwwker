//! 登录端到端测试（需要本机 Chromium）：
//!   HAWWWKER_BROWSER=/path/to/chrome HAWWWKER_HEADLESS=1 cargo test --test login_e2e -- --ignored --nocapture
//!
//! 本地模拟站点：
//!   /home    首页
//!   /login   登录页（游客 Cookie guest，含密码框）
//!   /verify  短信验证页（已写入强 Cookie sid=pending，但登录尚未完成）
//!   /done    登录完成（sid=ok-1）
//!   /account 账号页：sid 以 ok 开头才返回 200，否则 302 到 /login
//!
//! 验证：登录页 / 验证页上绝不判定成功；离开验证页但账号页仍要求登录时继续等待；完成后才成功；
//! 会话备份可在 Cookie 丢失后恢复；正常关闭浏览器后 Cookie 仍在；启动钩子在新进程里恢复会话。

use hawwwker_lib::browser::login::{verify, watch_login, Verify};
use hawwwker_lib::browser::manager::Browser;
use hawwwker_lib::browser::vault;
use hawwwker_lib::db::Db;
use hawwwker_lib::platforms::Platform;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

fn page(title: &str, body: &str) -> String {
    format!(r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title></head><body>{body}</body></html>"#)
}

async fn serve() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let cookie = req.lines().find(|l| l.to_lowercase().starts_with("cookie:")).unwrap_or("").to_string();
                if path.starts_with("/hang") {
                    tokio::time::sleep(Duration::from_secs(40)).await;
                    return;
                }
                let (status, body, extra) = if path.starts_with("/login") {
                    ("200 OK", page("登录", "<h1>密码登录</h1><form><input name=u><input type=password name=p></form>"), "Set-Cookie: guest=1; Path=/\r\n".to_string())
                } else if path.starts_with("/verify") {
                    ("200 OK", page("安全验证", "<h1>短信验证</h1><input name=code>"), "Set-Cookie: sid=pending; Path=/; Max-Age=86400\r\n".to_string())
                } else if path.starts_with("/done") {
                    ("200 OK", page("登录成功", "<h1>欢迎回来</h1>"), "Set-Cookie: sid=ok-1; Path=/; Max-Age=86400\r\n".to_string())
                } else if path.starts_with("/account") {
                    if cookie.contains("sid=ok") {
                        ("200 OK", page("我的账户", "<h1>我的订单</h1>"), String::new())
                    } else {
                        ("302 Found", String::new(), "Location: /login?from=account\r\n".to_string())
                    }
                } else {
                    ("200 OK", page("MockShop", "<h1>MockShop 首页</h1>"), String::new())
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n{extra}Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

fn platform(port: u16) -> Platform {
    Platform {
        id: "mockshop".into(),
        name: "MockShop".into(),
        account_group: "mockshop".into(),
        login: "required".into(),
        home: format!("http://127.0.0.1:{port}/home"),
        probe_url: format!("http://127.0.0.1:{port}/account"),
        cookie_domains: vec!["127.0.0.1".into()],
        auth_cookies: vec!["sid".into()],
        wall_text: vec!["密码登录".into()],
        ..Default::default()
    }
}

async fn navigate(browser: &Browser, target: &str, url: &str) {
    let conn = browser.ensure().await.unwrap();
    let r = conn.send("Target.attachToTarget", json!({"targetId": target, "flatten": true}), Duration::from_secs(10)).await.unwrap();
    let session = r["sessionId"].as_str().unwrap().to_string();
    conn.send_to(Some(&session), "Page.navigate", json!({"url": url}), Duration::from_secs(10)).await.unwrap();
    let _ = conn.send("Target.detachFromTarget", json!({"sessionId": session}), Duration::from_secs(5)).await;
}

fn has_sid_ok(cookies: &[serde_json::Value]) -> bool {
    cookies.iter().any(|c| c["name"] == "sid" && c["value"].as_str().unwrap_or("").starts_with("ok"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[ignore]
async fn strict_login_detection_vault_and_graceful_shutdown() {
    let port = serve().await;
    let root = std::env::temp_dir().join(format!("hawwwker-login-e2e-{}", std::process::id()));
    let profile = root.join("browser-profile");
    let browser = Arc::new(Browser::new(profile.clone()));
    browser.configure(true, 2);
    let p = platform(port);
    let base = format!("http://127.0.0.1:{port}");

    /* ---------- 1. 严格登录判定 ---------- */
    let target = browser.open_plain(&format!("{base}/home")).await.unwrap();
    let msgs: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(vec![]));
    let m2 = msgs.clone();
    let report = move |s: &str, m: String| {
        println!("[flow] {s}: {m}");
        m2.lock().unwrap().push((s.to_string(), m));
    };
    let cancel = CancellationToken::new();
    let confirm = tokio::sync::Notify::new();
    let started = Instant::now();
    let watcher = async {
        let o = watch_login(&browser, None, &p, &target, &cancel, &confirm, Duration::from_secs(1), Duration::from_secs(150), &report).await;
        (o, Instant::now())
    };
    let driver = async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        navigate(&browser, &target, &format!("{base}/login")).await;
        tokio::time::sleep(Duration::from_secs(6)).await;
        // 短信验证页：强 Cookie 已出现，但仍在验证页 → 不能成功
        navigate(&browser, &target, &format!("{base}/verify?step=sms")).await;
        tokio::time::sleep(Duration::from_secs(9)).await;
        // 离开验证页，但账号页仍要求登录（sid=pending）→ 主动验证失败，继续等待
        navigate(&browser, &target, &format!("{base}/home")).await;
        tokio::time::sleep(Duration::from_secs(16)).await;
        let before_done = Instant::now();
        navigate(&browser, &target, &format!("{base}/done")).await;
        before_done
    };
    let ((outcome, finished), before_done) = tokio::join!(watcher, driver);
    println!("outcome: {} — {} (after {:?})", outcome.state, outcome.message, finished - started);
    assert_eq!(outcome.state, "success", "{}", outcome.message);
    assert!(finished > before_done, "must not succeed before the login really completed");
    assert!(outcome.close_window);
    let log = msgs.lock().unwrap().clone();
    assert!(log.iter().any(|(_, m)| m.contains("身份验证")), "should report being on the verification page: {log:?}");
    assert!(log.iter().any(|(_, m)| m.contains("还在登录中")), "should keep waiting after a failed probe: {log:?}");
    assert!(!log.iter().any(|(s, _)| s == "success"), "watch_login itself never reports success early");
    assert_eq!(verify(&browser, &p).await, Verify::Ok);
    browser.close_target(&target).await;

    /* ---------- 2. 会话备份：Cookie 丢失后恢复 ---------- */
    let db = Db::memory().unwrap();
    let dir = browser.data_dir().unwrap();
    let cookies = browser.cookies().await.unwrap();
    assert!(has_sid_ok(&cookies));
    assert!(vault::save(&db, &dir, &p, &cookies).unwrap() >= 2, "sid + guest");
    browser.delete_site_cookies(&["127.0.0.1".to_string()]).await.unwrap();
    assert!(!has_sid_ok(&browser.cookies().await.unwrap()));
    assert!(matches!(verify(&browser, &p).await, Verify::Fail(_)), "logged out after cookie loss");
    let restored = vault::restore_missing(&db, &browser, std::slice::from_ref(&p)).await;
    assert_eq!(restored, vec!["mockshop".to_string()]);
    assert!(has_sid_ok(&browser.cookies().await.unwrap()));
    assert_eq!(verify(&browser, &p).await, Verify::Ok, "restored session works");

    /* ---------- 3. 正常关闭：Cookie 写盘，重启后仍在 ---------- */
    browser.shutdown().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let b2 = Arc::new(Browser::new(profile.clone()));
    b2.configure(true, 2);
    assert!(has_sid_ok(&b2.cookies().await.unwrap()), "cookie DB flushed on graceful shutdown");

    /* ---------- 4. 启动钩子：新浏览器进程自动恢复会话 ---------- */
    b2.delete_site_cookies(&["127.0.0.1".to_string()]).await.unwrap();
    b2.shutdown().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let b3 = Arc::new(Browser::new(profile.clone()));
    b3.configure(true, 2);
    {
        let (db, dir, p) = (db.clone(), dir.clone(), p.clone());
        b3.set_launch_hook(Arc::new(move |current| vault::plan_restore(&db, &dir, std::slice::from_ref(&p), current).1));
    }
    assert!(has_sid_ok(&b3.cookies().await.unwrap()), "launch hook restored the session");
    assert_eq!(verify(&b3, &p).await, Verify::Ok);

    /* ---------- 5. 用户关闭登录窗口且未登录 → 已取消 ---------- */
    b3.delete_site_cookies(&["127.0.0.1".to_string()]).await.unwrap();
    vault::remove(&db, "mockshop");
    let t2 = b3.open_plain(&format!("{base}/login")).await.unwrap();
    let quiet = |_: &str, _: String| {};
    let c2 = CancellationToken::new();
    let n2 = tokio::sync::Notify::new();
    let closer = async {
        tokio::time::sleep(Duration::from_secs(4)).await;
        b3.close_target(&t2).await;
    };
    let (o2, _) = tokio::join!(watch_login(&b3, None, &p, &t2, &c2, &n2, Duration::from_secs(1), Duration::from_secs(60), &quiet), closer);
    assert_eq!(o2.state, "cancelled", "{}", o2.message);

    /* ---------- 6. new_tab 被外层超时丢弃：标签不能遗留 ---------- */
    let hang = format!("{base}/hang");
    assert!(tokio::time::timeout(Duration::from_secs(2), b3.new_tab(&hang, false)).await.is_err(), "page never loads");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let leaked: Vec<_> = b3.targets().await.into_iter().filter(|(_, u)| u.contains("/hang")).collect();
    assert!(leaked.is_empty(), "leaked tabs: {leaked:?}");

    b3.shutdown().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = std::fs::remove_dir_all(root);
}
