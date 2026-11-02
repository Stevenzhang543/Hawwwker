//! 商品风险（v26.03）：规则因素 + AI 调整 + 店铺 / 评论修正；最终细筛综合分；结果标签。
//!
//! 风险分 0–100，越高越危险：基准 15 分，加上各因素权重后截断；< 35 低、35–64 中、≥ 65 高。
//! 综合分（final_score）权重：匹配 40% · 风险（100 − 风险分）20% · 店铺可信度 15% · 评论 15% · 价格 10%；
//! 某项缺失（例如没读评论）时按其余权重重新归一。

use crate::models::{DealInfo, ItemTag, MatchInfo, ReviewInsight, RiskFactor, RiskInfo, SearchPlan, ShopInfo};
use crate::search::parse::Candidate;
use regex::Regex;
use std::sync::LazyLock;

pub const BASE: i32 = 15;

/// 高仿 / 非正品信号（标题）。
static FAKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)高仿|原单|A货|复刻|1\s*[:：比]\s*1|超A|精仿|仿品|仿真款|山寨|尾单|代工厂|工厂货|大牌平替|\breplica\b|\bfake\b|\bknock-?off\b|\bdupe\b|inspired by|nachbildung|fälschung").unwrap()
});
/// "同款 / 平替 / 适用于"——可能不是目标品牌本身。
static ALIKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)同款|平替|类似|仿.{0,2}款|适用于|适配|兼容|for\s+(?:sony|apple|iphone|samsung|bose|dyson|nintendo)|compatible with|kompatibel mit|passend für").unwrap()
});
static OVERSEAS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)代购|海外直邮|港版|日版|美版|欧版|水货|平行进口|grey import|import version").unwrap());

pub fn level_of(score: u32) -> &'static str {
    match score {
        0..=34 => "low",
        35..=64 => "medium",
        _ => "high",
    }
}

fn push(f: &mut Vec<RiskFactor>, kind: &str, label: impl Into<String>, weight: i32) {
    let label = label.into();
    if !f.iter().any(|x| x.kind == kind && x.label == label) {
        f.push(RiskFactor { kind: kind.into(), label, weight });
    }
}

/// 用户是否强调正品 / 官方渠道。
pub fn wants_authentic(plan: &SearchPlan) -> bool {
    let blob = format!("{} {} {}", plan.instruction, plan.product.must_have.join(" "), plan.notes.join(" ")).to_lowercase();
    ["正品", "官方", "行货", "国行", "authentic", "genuine", "original", "offiziell"].iter().any(|w| blob.contains(w))
}

/// 规则风险因素（不含店铺、评论，那两项在各自阶段加入）。
/// `ref_price`：同款精确 / 接近匹配商品的到手价中位数（显示币种）；`price`：本商品到手价（显示币种）。
pub fn rule_factors(c: &Candidate, m: &MatchInfo, plan: &SearchPlan, price: Option<f64>, ref_price: Option<f64>) -> Vec<RiskFactor> {
    let mut f = vec![];
    let title = &c.title;
    let branded = plan.product.brand.is_some() || plan.product.model.is_some();
    if FAKE_RE.is_match(title) {
        push(&mut f, "counterfeit", "标题含高仿 / 原单等字样", 40);
    } else if ALIKE_RE.is_match(title) && branded {
        push(&mut f, "counterfeit", "标注同款 / 平替 / 适配，可能不是正品本身", 18);
    }
    if let (Some(p), Some(r)) = (price, ref_price) {
        if r > 0.0 && m.verdict != "mismatch" {
            let ratio = p / r;
            if ratio < 0.45 && (branded || m.verdict == "exact") {
                push(&mut f, "price_anomaly", format!("价格仅为同款中位价的 {:.0}%，疑似非正品", ratio * 100.0), 32);
            } else if ratio < 0.65 && (branded || m.verdict == "exact") {
                push(&mut f, "price_anomaly", format!("价格明显低于同款（中位价的 {:.0}%）", ratio * 100.0), 14);
            }
        }
    }
    let flags: Vec<&str> = m.flags.iter().map(|s| s.as_str()).collect();
    if c.condition == "used" || flags.contains(&"used") {
        push(&mut f, "used", "二手商品", if plan.condition == "new" { 25 } else { 8 });
    }
    if flags.contains(&"refurbished") {
        push(&mut f, "refurbished", "翻新 / 官翻", 12);
    }
    if flags.contains(&"accessory") {
        push(&mut f, "accessory", "配件，不是商品本身", 30);
    }
    if flags.contains(&"different_model") {
        push(&mut f, "spec_mismatch", "型号 / 代际与需求不同", 15);
    }
    if flags.contains(&"price_suspicious") {
        push(&mut f, "price_anomaly", "AI 认为价格可疑", 15);
    }
    if OVERSEAS_RE.is_match(title) {
        push(&mut f, "overseas", "代购 / 非国行版本，保修与售后需确认", 10);
    }
    let no_sales = c.sales.is_none_or(|s| s == 0) && c.reviews.is_none_or(|r| r == 0) && c.rating.is_none();
    if no_sales && !["c2c", "compare", "social"].contains(&kind_of_platform(&c.platform_id)) {
        push(&mut f, "no_reviews", "没有销量与评价信息", 6);
    }
    f
}

fn kind_of_platform(id: &str) -> &'static str {
    match id {
        "xianyu" | "kleinanzeigen" | "mercari_jp" => "c2c",
        "idealo" => "compare",
        "xiaohongshu" | "rednote" | "douyin" | "tiktok" | "weibo" | "reddit" => "social",
        _ => "retail",
    }
}

/// 店铺带来的风险修正。
pub fn shop_factor(shop: &ShopInfo, authentic: bool) -> Option<RiskFactor> {
    let mul = if authentic { 1.4 } else { 1.0 };
    let (w, label) = match shop.kind.as_str() {
        "self_operated" => (-12, "平台自营，渠道可靠"),
        "flagship" => (-10, "品牌旗舰店"),
        "official" => (-10, "官方店铺"),
        "authorized" => (-5, "品牌专卖 / 授权店"),
        "distributor" => (2, "多品牌专营店"),
        "marketplace" => (4, "第三方卖家"),
        "enterprise" => (2, "企业卖家"),
        "studio" => (6, "小工作室，品控差异较大"),
        "overseas" => (10, "海外 / 代购渠道，售后与真伪需确认"),
        "secondhand" => (8, "二手商家"),
        "individual" => (12, "个人卖家，无官方售后"),
        _ => (5, "店铺信息不足"),
    };
    let w = if w > 0 { (w as f64 * mul).round() as i32 } else { w };
    let mut w = w;
    if shop.credibility < 45 && shop.kind != "individual" {
        w += 6;
    }
    Some(RiskFactor { kind: "shop".into(), label: label.into(), weight: w })
}

/// 评论带来的风险修正。
pub fn review_factors(r: &ReviewInsight) -> Vec<RiskFactor> {
    let mut f = vec![];
    for (i, flag) in r.red_flags.iter().take(3).enumerate() {
        push(&mut f, "reviews", format!("评论风险：{}", flag.chars().take(20).collect::<String>()), if i == 0 { 12 } else { 6 });
    }
    if let Some(s) = r.score {
        if s < 55 {
            push(&mut f, "reviews", "评论整体偏负面", 12);
        } else if s >= 85 {
            push(&mut f, "reviews", "评论口碑好", -8);
        }
    }
    if r.good_rate.is_some_and(|g| g < 90.0) {
        push(&mut f, "reviews", format!("好评率 {:.0}%", r.good_rate.unwrap_or_default()), 8);
    }
    f
}

/// 由因素与 AI 调整计算风险。
pub fn compose(factors: Vec<RiskFactor>, ai_adjust: i32, ai_summary: Option<String>, ai_checked: bool) -> RiskInfo {
    let sum: i32 = factors.iter().map(|f| f.weight).sum::<i32>() + ai_adjust.clamp(-25, 30);
    let score = (BASE + sum).clamp(0, 100) as u32;
    let level = level_of(score).to_string();
    let summary = ai_summary.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        let mut top: Vec<&RiskFactor> = factors.iter().filter(|f| f.weight > 0).collect();
        top.sort_by_key(|f| std::cmp::Reverse(f.weight));
        if top.is_empty() {
            "未发现明显风险".to_string()
        } else {
            top.iter().take(2).map(|f| f.label.as_str()).collect::<Vec<_>>().join("；")
        }
    });
    RiskInfo { score, level, factors, summary, ai_checked }
}

/// 在已有风险上追加因素（店铺 / 评论阶段），保留 AI 调整量。
pub fn add_factors(r: &RiskInfo, extra: Vec<RiskFactor>, ai_adjust: i32) -> RiskInfo {
    let mut factors = r.factors.clone();
    for f in extra {
        if !factors.iter().any(|x| x.kind == f.kind && x.label == f.label) {
            factors.push(f);
        }
    }
    let keep_summary = r.ai_checked.then(|| r.summary.clone());
    compose(factors, ai_adjust, keep_summary, r.ai_checked)
}

/// 价格分：价格判断（deal）与相对同款中位价的位置各占一半。
pub fn price_score(deal: Option<&DealInfo>, price: Option<f64>, ref_price: Option<f64>) -> Option<f64> {
    let d = deal.map(|d| match d.level.as_str() {
        "lowest" => 100.0,
        "low" => 85.0,
        "normal" => 65.0,
        "high" => 40.0,
        _ => 60.0,
    });
    let rel = match (price, ref_price) {
        (Some(p), Some(r)) if r > 0.0 => Some((80.0 - (p / r - 1.0) * 120.0).clamp(0.0, 100.0)),
        _ => None,
    };
    match (d, rel) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        _ => None,
    }
}

pub const W_MATCH: f64 = 0.40;
pub const W_RISK: f64 = 0.20;
pub const W_SHOP: f64 = 0.15;
pub const W_REVIEW: f64 = 0.15;
pub const W_PRICE: f64 = 0.10;

/// 最终细筛综合分 0–100。
pub fn final_score(m: &MatchInfo, risk: Option<&RiskInfo>, shop: Option<&ShopInfo>, review: Option<&ReviewInsight>, price: Option<f64>) -> u32 {
    let mut parts: Vec<(f64, f64)> = vec![(m.score as f64, W_MATCH)];
    if let Some(r) = risk {
        parts.push((100.0 - r.score as f64, W_RISK));
    }
    if let Some(s) = shop.filter(|s| s.kind != "unknown") {
        parts.push((s.credibility as f64, W_SHOP));
    }
    if let Some(sc) = review.and_then(|r| r.score) {
        parts.push((sc as f64, W_REVIEW));
    }
    if let Some(p) = price {
        parts.push((p, W_PRICE));
    }
    let wsum: f64 = parts.iter().map(|(_, w)| w).sum();
    let mut v = parts.iter().map(|(x, w)| x * w).sum::<f64>() / wsum.max(0.01);
    match m.verdict.as_str() {
        "mismatch" => v = v.min(30.0),
        "related" => v = v.min(70.0),
        _ => {}
    }
    if risk.is_some_and(|r| r.level == "high") {
        v = v.min(55.0);
    }
    v.round().clamp(0.0, 100.0) as u32
}

fn tag(kind: &str, label: &str, tone: &str) -> ItemTag {
    ItemTag { kind: kind.into(), label: label.into(), tone: tone.into() }
}

/// 结果标签：店铺类型在最前，其后包邮 / 好价 / 高销量 / 好评多 / 二手 / 风险。最多 6 个。
pub fn tags(shop: Option<&ShopInfo>, c_shipping: Option<f64>, coupon: bool, deal: Option<&DealInfo>, sales: Option<i64>, review: Option<&ReviewInsight>, rating: Option<f64>, condition: &str, risk: Option<&RiskInfo>) -> Vec<ItemTag> {
    let mut out = vec![];
    if let Some(s) = shop.filter(|s| s.kind != "unknown") {
        let tone = match s.kind.as_str() {
            "self_operated" | "flagship" | "official" => "good",
            "authorized" | "enterprise" | "distributor" | "marketplace" | "studio" => "neutral",
            _ => "warn",
        };
        out.push(tag(&s.kind, &s.label, tone));
    }
    if risk.is_some_and(|r| r.level == "high") {
        out.push(tag("risk", "风险高", "bad"));
    } else if risk.is_some_and(|r| r.level == "medium") {
        out.push(tag("risk", "需留意", "warn"));
    }
    if c_shipping == Some(0.0) {
        out.push(tag("free_shipping", "包邮", "good"));
    }
    match deal.map(|d| d.level.as_str()) {
        Some("lowest") => out.push(tag("good_deal", deal.map(|d| d.label.as_str()).unwrap_or("历史最低"), "good")),
        Some("low") => out.push(tag("good_deal", "好价", "good")),
        _ => {}
    }
    if coupon {
        out.push(tag("coupon", "券后价", "neutral"));
    }
    if sales.is_some_and(|s| s >= 1000) {
        out.push(tag("high_sales", "高销量", "good"));
    }
    let well = review.and_then(|r| r.score).is_some_and(|s| s >= 85)
        || review.and_then(|r| r.good_rate).is_some_and(|g| g >= 98.0)
        || (rating.is_some_and(|r| r >= 4.7) && review.and_then(|r| r.count).is_some_and(|c| c >= 100));
    if well {
        out.push(tag("well_reviewed", "好评多", "good"));
    }
    if condition == "used" {
        out.push(tag("secondhand_item", "二手", "warn"));
    }
    out.truncate(6);
    out
}

/// 中位数。
pub fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProductSpec;

    fn cand(title: &str, cond: &str) -> Candidate {
        Candidate { platform_id: "taobao".into(), external_id: "1".into(), title: title.into(), price: Some(100.0), currency: "CNY".into(), condition: cond.into(), sales: Some(500), ..Default::default() }
    }
    fn exact() -> MatchInfo {
        MatchInfo { score: 92, verdict: "exact".into(), ..Default::default() }
    }
    fn plan() -> SearchPlan {
        SearchPlan { product: ProductSpec { brand: Some("Sony".into()), model: Some("WH-1000XM5".into()), ..Default::default() }, condition: "new".into(), ..Default::default() }
    }

    #[test]
    fn counterfeit_and_price_anomaly() {
        let f = rule_factors(&cand("索尼 WH-1000XM5 高仿 原单", "new"), &exact(), &plan(), Some(500.0), Some(2300.0));
        assert!(f.iter().any(|x| x.kind == "counterfeit" && x.weight >= 40));
        assert!(f.iter().any(|x| x.kind == "price_anomaly" && x.weight >= 30));
        let r = compose(f, 0, None, false);
        assert_eq!(r.level, "high");
        let ok = compose(rule_factors(&cand("索尼 WH-1000XM5 官方正品", "new"), &exact(), &plan(), Some(2250.0), Some(2300.0)), 0, None, false);
        assert_eq!(ok.level, "low");
        assert_eq!(ok.summary, "未发现明显风险");
    }

    #[test]
    fn used_when_new_wanted_and_shop_adjusts() {
        let f = rule_factors(&cand("索尼 WH-1000XM5 95新", "used"), &exact(), &plan(), None, None);
        assert!(f.iter().any(|x| x.kind == "used" && x.weight == 25));
        let r = compose(f, 0, None, false);
        let flagship = ShopInfo { kind: "flagship".into(), label: "旗舰店".into(), credibility: 92, ..Default::default() };
        let r2 = add_factors(&r, vec![shop_factor(&flagship, false).unwrap()], 0);
        assert!(r2.score < r.score);
    }

    #[test]
    fn final_score_weights_and_caps() {
        let m = exact();
        let low = RiskInfo { score: 10, level: "low".into(), ..Default::default() };
        let shop = ShopInfo { kind: "flagship".into(), credibility: 90, ..Default::default() };
        let rv = ReviewInsight { score: Some(90), ..Default::default() };
        let s = final_score(&m, Some(&low), Some(&shop), Some(&rv), Some(80.0));
        // 0.4*92 + 0.2*90 + 0.15*90 + 0.15*90 + 0.1*80 = 89.8
        assert_eq!(s, 90);
        // 缺评论时按其余权重归一
        let s2 = final_score(&m, Some(&low), Some(&shop), None, Some(80.0));
        assert_eq!(s2, ((0.4 * 92.0 + 0.2 * 90.0 + 0.15 * 90.0 + 0.1 * 80.0) / 0.85_f64).round() as u32);
        let high = RiskInfo { score: 80, level: "high".into(), ..Default::default() };
        assert!(final_score(&m, Some(&high), Some(&shop), Some(&rv), Some(80.0)) <= 55);
        let mm = MatchInfo { score: 90, verdict: "mismatch".into(), ..Default::default() };
        assert!(final_score(&mm, None, None, None, None) <= 30);
    }

    #[test]
    fn tags_order_and_tones() {
        let shop = ShopInfo { kind: "flagship".into(), label: "旗舰店".into(), credibility: 90, ..Default::default() };
        let deal = DealInfo { level: "lowest".into(), label: "历史最低".into(), basis: String::new(), low: None, median: None, high: None, points: 5, days: 40 };
        let t = tags(Some(&shop), Some(0.0), false, Some(&deal), Some(20000), None, None, "new", None);
        assert_eq!(t[0].label, "旗舰店");
        assert_eq!(t[0].tone, "good");
        let labels: Vec<&str> = t.iter().map(|x| x.label.as_str()).collect();
        assert!(labels.contains(&"包邮") && labels.contains(&"历史最低") && labels.contains(&"高销量"));
        let ind = ShopInfo { kind: "individual".into(), label: "个人卖家".into(), credibility: 40, ..Default::default() };
        let risky = RiskInfo { score: 80, level: "high".into(), ..Default::default() };
        let t = tags(Some(&ind), None, false, None, None, None, None, "used", Some(&risky));
        assert_eq!(t[0].tone, "warn");
        assert!(t.iter().any(|x| x.label == "风险高" && x.tone == "bad"));
    }
}
