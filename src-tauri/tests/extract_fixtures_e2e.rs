//! v26.03 抽取精度：按各平台公开页面结构编写的本地夹具页 + 真实 Chromium，验证提示选择器、JSON-LD 与评论读取。
//!   HAWWWKER_BROWSER=/path/to/chrome HAWWWKER_HEADLESS=1 cargo test --test extract_fixtures_e2e -- --ignored --nocapture

use hawwwker_lib::browser::extract::{read_detail, search_platform, Outcome, SearchOpts};
use hawwwker_lib::browser::manager::Browser;
use hawwwker_lib::platforms::{Platform, BUILTIN};
use hawwwker_lib::search::parse::Candidate;
use hawwwker_lib::search::reviews;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const AMAZON: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<div class="s-result-list">
 <div data-component-type="s-search-result" data-asin="B09Y2MYL5C"><div class="puis-card">
  <span class="puis-sponsored-label-text">Gesponsert</span>
  <a class="a-link-normal s-no-outline" href="/dp/B09Y2MYL5C"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120"></a>
  <h2><a href="/dp/B09Y2MYL5C"><span>Sony WH-1000XM5 Kabellose Kopfhörer mit Noise Cancelling, Schwarz</span></a></h2>
  <span class="a-icon-alt">4,6 von 5 Sternen</span><a href="/dp/B09Y2MYL5C#customerReviews"><span class="s-underline-text">12.345</span></a>
  <span class="a-size-base a-color-secondary">1K+ bought in past month</span>
  <span class="a-price"><span class="a-offscreen">279,00 €</span><span aria-hidden="true">279,00€</span></span>
  <span class="a-price a-text-price"><span class="a-offscreen">419,00 €</span></span>
  <div data-cy="delivery-recipe">KOSTENLOSE Lieferung Mi., 5. Nov.</div>
  <span class="a-badge-text">Bestseller</span>
 </div></div>
 <div data-component-type="s-search-result" data-asin="B0C1234567"><div class="puis-card">
  <a href="/dp/B0C1234567"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120"></a>
  <h2><a href="/dp/B0C1234567"><span>Ohrpolster für Sony WH-1000XM5 Ersatz Earpads</span></a></h2>
  <span class="a-icon-alt">4,1 von 5 Sternen</span><a href="/dp/B0C1234567#customerReviews"><span class="s-underline-text">812</span></a>
  <span class="a-price"><span class="a-offscreen">19,99 €</span></span>
  <div data-cy="delivery-recipe">Lieferung 4,99 € Versand</div>
 </div></div>
 <div data-component-type="s-search-result" data-asin="B0E1111111"><div class="puis-card">
  <a href="/dp/B0E1111111"><img src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" width="120" height="120"></a>
  <h2><a href="/dp/B0E1111111"><span>Sony WH-1000XM5 Silber</span></a></h2>
  <span class="a-price"><span class="a-offscreen">305,00 €</span></span>
 </div></div>
</div>
<div class="s-footer">Brauchen Sie Hilfe? 0,00 €</div>
</body></html>"#;

const JD: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="J_goodsList"><ul>
 <li class="gl-item" data-sku="100012043978"><div class="gl-i-wrap">
  <div class="p-img"><a href="//item.jd.com/100012043978.html"><img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a></div>
  <div class="p-price"><strong><em>￥</em><i>2299.00</i></strong></div>
  <div class="p-name"><a href="//item.jd.com/100012043978.html"><em>索尼（SONY）WH-1000XM5 头戴式无线降噪耳机 黑色</em></a></div>
  <div class="p-commit"><strong><a>2万+</a>条评价</strong></div>
  <div class="p-shop"><span><a href="//mall.jd.com/index-1000000.html">索尼京东自营官方旗舰店</a></span></div>
  <div class="p-icons"><i>自营</i><i>放心购</i></div>
 </div></li>
 <li class="gl-item" data-sku="100055555555"><div class="gl-i-wrap">
  <div class="p-img"><a href="//item.jd.com/100055555555.html"><img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a></div>
  <div class="p-price"><strong><em>￥</em><i>1599.00</i></strong></div>
  <div class="p-name"><a href="//item.jd.com/100055555555.html"><em>索尼 WH-1000XM5 降噪耳机 95新 二手</em></a></div>
  <div class="p-commit"><strong><a>36</a>条评价</strong></div>
  <div class="p-shop"><span><a href="//mall.jd.com/index-2.html">某某数码专营店</a></span></div>
 </div></li>
 <li class="gl-item" data-sku="100066666666"><div class="gl-i-wrap">
  <div class="p-img"><a href="//item.jd.com/100066666666.html"><img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a></div>
  <div class="p-price"><strong><em>￥</em><i>399.00</i></strong></div>
  <div class="p-name"><a href="//item.jd.com/100066666666.html"><em>头戴式降噪耳机 XM5同款 平替</em></a></div>
  <div class="p-shop"><span><a href="//mall.jd.com/index-3.html">小林的店</a></span></div>
 </div></li>
</ul></div></body></html>"#;

const TAOBAO: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body><div id="content_items_wrapper">
 <a class="Card--doubleCardWrapper--L2XFE73" href="https://item.taobao.com/item.htm?id=711111111111">
  <img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=">
  <div class="Title--title--jCOPvpf"><span>Sony/索尼 WH-1000XM5 头戴式降噪蓝牙耳机</span></div>
  <div class="Price--priceWrapper--Q0Dn7pN"><span class="Price--unit">¥</span><span class="Price--priceInt--ZlsSi_M">1799</span><span class="Price--priceFloat">.00</span></div>
  <span class="Price--realSales--FhTZc7U">3000+人付款</span><div class="Price--procity--_7Vt3mX">广东 深圳</div>
  <div class="ShopInfo--shopName--rg6mGmy">索尼官方旗舰店</div><div class="SalesPoint--subIconWrapper--s6vanNY"><span>天猫</span></div>
 </a>
 <a class="Card--doubleCardWrapper--L2XFE73" href="https://item.taobao.com/item.htm?id=722222222222">
  <img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=">
  <div class="Title--title--jCOPvpf"><span>WH-1000XM5 原单 高仿 耳机</span></div>
  <div class="Price--priceWrapper--Q0Dn7pN"><span>¥</span><span>299</span></div>
  <span class="Price--realSales--FhTZc7U">已售 120</span><div class="Price--procity--_7Vt3mX">广东 广州</div>
  <div class="ShopInfo--shopName--rg6mGmy">阿花的小店</div>
 </a>
 <a class="Card--doubleCardWrapper--L2XFE73" href="https://item.taobao.com/item.htm?id=733333333333">
  <img width="200" height="200" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=">
  <div class="Title--title--jCOPvpf"><span>索尼 WH-1000XM5 日本代购 海外直邮</span></div>
  <div class="Price--priceWrapper--Q0Dn7pN"><span>¥</span><span>1650</span><span>.50</span></div>
  <div class="ShopInfo--shopName--rg6mGmy">东京代购小铺</div>
 </a>
</div></body></html>"#;

const EBAY: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body><ul class="srp-results">
 <li class="s-item"><div class="s-item__info"><a class="s-item__link" href="https://ebay.de/itm/123"><div class="s-item__title"><span>Shop on eBay</span></div></a><span class="s-item__price">20,00 EUR</span></div></li>
 <li class="s-item"><img width="140" height="140" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="><div class="s-item__info"><a class="s-item__link" href="https://www.ebay.de/itm/314159265358"><div class="s-item__title"><span>Neues Angebot Sony WH-1000XM5 Kopfhörer schwarz</span></div></a>
  <div class="s-item__subtitle"><span class="SECONDARY_INFO">Gebraucht</span></div><span class="s-item__price">EUR 199,00</span><span class="s-item__shipping s-item__logisticsCost">+EUR 4,99 Versand</span>
  <span class="s-item__location s-item__itemLocation">aus Deutschland</span><span class="s-item__seller-info-text">kopfhoerer-fan (512) 100%</span><span class="SECONDARY_INFO">Privat</span></div></li>
 <li class="s-item"><img width="140" height="140" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=">><div class="s-item__info"><a class="s-item__link" href="https://www.ebay.de/itm/271828182845"><div class="s-item__title"><span>Sony WH-1000XM5 Neu & OVP</span></div></a>
  <div class="s-item__subtitle"><span class="SECONDARY_INFO">Neu</span></div><span class="s-item__price">EUR 259,00</span><span class="s-item__shipping">Kostenloser Versand</span><span class="s-item__quantitySold">23 verkauft</span></div></li>
 <li class="s-item"><img width="140" height="140" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="><div class="s-item__info"><a class="s-item__link" href="https://www.ebay.de/itm/161803398874"><div class="s-item__title"><span>Sony WH-1000XM5 Silber</span></div></a><span class="s-item__price">EUR 279,00</span></div></li>
</ul></body></html>"#;

const LD: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<script type="application/ld+json">{"@context":"https://schema.org","@type":"ItemList","itemListElement":[
 {"@type":"ListItem","position":1,"item":{"@type":"Product","name":"Sony WH-1000XM5 Black","url":"https://shop.example/p/1","offers":{"@type":"Offer","price":"289.00","priceCurrency":"EUR"},"aggregateRating":{"ratingValue":"4.7","reviewCount":"321"}}},
 {"@type":"ListItem","position":2,"item":{"@type":"Product","name":"Sony WH-1000XM5 Silver","url":"https://shop.example/p/2","offers":{"@type":"Offer","price":"299.00","priceCurrency":"EUR"}}},
 {"@type":"ListItem","position":3,"item":{"@type":"Product","name":"Sony WH-1000XM4","url":"https://shop.example/p/3","offers":{"@type":"Offer","price":"199.00","priceCurrency":"EUR"}}}
]}</script></head><body><div id="app">Lädt…</div></body></html>"#;

const AMAZON_DP: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<h1 id="title">Sony WH-1000XM5</h1>
<div id="averageCustomerReviews"><span id="acrPopover"><span class="a-icon-alt">4,6 von 5 Sternen</span></span><span id="acrCustomerReviewText">12.345 Sternebewertungen</span></div>
<div id="merchantInfoFeature_feature_div"><span class="offer-display-feature-text-message">Amazon</span></div>
<div id="merchant-info">Verkauf und Versand durch Amazon.</div>
<div style="height:1600px"></div>
<h2>Kundenrezensionen</h2>
<div data-hook="review"><span data-hook="review-body"><span>Hervorragende Geräuschunterdrückung, perfekt für Pendler.</span></span></div>
<div data-hook="review"><span data-hook="review-body"><span>Klang sehr gut, Akku hält ewig.</span></span></div>
<div data-hook="review"><span data-hook="review-body"><span>Nach drei Wochen kaputt, Bügel gebrochen.</span></span></div>
</body></html>"#;

const JD_ITEM: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>.hidden{display:none}</style></head><body>
<div class="sku-name">索尼（SONY）WH-1000XM5</div>
<div class="J-hove-wrap"><div class="name"><a>索尼京东自营官方旗舰店</a></div><span class="ziying">自营</span></div>
<div class="score-parts">商品评价 9.6 物流履约 9.5 售后服务 9.4</div><div>开店 8 年 · 关注人数 12万</div>
<ul class="tabs"><li id="t1">商品介绍</li><li id="t2" onclick="document.getElementById('comments').className=''">商品评价(2万+)</li></ul>
<div id="comments" class="hidden">
 <div class="comment-percent"><strong class="percent-con">98%</strong></div>
 <div class="comment-item"><p class="comment-con">降噪效果非常好，地铁上很安静，正品无疑</p></div>
 <div class="comment-item"><p class="comment-con">佩戴舒适，音质好，京东物流很快</p></div>
 <div class="comment-item"><p class="comment-con">用了一个月右耳坏了，售后换了新的</p></div>
</div></body></html>"#;

const WALL: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>登录</title></head><body><div>亲，请登录</div><div>密码登录 短信登录</div></body></html>"#;

async fn read_request(sock: &mut tokio::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = sock.read(&mut tmp).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

async fn serve() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let head = read_request(&mut sock).await;
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = if path.starts_with("/amazon") {
                    AMAZON
                } else if path.starts_with("/jd") {
                    JD
                } else if path.starts_with("/taobao") {
                    TAOBAO
                } else if path.starts_with("/ebay") {
                    EBAY
                } else if path.starts_with("/ld") {
                    LD
                } else if path.starts_with("/dp/") {
                    AMAZON_DP
                } else if path.starts_with("/item/") {
                    JD_ITEM
                } else if path.starts_with("/wall") {
                    WALL
                } else {
                    "<html><body>nope</body></html>"
                };
                let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

fn plat(id: &str, port: u16, path: &str) -> Platform {
    let mut p = BUILTIN.iter().find(|p| p.id == id).unwrap().clone();
    p.search = format!("http://127.0.0.1:{port}/{path}?q={{q}}");
    p
}

async fn run(browser: &Arc<Browser>, p: &Platform) -> Vec<Candidate> {
    let notify = |s: &str, m: &str| println!("  [{s}] {m}");
    let cancel = CancellationToken::new();
    match search_platform(browser, p, &["Sony WH-1000XM5".into()], "relevance", SearchOpts { target: 10, max_rounds: 1, wait_user_secs: 0 }, &notify, &cancel).await {
        Outcome::Ok(c) => {
            for x in &c {
                println!("  {:<44} {:?} {} orig={:?} ship={:?} shop={:?} sales={:?} rating={:?} reviews={:?} badges={:?} loc={:?}",
                    x.title.chars().take(44).collect::<String>(), x.price, x.currency, x.original_price, x.shipping, x.shop, x.sales, x.rating, x.reviews, x.badges, x.location);
            }
            c
        }
        Outcome::Error(e) => panic!("{}: {e}", p.id),
        Outcome::LoginRequired(m) => panic!("{}: login {m}", p.id),
        Outcome::Cancelled => panic!("cancelled"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn platform_fixtures_extract_precisely() {
    let port = serve().await;
    let dir = std::env::temp_dir().join(format!("hawwwker-fx-{}", std::process::id()));
    let browser = Arc::new(Browser::new(dir.clone()));
    browser.configure(true, 2);

    println!("amazon_de:");
    let mut a = plat("amazon_de", port, "amazon");
    a.link = r"/dp/([A-Z0-9]{10})".into();
    let c = run(&browser, &a).await;
    assert_eq!(c.len(), 3);
    let main = c.iter().find(|x| x.external_id == "B09Y2MYL5C").unwrap();
    assert_eq!(main.title, "Sony WH-1000XM5 Kabellose Kopfhörer mit Noise Cancelling, Schwarz");
    assert_eq!((main.price, main.original_price, main.currency.as_str()), (Some(279.0), Some(419.0), "EUR"));
    assert_eq!((main.rating, main.reviews, main.sales), (Some(4.6), Some(12345), Some(1000)));
    assert_eq!(main.shipping, Some(0.0));
    assert!(main.badges.iter().any(|b| b == "Bestseller"), "{:?}", main.badges);
    let pads = c.iter().find(|x| x.external_id == "B0C1234567").unwrap();
    assert_eq!((pads.price, pads.shipping), (Some(19.99), Some(4.99)));

    println!("jd:");
    let c = run(&browser, &plat("jd", port, "jd")).await;
    assert_eq!(c.len(), 3);
    let main = c.iter().find(|x| x.external_id == "100012043978").unwrap();
    assert!(main.title.starts_with("索尼（SONY）WH-1000XM5"));
    assert_eq!((main.price, main.currency.as_str()), (Some(2299.0), "CNY"));
    assert_eq!((main.reviews, main.sales), (Some(20000), None));
    assert_eq!(main.shop.as_deref(), Some("索尼京东自营官方旗舰店"));
    assert!(main.badges.iter().any(|b| b == "自营"));

    println!("taobao:");
    let c = run(&browser, &plat("taobao", port, "taobao")).await;
    assert_eq!(c.len(), 3);
    let main = c.iter().find(|x| x.external_id == "711111111111").unwrap();
    assert_eq!(main.title, "Sony/索尼 WH-1000XM5 头戴式降噪蓝牙耳机");
    assert_eq!(main.price, Some(1799.0));
    assert_eq!(main.sales, Some(3000));
    assert_eq!(main.shop.as_deref(), Some("索尼官方旗舰店"));
    assert_eq!(main.location.as_deref(), Some("广东 深圳"));
    assert!(main.badges.iter().any(|b| b == "天猫"));
    assert_eq!(c.iter().find(|x| x.external_id == "733333333333").unwrap().price, Some(1650.5));

    println!("ebay_de:");
    let c = run(&browser, &plat("ebay_de", port, "ebay")).await;
    assert_eq!(c.len(), 3, "placeholder card dropped");
    let used = c.iter().find(|x| x.external_id == "314159265358").unwrap();
    assert_eq!(used.title, "Sony WH-1000XM5 Kopfhörer schwarz");
    assert_eq!((used.price, used.shipping, used.condition.as_str()), (Some(199.0), Some(4.99), "used"));
    assert!(used.badges.iter().any(|b| b == "Privat"));
    assert_eq!(used.shop.as_deref(), Some("kopfhoerer-fan"));
    let new = c.iter().find(|x| x.external_id == "271828182845").unwrap();
    assert_eq!((new.shipping, new.sales), (Some(0.0), Some(23)));

    println!("json-ld:");
    let mut ld = plat("shein", port, "ld");
    ld.link = String::new();
    let c = run(&browser, &ld).await;
    assert_eq!(c.len(), 3);
    assert_eq!((c[0].title.as_str(), c[0].price, c[0].currency.as_str()), ("Sony WH-1000XM5 Black", Some(289.0), "EUR"));
    assert_eq!((c[0].rating, c[0].reviews), (Some(4.7), Some(321)));

    println!("reviews:");
    let cancel = CancellationToken::new();
    let v = read_detail(&browser, &a, &format!("http://127.0.0.1:{port}/dp/B09Y2MYL5C"), &cancel).await.unwrap();
    let raw = reviews::parse_page(&v);
    println!("  amazon: {raw:?}");
    assert_eq!((raw.rating, raw.count), (Some(4.6), Some(12345)));
    assert_eq!(raw.texts.len(), 3);
    assert!(raw.seller_text.as_deref().unwrap_or("").contains("Verkauf und Versand durch Amazon"));
    let jd = plat("jd", port, "jd");
    let v = read_detail(&browser, &jd, &format!("http://127.0.0.1:{port}/item/100012043978"), &cancel).await.unwrap();
    let raw = reviews::parse_page(&v);
    println!("  jd: {raw:?}");
    assert_eq!(raw.good_rate, Some(98.0), "comments tab clicked");
    assert_eq!(raw.count, Some(20000));
    assert_eq!(raw.texts.len(), 3);
    assert_eq!(raw.shop_name.as_deref(), Some("索尼京东自营官方旗舰店"));
    assert_eq!(raw.shop_scores.get("服务"), Some(&4.7));
    assert_eq!(raw.shop_years, Some(8.0));
    let tb = plat("taobao", port, "taobao");
    let v = read_detail(&browser, &tb, &format!("http://127.0.0.1:{port}/wall"), &cancel).await.unwrap();
    let raw = reviews::parse_page(&v);
    assert!(raw.login_wall);
    assert_eq!(reviews::rule_insight(&raw).unavailable_reason.as_deref(), Some("需要登录才能查看评论"));

    browser.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}
