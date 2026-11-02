//! 完整搜索管线端到端测试：模拟大模型（OpenAI 兼容）+ 模拟商城（自定义平台）+ 真实 Chromium。
//!   HAWWWKER_BROWSER=/path/to/chrome HAWWWKER_HEADLESS=1 cargo test --test pipeline_e2e -- --ignored --nocapture

use hawwwker_lib::models::{AiConfigInput, CustomPlatformInput, PlatformPatch, RunEventBody};
use hawwwker_lib::platforms;
use hawwwker_lib::search::pipeline;
use hawwwker_lib::state::AppState;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SHOP: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Shop</title><style>.p{font-size:22px;font-weight:700}</style></head><body>
<div><a href="/dp/B09Y2MYL5C"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B09Y2MYL5C">Sony WH-1000XM5 Kopfhörer Schwarz</a><div class="p">289,99 €</div><div>索尼官方旗舰店</div></div>
<div><a href="/dp/B0C1234567"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0C1234567">Ohrpolster für Sony WH-1000XM5</a><div class="p">19,99 €</div></div>
<div><a href="/dp/B0E1111111"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0E1111111">Sony WH-1000XM5 Silber Neu</a><div class="p">305,00 €</div><div>阿花的数码小店</div></div>
<div><a href="/dp/B0F2222222"><img width="100" height="100" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a><a href="/dp/B0F2222222">Sony WH-1000XM5 gebraucht</a><div class="p">180,00 €</div></div>
</body></html>"#;

const DETAIL_GOOD: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Sony WH-1000XM5</title>
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"Sony WH-1000XM5","aggregateRating":{"ratingValue":"4.7","reviewCount":"2311"},
"review":[{"reviewBody":"降噪效果非常好，通勤地铁上很安静"},{"reviewBody":"音质清晰，佩戴舒适，续航长"},{"reviewBody":"官方旗舰店发货快，包装完好是正品"}]}</script></head>
<body><h1>Sony WH-1000XM5</h1><div class="shop">店铺评分 描述 4.9 服务 4.8 物流 4.8 · 开店 6 年</div></body></html>"#;

const DETAIL_BAD: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Sony WH-1000XM5 Silber</title></head><body><h1>Sony WH-1000XM5 Silber</h1>
<div class="review-list"><div class="review-item">收到的是假货，做工很差</div><div class="review-item">怀疑不是正品，声音发闷</div><div class="review-item">客服态度差，不给退</div></div>
<div>4,0 von 5 · 58 Bewertungen</div></body></html>"#;

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
    } else if sys.contains("商品风险审核员") {
        let u: Value = serde_json::from_str(&user).unwrap_or(json!({}));
        let out: Vec<Value> = u["candidates"].as_array().cloned().unwrap_or_default().iter().map(|c| {
            let t = c["title"].as_str().unwrap_or("");
            if t.contains("Schwarz") { json!({"i":c["i"],"adjust":-3,"factors":[],"summary":"旗舰店正品，价格正常，风险低"}) }
            else { json!({"i":c["i"],"adjust":4,"factors":[{"kind":"shop","label":"小店销售大牌","weight":6}],"summary":"小店销售大牌商品，需确认渠道"}) }
        }).collect();
        json!({"items": out}).to_string()
    } else if sys.contains("评论分析师") {
        let u: Value = serde_json::from_str(&user).unwrap_or(json!({}));
        let out: Vec<Value> = u["products"].as_array().cloned().unwrap_or_default().iter().map(|c| {
            let t = c["title"].as_str().unwrap_or("");
            if t.contains("Schwarz") { json!({"i":c["i"],"score":91,"pros":["降噪好","佩戴舒适"],"cons":[],"redFlags":[],"matchNote":"多条评论提到通勤降噪效果好","shopNote":"店铺评分高"}) }
            else { json!({"i":c["i"],"score":38,"pros":[],"cons":["做工差"],"redFlags":["多条评论怀疑假货"],"matchNote":null,"shopNote":null}) }
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
                } else if path.starts_with("/dp/B09Y2MYL5C") {
                    ("text/html; charset=utf-8", DETAIL_GOOD.to_string())
                } else if path.starts_with("/dp/") {
                    ("text/html; charset=utf-8", DETAIL_BAD.to_string())
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
                RunEventBody::PlatformPlan { chosen, skipped, reason } => println!("#{} platform_plan {chosen:?} skipped={} {reason}", ev.seq, skipped.len()),
                RunEventBody::Funnel { stage, input, kept, note, .. } => println!("#{} funnel {stage} {input}→{kept} {note:?}", ev.seq),
                RunEventBody::Results { items } => println!("#{} results {}", ev.seq, items.len()),
                RunEventBody::Done { stats } => println!("#{} done ai_calls={} elapsed={}ms", ev.seq, stats.ai_calls, stats.elapsed_ms),
                RunEventBody::Error { message } => println!("#{} ERROR {message}", ev.seq),
                other => println!("#{} {:?}", ev.seq, std::mem::discriminant(other)),
            }
        }
        assert_eq!(outcome.status, "done", "{:?}", outcome.error);
        for r in &outcome.results {
            println!("{:>2}. {:<40} {:?} {} → {:?} CNY? {} {:?} also={}", r.rank, r.title, r.price, r.currency, r.display_price, r.match_info.verdict, r.match_info.reasons, r.also_on.len());
        }
        for r in &outcome.results {
            println!("    tags={:?} shop={:?} risk={:?} review={:?} final={:?}", r.tags.iter().map(|t| &t.label).collect::<Vec<_>>(),
                r.shop_info.as_ref().map(|s| (&s.label, s.credibility)), r.risk.as_ref().map(|x| (x.score, &x.level, &x.summary)),
                r.review_insight.as_ref().map(|v| (v.score, v.rating, v.count, &v.red_flags, &v.unavailable_reason)), r.final_score);
        }
        // v26.03：阶段顺序、平台判定、漏斗
        let mut stages: Vec<String> = vec![];
        for ev in h.since(0) {
            if let RunEventBody::Stage { stage, .. } = &ev.body {
                if stages.last() != Some(stage) {
                    stages.push(stage.clone());
                }
            }
        }
        assert_eq!(stages, ["plan", "platforms", "search", "screen", "risk", "shop", "reviews", "review_ai", "final", "summarize", "done"]);
        let evs = h.since(0);
        assert!(evs.iter().any(|e| matches!(&e.body, RunEventBody::PlatformPlan { chosen, .. } if chosen == &vec![cid.lock().unwrap().clone()])));
        let funnels: Vec<String> = evs.iter().filter_map(|e| if let RunEventBody::Funnel { stage, .. } = &e.body { Some(stage.clone()) } else { None }).collect();
        assert_eq!(funnels, ["search", "screen", "risk", "shop", "reviews", "review_ai", "final"]);
        assert!(evs.iter().filter(|e| matches!(e.body, RunEventBody::Results { .. })).count() >= 6, "progressive results");
        let best = outcome.results.iter().find(|r| r.title.contains("Schwarz")).unwrap();
        assert_eq!(best.rank, 1);
        assert_eq!(best.tags[0].label, "旗舰店");
        assert_eq!(best.shop_info.as_ref().unwrap().kind, "flagship");
        assert!(best.shop_info.as_ref().unwrap().scores.contains_key("服务"));
        assert_eq!(best.risk.as_ref().unwrap().level, "low");
        assert!(best.risk.as_ref().unwrap().ai_checked);
        let rv = best.review_insight.as_ref().unwrap();
        assert_eq!((rv.score, rv.rating, rv.count, rv.ai_checked), (Some(91), Some(4.7), Some(2311), true));
        assert_eq!(rv.match_note.as_deref(), Some("多条评论提到通勤降噪效果好"));
        let silver = outcome.results.iter().find(|r| r.title.contains("Silber")).unwrap();
        assert!(!silver.review_insight.as_ref().unwrap().red_flags.is_empty());
        assert!(silver.risk.as_ref().unwrap().score > best.risk.as_ref().unwrap().score + 20, "reviews and shop raise risk");
        assert!(best.final_score.unwrap() > silver.final_score.unwrap());
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
        let replay = detail.results.iter().find(|r| r.title.contains("Schwarz")).unwrap();
        assert_eq!((replay.final_score, replay.tags.first().map(|t| t.label.as_str())), (best.final_score, Some("旗舰店")));
        assert_eq!(replay.review_insight.as_ref().and_then(|v| v.score), Some(91));
        assert_eq!(replay.risk.as_ref().map(|r| r.level.as_str()), Some("low"));
        let seqs: Vec<u64> = h.since(0).iter().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "seq contiguous");
        st.browser.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    });
}
