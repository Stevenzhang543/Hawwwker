//! 过滤、去重、规则匹配分（AI 校验失败时兜底）与跨平台同款聚类。

use super::parse::Candidate;
use crate::fx::Fx;
use crate::models::{MatchInfo, SearchPlan};
use std::collections::{BTreeMap, HashSet};

pub struct FilterOutcome {
    pub kept: Vec<Candidate>,
    pub dropped: BTreeMap<String, u32>,
    pub duplicates: u32,
    pub relaxed: Vec<String>,
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// 候选价格换算到计划币种。
/// 候选的到手价（券后 + 运费）换算到指定币种；预算、排序都按到手价比较。
pub fn price_in(c: &Candidate, currency: &str, fx: &Fx) -> Option<f64> {
    c.landed().and_then(|p| fx.convert(p, &c.currency, currency))
}

fn excluded(c: &Candidate, plan: &SearchPlan) -> bool {
    let t = c.title.to_lowercase();
    plan.exclude.iter().any(|x| {
        let x = x.trim().to_lowercase();
        // 用户说"不要二手"，而标题是"非二手 全新"之类时不误杀
        !x.is_empty() && t.contains(&x) && !t.contains(&format!("非{x}")) && !t.contains(&format!("不是{x}"))
    })
}

pub fn filter(cands: Vec<Candidate>, plan: &SearchPlan, fx: &Fx) -> FilterOutcome {
    let dropped: BTreeMap<String, u32>;
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut duplicates = 0u32;
    let mut unique: Vec<Candidate> = vec![];
    for c in cands {
        if !seen.insert((c.platform_id.clone(), c.external_id.clone())) {
            duplicates += 1;
            continue;
        }
        unique.push(c);
    }
    let pass = |c: &Candidate, relax: &HashSet<&str>, dropped: &mut BTreeMap<String, u32>| -> bool {
        if !relax.contains("exclude") && excluded(c, plan) {
            *dropped.entry("exclude".into()).or_default() += 1;
            return false;
        }
        if !relax.contains("price") {
            if let Some(p) = price_in(c, &plan.currency, fx) {
                if plan.price_min.is_some_and(|m| p < m * 0.9) || plan.price_max.is_some_and(|m| p > m * 1.1) {
                    *dropped.entry("price".into()).or_default() += 1;
                    return false;
                }
            }
        }
        if !relax.contains("condition") && plan.condition == "new" && c.condition == "used" {
            *dropped.entry("condition".into()).or_default() += 1;
            return false;
        }
        true
    };
    let mut relax: HashSet<&str> = HashSet::new();
    let mut relaxed = vec![];
    loop {
        let mut d = BTreeMap::new();
        let kept: Vec<Candidate> = unique.iter().filter(|c| pass(c, &relax, &mut d)).cloned().collect();
        if !kept.is_empty() || unique.is_empty() {
            dropped = d;
            return FilterOutcome { kept, dropped, duplicates, relaxed };
        }
        // 全部被过滤：逐级放宽，保证有输出并说明
        let next = ["condition", "price", "exclude"].into_iter().find(|k| !relax.contains(k) && d.contains_key(*k));
        match next {
            Some(k) => {
                relax.insert(k);
                relaxed.push(k.to_string());
            }
            None => {
                dropped = d;
                return FilterOutcome { kept, dropped, duplicates, relaxed };
            }
        }
    }
}

/// 字符 bigram Jaccard 相似度。
pub fn similarity(a: &str, b: &str) -> f64 {
    let grams = |s: &str| -> HashSet<(char, char)> {
        let v: Vec<char> = norm(s).chars().collect();
        v.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (x, y) = (grams(a), grams(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    jaccard(&x, &y)
}

fn jaccard(x: &HashSet<(char, char)>, y: &HashSet<(char, char)>) -> f64 {
    let inter = x.intersection(y).count() as f64;
    let union = x.union(y).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// 规则匹配分：关键词/型号覆盖率 + 必须条件 + 排除冲突。AI 校验不可用时使用。
pub fn rule_match(c: &Candidate, plan: &SearchPlan) -> MatchInfo {
    let title = norm(&c.title);
    let mut terms: Vec<String> = vec![];
    if let Some(m) = plan.product.model.as_ref() {
        terms.push(norm(m));
    }
    if let Some(b) = plan.product.brand.as_ref() {
        terms.push(norm(b));
    }
    for k in plan.keywords.iter() {
        for w in k.split_whitespace() {
            let n = norm(w);
            if n.chars().count() >= 2 && !terms.contains(&n) {
                terms.push(n);
            }
        }
    }
    let q = norm(&c.query);
    if terms.is_empty() && !q.is_empty() {
        terms.push(q);
    }
    let hit = terms.iter().filter(|t| title.contains(t.as_str())).count();
    let cover = if terms.is_empty() { 0.5 } else { hit as f64 / terms.len() as f64 };
    let mut score = (35.0 + cover * 55.0) as i64;
    let mut flags = vec![];
    let mut reasons = vec![];
    if c.is_ad {
        flags.push("ad".to_string());
        score -= 10;
    }
    if c.condition == "used" && plan.condition == "new" {
        flags.push("used".to_string());
        score -= 25;
    }
    if c.price.is_none() {
        flags.push("unknown_price".to_string());
        score -= 5;
    }
    if plan.product.must_not.iter().any(|m| !m.is_empty() && title.contains(&norm(m))) {
        flags.push("accessory".to_string());
        score -= 30;
    }
    reasons.push(if cover >= 0.99 {
        "标题包含全部关键词（规则判断）".to_string()
    } else if cover > 0.0 {
        format!("标题包含 {hit}/{} 个关键词（规则判断）", terms.len())
    } else {
        "标题未包含关键词（规则判断）".to_string()
    });
    let score = score.clamp(0, 100) as u32;
    let verdict = match score {
        85.. => "exact",
        65..=84 => "close",
        35..=64 => "related",
        _ => "mismatch",
    };
    MatchInfo { score, verdict: verdict.into(), reasons, flags, ai_checked: false }
}

/// 同款聚类：优先 AI 给出的 productKey；否则标题相似度 ≥ 0.8 且来自不同平台。返回每条的 cluster id。
pub fn clusters(items: &[(String, String, String)]) -> Vec<Option<String>> {
    // items: (platform_id, product_key, title)
    let mut ids: Vec<Option<String>> = vec![None; items.len()];
    let mut next = 0usize;
    for i in 0..items.len() {
        if ids[i].is_some() {
            continue;
        }
        let mut group = vec![i];
        for j in (i + 1)..items.len() {
            if ids[j].is_some() || items[j].0 == items[i].0 {
                continue;
            }
            let same_key = !items[i].1.is_empty() && items[i].1 == items[j].1;
            let similar = items[i].1.is_empty() && items[j].1.is_empty() && similarity(&items[i].2, &items[j].2) >= 0.8;
            if same_key || similar {
                group.push(j);
            }
        }
        if group.len() > 1 {
            next += 1;
            for g in group {
                ids[g] = Some(format!("c{next}"));
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn cand(pid: &str, id: &str, title: &str, price: f64, cur: &str) -> Candidate {
        Candidate {
            platform_id: pid.into(),
            external_id: id.into(),
            title: title.into(),
            price: Some(price),
            currency: cur.into(),
            condition: "unknown".into(),
            ..Default::default()
        }
    }

    #[test]
    fn price_filter_converts_currency_and_relaxes() {
        let db = Db::memory().unwrap();
        let fx = Fx::new(&db);
        let plan = SearchPlan { price_max: Some(200.0), currency: "EUR".into(), exclude: vec!["二手".into()], ..Default::default() };
        let out = filter(
            vec![
                cand("amazon_de", "1", "Sony WH-1000XM5", 189.0, "EUR"),
                cand("taobao", "2", "索尼 WH-1000XM5", 1899.0, "CNY"), // ≈233 EUR → 超预算
                cand("taobao", "2", "dup", 1.0, "CNY"),
                cand("xianyu", "3", "二手 索尼耳机", 900.0, "CNY"),
            ],
            &plan,
            &fx,
        );
        assert_eq!(out.kept.len(), 1);
        assert_eq!(out.duplicates, 1);
        assert_eq!(out.dropped.get("price"), Some(&1));
        assert_eq!(out.dropped.get("exclude"), Some(&1));
        let tight = SearchPlan { price_max: Some(10.0), currency: "EUR".into(), ..Default::default() };
        let out = filter(vec![cand("amazon_de", "1", "x", 189.0, "EUR")], &tight, &fx);
        assert_eq!(out.kept.len(), 1, "relaxed instead of empty");
        assert_eq!(out.relaxed, vec!["price".to_string()]);
    }

    #[test]
    fn similarity_and_clusters() {
        assert!(similarity("Sony WH-1000XM5 Black", "SONY WH1000XM5 black") > 0.8);
        assert!(similarity("Sony WH-1000XM5", "Bose QC Ultra") < 0.3);
        let ids = clusters(&[
            ("amazon_de".into(), "sony wh-1000xm5".into(), "a".into()),
            ("ebay_de".into(), "sony wh-1000xm5".into(), "b".into()),
            ("ebay_de".into(), "bose qc".into(), "c".into()),
        ]);
        assert_eq!(ids[0], ids[1]);
        assert!(ids[0].is_some() && ids[2].is_none());
    }

    #[test]
    fn rule_match_flags_accessories() {
        let plan = SearchPlan {
            keywords: vec!["Sony WH-1000XM5".into()],
            product: crate::models::ProductSpec { must_not: vec!["耳机套".into()], ..Default::default() },
            ..Default::default()
        };
        let mut c = cand("taobao", "1", "适用索尼 Sony WH-1000XM5 耳机套 替换海绵", 29.0, "CNY");
        c.query = "Sony WH-1000XM5".into();
        let m = rule_match(&c, &plan);
        assert!(m.flags.contains(&"accessory".to_string()));
        assert!(m.score < 65);
        let good = cand("amazon_de", "2", "Sony WH-1000XM5 Kopfhörer", 289.0, "EUR");
        assert!(rule_match(&good, &plan).score >= 85);
    }
}
