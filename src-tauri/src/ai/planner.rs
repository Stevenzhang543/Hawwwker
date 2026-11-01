//! 搜索规划：规则预解析 + AI 规划 → 合并 → 规范化（平台必须存在且启用、每个平台都有搜索词）。

use super::{prompts, AiClient};
use crate::error::AppResult;
use crate::models::{AppSettings, MonitorHint, PlatformQuery, ProductSpec, SearchPlan};
use crate::platforms::Platform;
use crate::search::rules::{self, RuleHints};
use serde_json::{json, Value};

pub struct PlanCtx<'a> {
    pub platforms: &'a [Platform],
    pub settings: &'a AppSettings,
}

impl PlanCtx<'_> {
    fn enabled(&self) -> impl Iterator<Item = &Platform> {
        self.platforms.iter().filter(|p| p.enabled)
    }
    fn get(&self, id: &str) -> Option<&Platform> {
        self.platforms.iter().find(|p| p.id == id)
    }
}

fn s(v: &Value) -> Option<String> {
    match v {
        Value::String(x) if !x.trim().is_empty() && x.trim() != "null" => Some(x.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn f(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(x) => x.trim().replace([',', '€', '$', '¥', '£'], "").parse().ok(),
        _ => None,
    }
    .filter(|x: &f64| x.is_finite() && *x >= 0.0)
}

fn list(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(s).collect(),
        Value::String(x) if !x.trim().is_empty() => x.split([',', '，', '、']).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        _ => vec![],
    }
}

fn dedup(mut v: Vec<String>, max: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for x in v.drain(..) {
        let x: String = x.chars().take(60).collect();
        if !x.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&x)) {
            out.push(x);
        }
        if out.len() >= max {
            break;
        }
    }
    out
}

fn one_of(v: Option<String>, allowed: &[&str], default: &str) -> String {
    v.filter(|x| allowed.contains(&x.as_str())).unwrap_or_else(|| default.to_string())
}

/// 纯规则计划（AI 不可用时的兜底，也用于即时预览）。
pub fn rule_plan(instruction: &str, h: &RuleHints, ctx: &PlanCtx) -> SearchPlan {
    let kw = if h.remainder.trim().is_empty() { instruction.trim().to_string() } else { h.remainder.trim().to_string() };
    let mut plan = SearchPlan {
        instruction: instruction.trim().to_string(),
        goal: h.goal.clone().unwrap_or_else(|| "buy".into()),
        product: ProductSpec { category: kw.clone(), ..Default::default() },
        keywords: vec![kw],
        exclude: h.exclude.clone(),
        price_min: h.price_min,
        price_max: h.price_max,
        currency: h.currency.clone().unwrap_or_else(|| ctx.settings.display_currency.clone()),
        condition: h.condition.clone().unwrap_or_else(|| "any".into()),
        platforms: h.platforms.clone(),
        sort: h.sort.clone().unwrap_or_else(|| "relevance".into()),
        limit: h.limit.unwrap_or(ctx.settings.search.result_limit),
        monitor: h.monitor_minutes.map(|m| MonitorHint { interval_minutes: m, target_price: h.target_price }),
        parser: "rules".into(),
        ..Default::default()
    };
    if plan.product.category.chars().count() > 20 {
        plan.product.category = plan.product.category.chars().take(20).collect();
    }
    normalize(plan, ctx, h.region.as_deref())
}

/// AI 规划；模型异常时退回规则计划并在 notes 中说明。
pub async fn plan(ai: &AiClient, instruction: &str, ctx: &PlanCtx<'_>) -> AppResult<SearchPlan> {
    ai.require()?;
    let h = rules::parse(instruction);
    let catalog: Vec<Value> = ctx
        .enabled()
        .map(|p| json!({"id": p.id, "name": format!("{} {}", p.name, p.edition), "region": p.region, "lang": p.locale, "currency": p.currency, "kind": p.kind}))
        .collect();
    let region_pref = h.region.clone().unwrap_or_else(|| ctx.settings.search.default_region.clone());
    let user = json!({
        "instruction": instruction,
        "defaultCurrency": ctx.settings.display_currency,
        "userRole": ctx.settings.role,
        "regionPreference": region_pref,
        "availablePlatforms": catalog,
        "ruleHints": {
            "priceMin": h.price_min, "priceMax": h.price_max, "currency": h.currency, "platforms": h.platforms,
            "exclude": h.exclude, "condition": h.condition, "monitorMinutes": h.monitor_minutes, "targetPrice": h.target_price,
        },
    })
    .to_string();
    let v: Value = match ai.chat_json(prompts::PLANNER, &user, false).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("AI planner failed: {}", e.message);
            let mut p = rule_plan(instruction, &h, ctx);
            p.notes.insert(0, format!("AI 规划暂时不可用（{}），已按规则理解指令", e.message.chars().take(40).collect::<String>()));
            return Ok(p);
        }
    };
    Ok(merge(instruction, &h, &v, ctx))
}

/// 合并 AI 输出与规则结果：规则数值优先；平台以用户点名为准。
pub fn merge(instruction: &str, h: &RuleHints, v: &Value, ctx: &PlanCtx) -> SearchPlan {
    let prod = &v["product"];
    let mut plan = SearchPlan {
        instruction: instruction.trim().to_string(),
        goal: one_of(s(&v["goal"]).or(h.goal.clone()), &["buy", "compare", "track", "research", "gift", "source"], "buy"),
        product: ProductSpec {
            category: s(&prod["category"]).unwrap_or_default(),
            brand: s(&prod["brand"]),
            model: s(&prod["model"]),
            attributes: dedup(list(&prod["attributes"]), 8),
            must_have: dedup(list(&prod["mustHave"]), 8),
            must_not: dedup(list(&prod["mustNot"]), 12),
        },
        keywords: dedup(list(&v["keywords"]), 4),
        exclude: dedup([h.exclude.clone(), list(&v["exclude"])].concat(), 20),
        price_min: h.price_min.or_else(|| f(&v["priceMin"])),
        price_max: h.price_max.or_else(|| f(&v["priceMax"])),
        currency: h.currency.clone().or_else(|| s(&v["currency"])).unwrap_or_else(|| ctx.settings.display_currency.clone()),
        condition: one_of(h.condition.clone().or_else(|| s(&v["condition"])), &["any", "new", "used"], "any"),
        platforms: if h.platforms.is_empty() { list(&v["platforms"]) } else { h.platforms.clone() },
        queries: v["queries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|q| {
                        Some(PlatformQuery {
                            platform_id: s(&q["platformId"])?,
                            q: s(&q["q"])?,
                            lang: s(&q["lang"]).unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        sort: one_of(h.sort.clone().or_else(|| s(&v["sort"])), &["relevance", "price_asc", "price_desc", "sales", "rating", "recent"], "relevance"),
        limit: h.limit.or_else(|| f(&v["limit"]).map(|x| x as u32)).unwrap_or(ctx.settings.search.result_limit),
        monitor: if h.monitor_minutes.is_some() {
            Some(MonitorHint { interval_minutes: h.monitor_minutes.unwrap_or(360), target_price: h.target_price.or_else(|| f(&v["monitor"]["targetPrice"])) })
        } else if v["monitor"].is_object() {
            Some(MonitorHint {
                interval_minutes: f(&v["monitor"]["intervalMinutes"]).map(|x| x as u32).unwrap_or(360),
                target_price: f(&v["monitor"]["targetPrice"]),
            })
        } else {
            None
        },
        notes: dedup(list(&v["notes"]), 3),
        parser: "ai+rules".into(),
    };
    if plan.keywords.is_empty() {
        let fallback = [plan.product.brand.clone(), plan.product.model.clone()].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let kw = if !fallback.trim().is_empty() { fallback } else if !h.remainder.trim().is_empty() { h.remainder.clone() } else { instruction.to_string() };
        plan.keywords = vec![kw];
    }
    if plan.product.category.is_empty() {
        plan.product.category = plan.keywords[0].clone();
    }
    normalize(plan, ctx, h.region.as_deref())
}

/// 规范化：平台存在且启用（用户点名但未启用的也允许，提示）；每个平台至少一条搜索词；数值边界。
pub fn normalize(mut plan: SearchPlan, ctx: &PlanCtx, region: Option<&str>) -> SearchPlan {
    plan.currency = plan.currency.trim().to_uppercase();
    if plan.currency.len() != 3 {
        plan.currency = ctx.settings.display_currency.clone();
    }
    if let (Some(a), Some(b)) = (plan.price_min, plan.price_max) {
        if a > b {
            plan.price_min = Some(b);
            plan.price_max = Some(a);
        }
    }
    plan.limit = plan.limit.clamp(5, 120);
    let mut chosen: Vec<String> = vec![];
    let mut disabled_named: Vec<String> = vec![];
    for id in plan.platforms.iter() {
        if chosen.contains(id) {
            continue;
        }
        match ctx.get(id) {
            Some(p) if p.enabled => chosen.push(id.clone()),
            Some(p) => {
                // 用户点名的平台即使未启用也执行，但提示
                disabled_named.push(p.name.clone());
                chosen.push(id.clone());
            }
            None => {}
        }
    }
    if chosen.is_empty() {
        let region = region.unwrap_or(&ctx.settings.search.default_region);
        chosen = ctx
            .enabled()
            .filter(|p| region == "all" || p.region == region)
            .filter(|p| p.kind != "social" || plan.goal == "research" || plan.goal == "gift")
            .map(|p| p.id.clone())
            .collect();
        if chosen.is_empty() {
            chosen = ctx.enabled().map(|p| p.id.clone()).collect();
        }
    }
    chosen.truncate(12);
    if !disabled_named.is_empty() {
        plan.notes.push(format!("{} 在平台目录中未启用，本次仍按你的要求搜索", disabled_named.join("、")));
    }
    plan.platforms = chosen;
    // 搜索词：去掉不在本次平台里的；为缺失的平台补关键词
    let max_q = ctx.settings.search.max_queries.clamp(1, 3) as usize;
    let mut queries: Vec<PlatformQuery> = vec![];
    for pid in plan.platforms.iter() {
        let mut mine: Vec<PlatformQuery> = plan
            .queries
            .iter()
            .filter(|q| &q.platform_id == pid && !q.q.trim().is_empty())
            .cloned()
            .collect();
        mine.dedup_by(|a, b| a.q.eq_ignore_ascii_case(&b.q));
        if mine.is_empty() {
            let lang = ctx.get(pid).map(|p| p.locale.clone()).unwrap_or_default();
            for kw in plan.keywords.iter().take(max_q) {
                mine.push(PlatformQuery { platform_id: pid.clone(), q: kw.clone(), lang: lang.clone() });
            }
        }
        mine.truncate(max_q);
        for q in mine.iter_mut() {
            q.q = q.q.chars().take(80).collect::<String>().trim().to_string();
        }
        queries.extend(mine);
    }
    plan.queries = queries;
    if let Some(m) = plan.monitor.as_mut() {
        m.interval_minutes = m.interval_minutes.clamp(30, 10080);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::BUILTIN;

    fn ctx_settings() -> AppSettings {
        AppSettings::default()
    }

    #[test]
    fn merge_prefers_rule_numbers_and_named_platforms() {
        let settings = ctx_settings();
        let ctx = PlanCtx { platforms: &BUILTIN, settings: &settings };
        let instr = "帮我在亚马逊德国找 Sony WH-1000XM5，200 欧以内，不要二手";
        let h = rules::parse(instr);
        let ai = json!({
            "goal": "compare",
            "product": {"category": "降噪耳机", "brand": "Sony", "model": "WH-1000XM5", "attributes": [], "mustHave": [], "mustNot": ["耳机套"]},
            "keywords": ["Sony WH-1000XM5"], "exclude": ["gebraucht", "refurbished"],
            "priceMax": 250, "currency": "EUR", "platforms": ["amazon_de", "taobao"],
            "queries": [{"platformId": "amazon_de", "q": "Sony WH-1000XM5", "lang": "de"}, {"platformId": "taobao", "q": "索尼", "lang": "zh"}],
            "sort": "price_asc", "limit": 30, "monitor": null, "notes": ["按欧元比价"]
        });
        let p = merge(instr, &h, &ai, &ctx);
        assert_eq!(p.price_max, Some(200.0), "rule price wins");
        assert_eq!(p.currency, "EUR");
        assert_eq!(p.platforms, vec!["amazon_de".to_string()], "user-named platform wins");
        assert_eq!(p.queries.len(), 1);
        assert_eq!(p.queries[0].q, "Sony WH-1000XM5");
        assert!(p.exclude.contains(&"gebraucht".to_string()) && p.exclude.contains(&"二手".to_string()));
        assert_eq!(p.condition, "new");
        assert_eq!(p.sort, "price_asc");
    }

    #[test]
    fn fills_queries_and_defaults() {
        let settings = ctx_settings();
        let ctx = PlanCtx { platforms: &BUILTIN, settings: &settings };
        let h = rules::parse("国际上找露营灯");
        let p = merge("国际上找露营灯", &h, &json!({"keywords": ["camping lantern"], "platforms": []}), &ctx);
        assert!(!p.platforms.is_empty());
        assert!(p.platforms.iter().all(|id| BUILTIN.iter().find(|b| &b.id == id).unwrap().region == "intl"));
        assert!(p.platforms.iter().all(|id| p.queries.iter().any(|q| &q.platform_id == id)));
    }

    #[test]
    fn garbage_ai_output_still_yields_plan() {
        let settings = ctx_settings();
        let ctx = PlanCtx { platforms: &BUILTIN, settings: &settings };
        let h = rules::parse("淘宝搜蓝牙耳机");
        let p = merge("淘宝搜蓝牙耳机", &h, &json!({"priceMin": "abc", "limit": -5, "queries": "x"}), &ctx);
        assert_eq!(p.platforms, vec!["taobao".to_string()]);
        assert!(!p.keywords.is_empty() && p.keywords[0].contains("蓝牙耳机"));
        assert!(p.limit >= 5);
    }
}
