//! 价格判断（对标 Keepa / Rufus 的历史价格、什么值得买的"好价"）与"不相关"反馈。
//!
//! 价格判断只依据本机记录的价格点（搜索、监控、追踪每次看到都会记录），所以新商品显示"数据积累中"，
//! 用得越久越准。不联网获取第三方历史价，也不估算未来价格。

use crate::db::{iso_ago, now_iso, Db};
use crate::models::{DealInfo, SearchPlan};
use rusqlite::params;
use sha2::{Digest, Sha256};

fn pct(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// points: (时间 ISO, 价格)，同一币种。current: 当前价。
pub fn verdict(points: &[(String, f64)], current: f64) -> DealInfo {
    let n = points.len() as u32;
    let first = points.iter().map(|(t, _)| t.as_str()).min().unwrap_or("");
    let last = points.iter().map(|(t, _)| t.as_str()).max().unwrap_or("");
    let days = match (crate::db::parse_iso(first), crate::db::parse_iso(last)) {
        (Some(a), Some(b)) => ((b - a).num_hours() as f64 / 24.0).ceil().max(0.0) as u32,
        _ => 0,
    };
    let mut prices: Vec<f64> = points.iter().map(|(_, p)| *p).filter(|p| p.is_finite() && *p > 0.0).collect();
    prices.sort_by(|a, b| a.total_cmp(b));
    if prices.len() < 3 || days < 2 {
        return DealInfo {
            level: "new".into(),
            label: "数据积累中".into(),
            basis: format!("本机已记录 {n} 次价格，再观察几天就能判断好坏"),
            low: prices.first().copied(),
            median: None,
            high: prices.last().copied(),
            points: n,
            days,
        };
    }
    let low = prices[0];
    let high = prices[prices.len() - 1];
    let median = pct(&prices, 0.5);
    let p25 = pct(&prices, 0.25);
    let (level, label) = if current <= low * 1.001 {
        ("lowest", if days >= 30 { "历史最低" } else { "近期最低" })
    } else if current <= p25 {
        ("low", "近期低位")
    } else if current <= median * 1.05 {
        ("normal", "价格正常")
    } else {
        ("high", "偏高")
    };
    DealInfo {
        level: level.into(),
        label: label.into(),
        basis: format!("近 {days} 天 {n} 次记录：最低 {low:.2}，中位 {median:.2}，最高 {high:.2}"),
        low: Some(low),
        median: Some(median),
        high: Some(high),
        points: n,
        days,
    }
}

/// 某个商品的价格判断（只取与当前价同币种的近 180 天记录）。
pub fn for_listing(c: &rusqlite::Connection, listing_id: &str, current: f64, currency: &str) -> Option<DealInfo> {
    let mut stmt = c
        .prepare("SELECT captured_at, price FROM price_points WHERE listing_id=?1 AND currency=?2 AND captured_at>=?3 ORDER BY captured_at")
        .ok()?;
    let pts: Vec<(String, f64)> = stmt
        .query_map(params![listing_id, currency, iso_ago(180)], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if pts.is_empty() {
        return None;
    }
    Some(verdict(&pts, current))
}

/// 反馈作用范围：同一个需求（品牌 + 型号 + 关键词）。
pub fn scope_of(plan: &SearchPlan) -> String {
    let mut parts: Vec<String> = plan.keywords.iter().map(|k| k.trim().to_lowercase()).collect();
    if let Some(m) = &plan.product.model {
        parts.push(m.trim().to_lowercase());
    }
    parts.sort();
    parts.dedup();
    hex::encode(&Sha256::digest(parts.join("|").as_bytes())[..8])
}

pub fn set_feedback(db: &Db, platform_id: &str, external_id: &str, scope: &str, kind: &str) -> rusqlite::Result<()> {
    let c = db.conn();
    if kind == "clear" {
        c.execute("DELETE FROM feedback WHERE platform_id=?1 AND external_id=?2 AND scope=?3", params![platform_id, external_id, scope])?;
    } else {
        c.execute(
            "INSERT INTO feedback(platform_id, external_id, scope, kind, created_at) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(platform_id, external_id, scope) DO UPDATE SET kind=excluded.kind, created_at=excluded.created_at",
            params![platform_id, external_id, scope, kind, now_iso()],
        )?;
    }
    Ok(())
}

/// 该需求下被用户标记为不相关的商品集合：(platform_id, external_id)
pub fn irrelevant_set(db: &Db, scope: &str) -> std::collections::HashSet<(String, String)> {
    let c = db.conn();
    let Ok(mut stmt) = c.prepare("SELECT platform_id, external_id FROM feedback WHERE scope=?1 AND kind='irrelevant'") else {
        return Default::default();
    };
    stmt.query_map([scope], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(i64, f64)]) -> Vec<(String, f64)> {
        v.iter().map(|(d, p)| (crate::db::iso(chrono::Utc::now() - chrono::Duration::days(*d)), *p)).collect()
    }

    #[test]
    fn verdict_levels() {
        let hist = pts(&[(60, 299.0), (45, 289.0), (30, 279.0), (20, 289.0), (10, 299.0), (1, 269.0)]);
        assert_eq!(verdict(&hist, 269.0).level, "lowest");
        assert_eq!(verdict(&hist, 269.0).label, "历史最低");
        assert_eq!(verdict(&hist, 279.0).level, "low");
        assert_eq!(verdict(&hist, 289.0).level, "normal");
        assert_eq!(verdict(&hist, 329.0).level, "high");
        assert_eq!(verdict(&pts(&[(0, 100.0)]), 100.0).level, "new");
    }

    #[test]
    fn feedback_roundtrip_and_scope() {
        let db = Db::memory().unwrap();
        let plan = SearchPlan { keywords: vec!["Sony WH-1000XM5".into()], ..Default::default() };
        let plan2 = SearchPlan { keywords: vec!["sony wh-1000xm5 ".into()], ..Default::default() };
        assert_eq!(scope_of(&plan), scope_of(&plan2));
        let sc = scope_of(&plan);
        set_feedback(&db, "amazon_de", "B0C1", &sc, "irrelevant").unwrap();
        assert!(irrelevant_set(&db, &sc).contains(&("amazon_de".to_string(), "B0C1".to_string())));
        set_feedback(&db, "amazon_de", "B0C1", &sc, "clear").unwrap();
        assert!(irrelevant_set(&db, &sc).is_empty());
    }
}
