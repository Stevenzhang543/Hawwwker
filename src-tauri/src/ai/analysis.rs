//! v26.03 搜索链路新增的 AI 分析：商品风险（批量）与评论分析（批量）。
//! 每次调用都有超时；失败返回 None，由调用方使用规则结果。

use super::AiClient;
use crate::models::{RiskFactor, SearchPlan};
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

pub const RISK: &str = r#"你是电商平台的商品风险审核员。给定用户需求与若干候选商品（含规则已识别的风险因素），逐一评估"买到不满意商品"的风险：
- 正品风险：价格远低于同款正品中位价（refPrice）、标题含高仿/原单/A货/复刻/1:1/同款/平替、大牌商品却由个人或非授权店铺销售；
- 功能 / 规格风险：型号、代际、容量、版本、配件与需求不符，或缺少需求中的关键功能（mustHave）；
- 成色风险：二手、翻新、展示机、拆封；
- 店铺 / 渠道风险：个人卖家、代购、跨境无保修、新店；
- 信息不足：没有销量、评价或店铺信息。
对每个候选输出：adjust（在规则风险分基础上的调整，整数 -20~30；规则因素已覆盖的不要重复加分）、factors（规则没有覆盖的新因素，0~3 个，每个 {"kind": "counterfeit"|"price_anomaly"|"function"|"spec_mismatch"|"used"|"shop"|"no_reviews"|"overseas"|"other", "label": ≤16 字中文, "weight": -20~30}）、summary（≤36 字中文结论，例如"旗舰店正品，价格正常，风险低"）。
输出 JSON 对象 { "items": [ { "i", "adjust", "factors", "summary" } ] }，必须覆盖所有候选，不要编造输入里没有的事实。"#;

pub const REVIEW: &str = r#"你是电商评论分析师。给定用户需求与若干商品的评论摘录（商品评论，以及可能的店铺评分/评价），逐一分析：
- pros：评论里反复出现的优点（≤4 条，每条 ≤16 字）；
- cons：反复出现的缺点（≤4 条）；
- redFlags：严重风险信号（假货/非正品、二手翻新冒充新品、质量故障集中、售后拒绝退换、与描述不符），只写评论里确有依据的，≤3 条；
- matchNote：评论与用户需求的对应（例如"多条评论提到降噪效果好，符合你的通勤需求"），没有依据就为 null；
- score：0~100 的评论口碑分（综合评分、好评率、评论内容；样本很少时向 60 收缩）；
- shopNote：对店铺评分 / 店铺评价的一句话结论（≤30 字），没有店铺信息为 null。
只依据给定评论，不要编造。输出 JSON 对象 { "items": [ { "i", "score", "pros", "cons", "redFlags", "matchNote", "shopNote" } ] }，必须覆盖所有商品。"#;

#[derive(Debug, Clone, Default)]
pub struct RiskAi {
    pub adjust: i32,
    pub factors: Vec<RiskFactor>,
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReviewAi {
    pub score: Option<u32>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub red_flags: Vec<String>,
    pub match_note: Option<String>,
    pub shop_note: Option<String>,
}

fn strs(v: &Value, max: usize, len: usize) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().chars().take(len).collect::<String>()).filter(|s| !s.is_empty()).take(max).collect())
        .unwrap_or_default()
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn need(plan: &SearchPlan) -> Value {
    json!({
        "instruction": plan.instruction, "goal": plan.goal, "product": plan.product, "condition": plan.condition,
        "budget": {"min": plan.price_min, "max": plan.price_max, "currency": plan.currency},
    })
}

const AI_TIMEOUT: Duration = Duration::from_secs(90);

/// 批量风险分析。`items`：已序列化的候选（必须含 "i"）。返回 i → 结果；None = AI 不可用。
pub async fn risk(ai: &AiClient, plan: &SearchPlan, items: Vec<Value>) -> (Option<HashMap<usize, RiskAi>>, u32) {
    if items.is_empty() || ai.require().is_err() {
        return (None, 0);
    }
    let need = need(plan);
    let batches: Vec<Vec<Value>> = items.chunks(25).map(|c| c.to_vec()).collect();
    let calls = batches.len() as u32;
    let futs = batches.into_iter().map(|b| {
        let user = json!({"need": need, "candidates": b}).to_string();
        async move { tokio::time::timeout(AI_TIMEOUT, ai.chat_json::<Value>(RISK, &user, true)).await }
    });
    let mut out: HashMap<usize, RiskAi> = HashMap::new();
    let mut ok = false;
    for r in join_all(futs).await {
        let Ok(Ok(v)) = r else { continue };
        ok = true;
        for x in super::items_of(v) {
            let Some(i) = x["i"].as_u64().map(|i| i as usize) else { continue };
            let factors = x["factors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .take(3)
                        .filter_map(|f| {
                            let label: String = f["label"].as_str()?.trim().chars().take(24).collect();
                            (!label.is_empty()).then(|| RiskFactor {
                                kind: f["kind"].as_str().unwrap_or("other").chars().take(20).collect(),
                                label,
                                weight: num(&f["weight"]).unwrap_or(5.0).clamp(-20.0, 30.0) as i32,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.insert(
                i,
                RiskAi {
                    adjust: num(&x["adjust"]).unwrap_or(0.0).clamp(-20.0, 30.0) as i32,
                    factors,
                    summary: x["summary"].as_str().unwrap_or("").trim().chars().take(60).collect(),
                },
            );
        }
    }
    (ok.then_some(out), calls)
}

/// 批量评论分析。`items` 每项含 "i"、评论摘录与店铺信息。
pub async fn reviews(ai: &AiClient, plan: &SearchPlan, items: Vec<Value>) -> (Option<HashMap<usize, ReviewAi>>, u32) {
    if items.is_empty() || ai.require().is_err() {
        return (None, 0);
    }
    let need = need(plan);
    let batches: Vec<Vec<Value>> = items.chunks(6).map(|c| c.to_vec()).collect();
    let calls = batches.len() as u32;
    let futs = batches.into_iter().map(|b| {
        let user = json!({"need": need, "products": b}).to_string();
        async move { tokio::time::timeout(AI_TIMEOUT, ai.chat_json::<Value>(REVIEW, &user, true)).await }
    });
    let mut out: HashMap<usize, ReviewAi> = HashMap::new();
    let mut ok = false;
    for r in join_all(futs).await {
        let Ok(Ok(v)) = r else { continue };
        ok = true;
        for x in super::items_of(v) {
            let Some(i) = x["i"].as_u64().map(|i| i as usize) else { continue };
            out.insert(
                i,
                ReviewAi {
                    score: num(&x["score"]).map(|s| s.clamp(0.0, 100.0).round() as u32),
                    pros: strs(&x["pros"], 4, 24),
                    cons: strs(&x["cons"], 4, 24),
                    red_flags: strs(&x["redFlags"], 3, 30),
                    match_note: x["matchNote"].as_str().map(|s| s.trim().chars().take(60).collect::<String>()).filter(|s| !s.is_empty() && s != "null"),
                    shop_note: x["shopNote"].as_str().map(|s| s.trim().chars().take(40).collect::<String>()).filter(|s| !s.is_empty() && s != "null"),
                },
            );
        }
    }
    (ok.then_some(out), calls)
}
