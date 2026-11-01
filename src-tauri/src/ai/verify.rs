//! 搜索链路中的 AI：卡片抽取（低置信卡片）、相关性校验（带缓存）、结果总结。

use super::{prompts, AiClient};
use crate::db::Db;
use crate::error::AppResult;
use crate::fx::Fx;
use crate::models::{AiSummary, BestPick, CopilotAction, ResultItem, SearchPlan};
use crate::search::filter::price_in;
use crate::search::parse::Candidate;
use futures_util::future::join_all;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Verdict {
    pub score: u32,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub flags: Vec<String>,
    pub product_key: String,
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

fn strs(v: &Value, max: usize) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().chars().take(80).collect::<String>()).filter(|s| !s.is_empty()).take(max).collect())
        .unwrap_or_default()
}

/// 对规则解析把握不足的卡片调用快速模型抽取；返回调用次数。
pub async fn extract(ai: &AiClient, platform_label: &str, cands: &mut [Candidate]) -> u32 {
    let idx: Vec<usize> = cands
        .iter()
        .enumerate()
        .filter(|(_, c)| (c.confidence < 0.7 || c.price.is_none()) && c.text.chars().count() >= 8)
        .map(|(i, _)| i)
        .collect();
    if idx.is_empty() {
        return 0;
    }
    let mut calls = 0;
    for chunk in idx.chunks(25) {
        let cards: Vec<Value> = chunk.iter().map(|&i| json!({"i": i, "text": cands[i].text})).collect();
        let user = json!({"platform": platform_label, "cards": cards}).to_string();
        calls += 1;
        let Ok(resp) = ai.chat_json::<Value>(prompts::EXTRACT, &user, true).await else { continue };
        for v in super::items_of(resp) {
            let Some(i) = v["i"].as_u64().map(|x| x as usize).filter(|i| chunk.contains(i)) else { continue };
            let c = &mut cands[i];
            if let Some(t) = v["title"].as_str().filter(|t| t.chars().count() >= 4) {
                if c.confidence < 0.7 {
                    c.title = t.chars().take(300).collect();
                }
            }
            if c.price.is_none() {
                if let Some(p) = num(&v["price"]).filter(|p| *p > 0.0) {
                    c.price = Some(p);
                    if let Some(cur) = v["currency"].as_str().filter(|s| s.len() == 3) {
                        c.currency = cur.to_uppercase();
                    }
                }
            }
            if c.sales.is_none() {
                c.sales = num(&v["sales"]).map(|x| x as i64);
            }
            if c.rating.is_none() {
                c.rating = num(&v["rating"]).filter(|r| *r <= 5.0);
            }
            if c.shop.is_none() {
                c.shop = v["shop"].as_str().map(|s| s.to_string()).filter(|s| !s.is_empty());
            }
            if c.condition == "unknown" {
                if let Some(cond) = v["condition"].as_str().filter(|s| *s == "new" || *s == "used") {
                    c.condition = cond.into();
                }
            }
            c.confidence = c.confidence.max(0.75);
        }
    }
    calls
}

fn plan_fingerprint(plan: &SearchPlan) -> String {
    let v = json!([plan.goal, plan.product, plan.keywords, plan.exclude, plan.condition, plan.price_min, plan.price_max, plan.currency]);
    hex::encode(Sha256::digest(v.to_string().as_bytes()))
}

fn cache_key(fp: &str, c: &Candidate) -> String {
    let raw = format!("{fp}|{}|{}|{:?}|{}|{}", c.platform_id, c.title, c.price, c.currency, c.condition);
    format!("v1:{}", hex::encode(&Sha256::digest(raw.as_bytes())[..16]))
}

/// AI 相关性校验。返回与 cands 对齐的结果（None = 未校验）与调用次数。
pub async fn verify(ai: &AiClient, db: &Db, fx: &Fx, plan: &SearchPlan, cands: &[Candidate]) -> (Vec<Option<Verdict>>, u32) {
    let fp = plan_fingerprint(plan);
    let mut out: Vec<Option<Verdict>> = vec![None; cands.len()];
    let mut todo: Vec<usize> = vec![];
    for (i, c) in cands.iter().enumerate() {
        match db.cache_get(&cache_key(&fp, c), 7).and_then(|s| serde_json::from_str::<Verdict>(&s).ok()) {
            Some(v) => out[i] = Some(v),
            None => todo.push(i),
        }
    }
    if todo.is_empty() {
        return (out, 0);
    }
    let need = json!({
        "goal": plan.goal, "product": plan.product, "keywords": plan.keywords, "exclude": plan.exclude,
        "condition": plan.condition, "priceMin": plan.price_min, "priceMax": plan.price_max, "currency": plan.currency,
        "instruction": plan.instruction,
    });
    let batches: Vec<Vec<usize>> = todo.chunks(30).map(|c| c.to_vec()).collect();
    let calls = batches.len() as u32;
    let futs = batches.into_iter().map(|batch| {
        let need = need.clone();
        let items: Vec<Value> = batch
            .iter()
            .map(|&i| {
                let c = &cands[i];
                json!({
                    "i": i, "platform": c.platform_id, "title": c.title, "price": c.price, "currency": c.currency,
                    "priceInNeedCurrency": price_in(c, &plan.currency, fx), "shop": c.shop, "condition": c.condition,
                    "sponsored": c.is_ad,
                })
            })
            .collect();
        async move {
            let user = json!({"need": need, "candidates": items}).to_string();
            (batch, ai.chat_json::<Value>(prompts::VERIFY, &user, true).await.map(super::items_of))
        }
    });
    for (batch, res) in join_all(futs).await {
        let Ok(arr) = res else { continue };
        for v in arr {
            let Some(i) = v["i"].as_u64().map(|x| x as usize).filter(|i| batch.contains(i)) else { continue };
            let verdict = v["verdict"].as_str().unwrap_or("related");
            let verdict = if ["exact", "close", "related", "mismatch"].contains(&verdict) { verdict } else { "related" };
            let score = num(&v["score"]).unwrap_or(50.0).clamp(0.0, 100.0) as u32;
            let vd = Verdict {
                score,
                verdict: verdict.into(),
                reasons: strs(&v["reasons"], 2),
                flags: strs(&v["flags"], 5),
                product_key: v["productKey"].as_str().unwrap_or("").trim().to_lowercase(),
            };
            db.cache_put(&cache_key(&fp, &cands[i]), &serde_json::to_string(&vd).unwrap_or_default());
            out[i] = Some(vd);
        }
    }
    (out, calls)
}

/// 结果总结（结构化 JSON）。
pub async fn summarize(ai: &AiClient, plan: &SearchPlan, items: &[ResultItem], display_currency: &str) -> AppResult<AiSummary> {
    let top: Vec<Value> = items
        .iter()
        .filter(|r| r.match_info.verdict != "mismatch")
        .take(20)
        .map(|r| {
            json!({
                "id": r.id, "platform": r.platform_id, "title": r.title, "price": r.price, "currency": r.currency,
                "displayPrice": r.display_price, "verdict": r.match_info.verdict, "score": r.match_info.score,
                "flags": r.match_info.flags, "shop": r.shop, "rating": r.rating, "sales": r.sales, "condition": r.condition,
                "alsoOnCount": r.also_on.len(),
            })
        })
        .collect();
    let prices: Vec<f64> = items.iter().filter(|r| r.match_info.verdict == "exact" || r.match_info.verdict == "close").filter_map(|r| r.display_price).collect();
    let stats = if prices.is_empty() {
        json!(null)
    } else {
        let mut p = prices.clone();
        p.sort_by(|a, b| a.total_cmp(b));
        json!({"min": p[0], "median": p[p.len() / 2], "max": p[p.len() - 1], "count": p.len()})
    };
    let user = json!({
        "instruction": plan.instruction, "goal": plan.goal, "product": plan.product, "budget": {"min": plan.price_min, "max": plan.price_max, "currency": plan.currency},
        "displayCurrency": display_currency, "priceStats": stats, "results": top,
    })
    .to_string();
    let v: Value = ai.chat_json(prompts::SUMMARY, &user, false).await?;
    let ids: Vec<&str> = items.iter().map(|r| r.id.as_str()).collect();
    let best = v["bestPick"]["id"].as_str().filter(|id| ids.contains(id)).map(|id| BestPick {
        id: id.to_string(),
        why: v["bestPick"]["why"].as_str().unwrap_or("").chars().take(120).collect(),
    });
    let mut actions: Vec<CopilotAction> = vec![];
    if let Some(arr) = v["nextActions"].as_array() {
        for a in arr.iter().take(3) {
            if let Ok(act) = serde_json::from_value::<CopilotAction>(a.clone()) {
                let ok = match &act {
                    CopilotAction::Watch { listing_id, .. } => ids.contains(&listing_id.as_str()),
                    CopilotAction::Search { instruction, .. } => !instruction.trim().is_empty(),
                    _ => false,
                };
                if ok {
                    actions.push(act);
                }
            }
        }
    }
    Ok(AiSummary {
        headline: v["headline"].as_str().unwrap_or("").chars().take(80).collect(),
        best_pick: best,
        bullets: strs(&v["bullets"], 4),
        price_insight: v["priceInsight"].as_str().unwrap_or("").chars().take(160).collect(),
        caveats: strs(&v["caveats"], 3),
        next_actions: actions,
    })
}
