//! 完整搜索管线端到端测试：模拟大模型（OpenAI 兼容）+ 模拟商城（自定义平台）+ 真实 Chromium。
//!   HAWWWKER_BROWSER=/path/to/chrome HAWWWKER_HEADLESS=1 cargo test --test pipeline_e2e -- --ignored --nocapture

use hawwwker_lib::models::{AiConfigInput, CustomPlatformInput, PlatformPatch, RunEventBody};
use hawwwker_lib::platforms;
use hawwwker_lib::search::pipeline;
use hawwwker_lib::state::AppState;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SHOP: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Shop</title><style>.p{font-size:22px;font-weight:700}</style></head><body>
<div><a href="/dp/B09Y2MYL5C"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B09Y2MYL5C">Sony WH-1000XM5 Kopfhörer Schwarz</a><div class="p">289,99 €</div></div>
<div><a href="/dp/B0C1234567"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0C1234567">Ohrpolster für Sony WH-1000XM5</a><div class="p">19,99 €</div></div>
<div><a href="/dp/B0E1111111"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0E1111111">Sony WH-1000XM5 Silber Neu</a><div class="p">305,00 €</div></div>
<div><a href="/dp/B0F2222222"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0F2222222">Sony WH-1000XM5 gebraucht</a><div class="p">180,00 €</div></div>
</body></html>"#;

async fn read_request(sock: &mut tokio::net::TcpStream) -> (String, String) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = sock.read(&mut tmp).await.unwrap_or(0);
        if n == 0 { break; }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            let len = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
            while buf.len() < pos + 4 + len {
                let n = sock.read(&mut tmp).await.unwrap_or(0);
                if n == 0 { break; }
                buf.extend_from_slice(&tmp[..n]);
            }
            let body = String::from_utf8_lossy(&buf[pos + 4..]).to_string();
            return (head, body);
        }
    }
    (String::new(), String::new())
}

fn llm_reply(body: &str, custom_id: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(json!({}));
    let sys = v["messages"][0]["content"].as_str().unwrap_or("");
    let user = v["messages"].as_array().and_then(|a| a.last()).and_then(|m| m["content"].as_str()).unwrap_or("").to_string();
    if sys.contains("购物搜索规划器") {
        json!({"goal":"compare","product":{"category":"降噪耳机","brand":"Sony","model":"WH-1000XM5","attributes":[],"mustHave":[],"mustNot":["耳罩","Ohrpolster"]},
               "keywords":["Sony WH-1000XM5"],"exclude":["gebraucht"],"priceMax":350,"currency":"EUR","condition":"new",
               "platforms":[custom_id],"queries":[{"platformId":custom_id,"q":"Sony WH-1000XM5","lang":"de"}],
               "sort":"price_asc","limit":20,"monitor":null,"notes":["按欧元比较"]}).to_string()
    } else if sys.contains("商品匹配审核员") {
        let u: Value = serde_json::from_str(&user).unwrap_or(json!({}));
        let out: Vec<Value> = u["candidates"].as_array().cloned().unwrap_or_default().iter().map(|c| {
            let t = c["title"].as_str().unwrap_or("");
            if t.contains("Ohrpolster") { json!({"i":c["i"],"score":10,"verdict":"mismatch","reasons":["是耳罩配件"],"flags":["accessory"],"productKey":""}) }
            else if t.contains("Silber") { json!({"i":c["i"],"score":80,"verdict":"close","reasons":["颜色不同"],"flags":[],"productKey":"sony wh-1000xm5"}) }
            else { json!({"i":c["i"],"score":95,"verdict":"exact","reasons":["型号一致"],"flags":[],"productKey":"sony wh-1000xm5"}) }
        }).collect();
        json!({"items": out}).to_string()
    } else if sys.contains("购物顾问") {
        let u: Value = serde_json::from_str(&user).unwrap_or(json!({}));
        let id = u["results"][0]["id"].as_str().unwrap_or("").to_string();
        json!({"headline":"最低 289.99 欧","bestPick":{"id":id,"why":"型号一致且最便宜"},"bullets":["两款在售"],"priceInsight":"价差约 5%","caveats":[],
               "nextActions":[{"type":"watch","label":"盯价","listingId":id,"targetPrice":260}]}).to_string()
    } else {
        "Hawwwker 已连接".to_string()
    }
}

async fn serve(custom_id: std::sync::Arc<std::sync::Mutex<String>>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            let cid = custom_id.lock().unwrap().clone();
            tokio::spawn(async move {
                let (head, body) = read_request(&mut sock).await;
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (ctype, out) = if path.starts_with("/v1/chat/completions") {
                    let content = llm_reply(&body, &cid);
                    ("application/json", json!({"choices":[{"message":{"role":"assistant","content":content}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}).to_string())
                } else if path.starts_with("/s?") {
                    ("text/html; charset=utf-8", SHOP.to_string())
                } else {
                    ("text/plain", "nope".to_string())
                };
                let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

#[test]
#[ignore]
fn full_pipeline_with_mock_ai() {
    tauri::async_runtime::block_on(async {
        let cid = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let port = serve(cid.clone()).await;
        let dir = std::env::temp_dir().join(format!("hawwwker-pipe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = AppState::new(dir.clone()).unwrap();

        // AI 未配置时必须拒绝
        assert!(pipeline::start(&st, None, "找耳机".into(), None, "manual", None).is_err());

        st.ai.save_config(AiConfigInput { protocol: "openai".into(), provider: "custom".into(), base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "mock".into(), fast_model: None, temperature: None, timeout_secs: None, max_concurrency: None, api_key: Some("sk-test".into()) }).unwrap();
        let status = st.ai.test().await;
        assert!(status.verified, "{:?}", status.last_error);

        // 只启用一个本地"商城"
        for p in platforms::all(&st.db) {
            platforms::update(&st.db, &p.id, PlatformPatch { enabled: Some(false), search_url: None }).unwrap();
        }
        let info = platforms::add_custom(&st.db, CustomPlatformInput { name: "MockShop".into(), region: "intl".into(), currency: "EUR".into(),
            locale: "de-DE".into(), search_url: format!("http://127.0.0.1:{port}/s?k={{q+}}"), link_pattern: Some(r"/dp/([A-Z0-9]{10})".into()), home: None, kind: None }).unwrap();
        *cid.lock().unwrap() = info.id.clone();

        let (h, provisional, rx) = pipeline::start(&st, None, "帮我找 Sony WH-1000XM5，350 欧以内，不要二手".into(), None, "manual", None).unwrap();
        println!("provisional platforms: {:?}", provisional.platforms);
        let outcome = rx.await.unwrap();
        for ev in h.since(0) {
            match &ev.body {
                RunEventBody::Stage { label, progress, .. } => println!("#{} stage {label} {progress:.2}", ev.seq),
                RunEventBody::Platform { platform_id, state, count, message, .. } => println!("#{} platform {platform_id} {state} {count:?} {message:?}", ev.seq),
                RunEventBody::Filter { fetched, kept, dropped, .. } => println!("#{} filter fetched={fetched} kept={kept} dropped={dropped:?}", ev.seq),
                RunEventBody::Verify { checked, matched, hidden } => println!("#{} verify {checked} {matched} {hidden}", ev.seq),
                RunEventBody::Summary { summary } => println!("#{} summary {}", ev.seq, summary.headline),
                RunEventBody::Done { stats } => println!("#{} done ai_calls={} elapsed={}ms", ev.seq, stats.ai_calls, stats.elapsed_ms),
                RunEventBody::Error { message } => println!("#{} ERROR {message}", ev.seq),
                other => println!("#{} {:?}", ev.seq, std::mem::discriminant(other)),
            }
        }
        assert_eq!(outcome.status, "done", "{:?}", outcome.error);
        for r in &outcome.results {
            println!("{:>2}. {:<40} {:?} {} → {:?} CNY? {} {:?} also={}", r.rank, r.title, r.price, r.currency, r.display_price, r.match_info.verdict, r.match_info.reasons, r.also_on.len());
        }
        // 二手被过滤；配件被标记为不相关并排在最后；精确匹配排第一（价格升序）
        assert!(outcome.results.iter().all(|r| !r.title.contains("gebraucht")));
        assert_eq!(outcome.results[0].match_info.verdict, "exact");
        assert_eq!(outcome.results.last().unwrap().match_info.verdict, "mismatch");
        assert!(outcome.best_price.is_some());
        // 回放：历史详情与事件补拉
        let detail = pipeline::run_detail(&st.db, &st.fx, &h.id, "CNY").unwrap();
        assert_eq!(detail.summary.status, "done");
        assert_eq!(detail.results.len(), outcome.results.len());
        assert!(detail.ai_summary.is_some());
        let seqs: Vec<u64> = h.since(0).iter().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "seq contiguous");
        st.browser.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    });
}
