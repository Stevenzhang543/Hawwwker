//! 浏览器端到端测试（需要本机 Chromium）：
//!   HAWWWKER_BROWSER=/path/to/chrome HAWWWKER_HEADLESS=1 cargo test --test browser_e2e -- --ignored --nocapture
//! 用本地模拟的"商城"页面验证：卡片抽取、价格/标题解析、登录墙识别、详情页读价、Cookie 按域名删除。

use hawwwker_lib::browser::extract::{probe, search_platform, Outcome, SearchOpts};
use hawwwker_lib::browser::manager::Browser;
use hawwwker_lib::platforms::BUILTIN;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const SEARCH_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Suche</title>
<style>.card{display:block;width:220px;margin:8px;border:1px solid #ccc}.t{font-size:15px}.p{font-size:24px;font-weight:700}.old{text-decoration:line-through;font-size:12px}</style></head>
<body><div id="grid">
 <div class="card"><a href="/dp/B09Y2MYL5C"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120" alt="Sony WH-1000XM5 Kabellose Kopfhörer Schwarz"></a>
   <h2 class="t"><a href="/dp/B09Y2MYL5C"><span>Sony WH-1000XM5 Kabellose</span> <span>Kopfhörer, Schwarz</span></a></h2>
   <div><span>4,6 von 5 Sternen</span> <span>(12.345)</span></div>
   <div class="p"><span>289</span><span>,99</span><span> €</span></div><div class="old">UVP: 419,00 €</div>
   <div>Kostenlose Lieferung Mi., 5. Nov.</div></div>
 <div class="card"><span>Gesponsert</span><a href="/dp/B0C1234567"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120"></a>
   <h2 class="t"><a href="/dp/B0C1234567">Ohrpolster für Sony WH-1000XM5 Ersatz Earpads</a></h2>
   <div class="p">19,99 €</div></div>
 <div class="card"><a href="/dp/B0D7654321"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120"></a>
   <h2 class="t"><a href="/dp/B0D7654321">Sony WH-1000XM5 Generalüberholt</a></h2>
   <div class="p">1.099,00 €</div></div>
</div></body></html>"#;

const LOGIN_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Anmelden</title></head><body><h1>Anmelden</h1><form><input type="password"></form></body></html>"#;

const DETAIL_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Sony</title>
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"Sony WH-1000XM5","offers":{"@type":"Offer","price":"279.00","priceCurrency":"EUR","availability":"https://schema.org/InStock"}}</script>
</head><body><h1>Sony WH-1000XM5</h1><span style="font-size:28px">279,00 €</span></body></html>"#;

async fn serve() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body, extra) = if path.starts_with("/s?") {
                    ("200 OK", SEARCH_PAGE, "Set-Cookie: session-id=abc; Path=/\r\nSet-Cookie: x-acbde=user; Path=/\r\n")
                } else if path.starts_with("/locked") {
                    ("302 Found", "", "Location: /ap/signin\r\n")
                } else if path.starts_with("/ap/signin") {
                    ("200 OK", LOGIN_PAGE, "")
                } else if path.starts_with("/dp/") {
                    ("200 OK", DETAIL_PAGE, "")
                } else {
                    ("404 Not Found", "nope", "")
                };
                let resp = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}", body.len());
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn search_login_probe_cookies() {
    let port = serve().await;
    let dir = std::env::temp_dir().join(format!("hawwwker-e2e-{}", std::process::id()));
    let browser = Arc::new(Browser::new(dir.clone()));
    browser.configure(true, 2);

    // 用 Amazon.de 的规则，但把地址指向本地模拟页面
    let mut p = BUILTIN.iter().find(|p| p.id == "amazon_de").unwrap().clone();
    p.search = format!("http://127.0.0.1:{port}/s?k={{q+}}");
    p.link = r"/dp/([A-Z0-9]{10})".into();
    p.cookie_domains = vec!["127.0.0.1".into()];

    let notify = |s: &str, m: &str| println!("[notify] {s}: {m}");
    let cancel = CancellationToken::new();
    let out = search_platform(&browser, &p, &["Sony WH-1000XM5".into()], "relevance", SearchOpts { target: 10, max_rounds: 1, wait_user_secs: 0 }, &notify, &cancel).await;
    let cands = match out {
        Outcome::Ok(c) => c,
        Outcome::Error(e) => panic!("search error: {e}"),
        Outcome::LoginRequired(m) => panic!("login: {m}"),
        Outcome::Cancelled => panic!("cancelled"),
    };
    for c in &cands {
        println!("{:<55} {:?} {} orig={:?} rating={:?} ad={} cond={}", c.title, c.price, c.currency, c.original_price, c.rating, c.is_ad, c.condition);
    }
    assert_eq!(cands.len(), 3);
    let main = cands.iter().find(|c| c.external_id == "B09Y2MYL5C").unwrap();
    assert!(main.title.contains("WH-1000XM5"), "title: {}", main.title);
    assert_eq!(main.price, Some(289.99));
    assert_eq!(main.currency, "EUR");
    assert_eq!(main.original_price, Some(419.0));
    assert_eq!(main.rating, Some(4.6));
    let pads = cands.iter().find(|c| c.external_id == "B0C1234567").unwrap();
    assert!(pads.is_ad);
    assert_eq!(pads.price, Some(19.99));
    let refurb = cands.iter().find(|c| c.external_id == "B0D7654321").unwrap();
    assert_eq!(refurb.price, Some(1099.0));
    assert_eq!(refurb.condition, "used");

    // 登录墙：被重定向到 /ap/signin
    let mut locked = p.clone();
    locked.search = format!("http://127.0.0.1:{port}/locked?q={{q}}");
    match search_platform(&browser, &locked, &["x".into()], "", SearchOpts { target: 5, max_rounds: 0, wait_user_secs: 0 }, &notify, &cancel).await {
        Outcome::LoginRequired(m) => println!("login wall detected: {m}"),
        Outcome::Ok(_) => panic!("expected login wall"),
        Outcome::Error(e) => panic!("expected login wall, got error {e}"),
        Outcome::Cancelled => panic!("cancelled"),
    }

    // 详情页读价（JSON-LD）
    let pr = probe(&browser, Some(&p), &format!("http://127.0.0.1:{port}/dp/B09Y2MYL5C")).await.unwrap();
    println!("probe: {:?} {:?} {:?} big={:?}", pr.title, pr.price, pr.currency, pr.big_prices);
    assert_eq!(pr.price, Some(279.0));
    assert_eq!(pr.currency.as_deref(), Some("EUR"));

    // Cookie：搜索页设置的 Cookie 可见；按域名删除后消失
    let cookies = browser.cookies().await.unwrap();
    assert!(cookies.iter().any(|c| c["name"] == "x-acbde"), "cookies: {cookies:?}");
    let persisted = browser.persist_session_cookies(&["127.0.0.1".to_string()]).await.unwrap();
    println!("persisted {persisted} session cookies");
    let after: Vec<_> = browser.cookies().await.unwrap();
    assert!(after.iter().filter(|c| c["name"] == "x-acbde").all(|c| c["session"] == false), "now persistent");
    let removed = browser.delete_site_cookies(&["127.0.0.1".to_string()]).await.unwrap();
    assert!(removed >= 2);
    assert!(browser.cookies().await.unwrap().iter().all(|c| c["domain"] != "127.0.0.1"));

    // 取消：已取消的令牌立即返回
    let c2 = CancellationToken::new();
    c2.cancel();
    assert!(matches!(search_platform(&browser, &p, &["x".into()], "", SearchOpts { target: 5, max_rounds: 0, wait_user_secs: 0 }, &notify, &c2).await, Outcome::Cancelled | Outcome::Error(_)));

    // 超时/取消导致搜索 future 被丢弃时，标签必须被关闭（TabGuard）
    let mut slow = p.clone();
    slow.search = format!("http://127.0.0.1:{port}/never?q={{q}}");
    let qs: Vec<String> = vec!["x".into()];
    let fut = search_platform(&browser, &slow, &qs, "", SearchOpts { target: 5, max_rounds: 0, wait_user_secs: 0 }, &notify, &cancel);
    assert!(tokio::time::timeout(Duration::from_secs(4), fut).await.is_err(), "should still be waiting");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let left: Vec<_> = browser.targets().await.into_iter().filter(|(_, u)| u.contains("/never")).collect();
    assert!(left.is_empty(), "leaked tabs: {left:?}");

    browser.shutdown().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = std::fs::remove_dir_all(dir);
}
