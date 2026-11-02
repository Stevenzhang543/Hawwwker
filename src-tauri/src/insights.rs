//! 洞察：市场概览、价格走势、AI 分析报告（流式）。

use crate::ai::{prompts, Msg, Opts};
use crate::db::{iso_ago, Db};
use crate::error::AppResult;
use crate::fx::Fx;
use crate::models::*;
use crate::state::AppState;
use rusqlite::params;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter};

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn nice(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mag = 10f64.powf(x.log10().floor());
    let r = x / mag;
    let n = if r < 1.5 { 1.0 } else if r < 3.5 { 2.0 } else if r < 7.5 { 5.0 } else { 10.0 };
    n * mag
}

fn symbol(c: &str) -> &'static str {
    match c {
        "CNY" => "¥",
        "EUR" => "€",
        "USD" => "$",
        "GBP" => "£",
        "JPY" => "¥",
        _ => "",
    }
}

pub fn overview(db: &Db, fx: &Fx, days: i64, display: &str) -> AppResult<MarketOverview> {
    let since = iso_ago(days);
    let c = db.conn();
    let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> i64 { c.query_row(sql, p, |r| r.get(0)).unwrap_or(0) };
    let tracked = count("SELECT COUNT(*) FROM watches", &[]);
    let drops = count("SELECT COUNT(*) FROM notices WHERE kind IN ('price_drop','target_hit') AND created_at>=?1", &[&since]);
    let monitors = count("SELECT COUNT(*) FROM monitors", &[]);
    let runs = count("SELECT COUNT(*) FROM runs WHERE started_at>=?1", &[&since]);
    let listings = count("SELECT COUNT(*) FROM listings WHERE last_seen_at>=?1", &[&since]);

    // 最近出现过的商品（换算到显示币种）
    let mut stmt = c.prepare("SELECT platform_id, price, currency FROM listings WHERE last_seen_at>=?1 AND price IS NOT NULL")?;
    let rows: Vec<(String, f64)> = stmt
        .query_map([&since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, String>(2)?)))?
        .filter_map(|r| r.ok())
        .filter_map(|(p, price, cur)| fx.convert(price, &cur, display).map(|v| (p, v)))
        .collect();
    let mut by_p: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (p, v) in rows.iter() {
        by_p.entry(p.clone()).or_default().push(*v);
    }
    let mut platform_medians: Vec<PlatformMedian> = by_p
        .iter()
        .map(|(p, v)| PlatformMedian { platform_id: p.clone(), median: (median(&mut v.clone()) * 100.0).round() / 100.0, count: v.len() as i64 })
        .collect();
    platform_medians.sort_by_key(|x| std::cmp::Reverse(x.count));
    platform_medians.truncate(10);
    let mut platform_share: Vec<PlatformCount> = by_p.iter().map(|(p, v)| PlatformCount { platform_id: p.clone(), count: v.len() as i64 }).collect();
    platform_share.sort_by_key(|x| std::cmp::Reverse(x.count));

    // 价格带：按分位数取"整齐"的边界
    let mut prices: Vec<f64> = rows.iter().map(|(_, v)| *v).collect();
    prices.sort_by(|a, b| a.total_cmp(b));
    let mut price_bands = vec![];
    if prices.len() >= 3 {
        let q = |f: f64| prices[((prices.len() - 1) as f64 * f).round() as usize];
        let mut edges: Vec<f64> = vec![0.0, nice(q(0.25)), nice(q(0.5)), nice(q(0.75)), nice(q(0.95))];
        edges.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let sym = symbol(display);
        for i in 0..edges.len() {
            let lo = edges[i];
            let hi = edges.get(i + 1).copied();
            let n = prices.iter().filter(|p| **p >= lo && hi.is_none_or(|h| **p < h)).count() as i64;
            let label = match hi {
                Some(h) => format!("{sym}{}–{}", lo as i64, h as i64),
                None => format!("{sym}{}+", lo as i64),
            };
            price_bands.push(LabelCount { label, count: n });
        }
    }

    // 同款价差（聚类后的同一商品在不同平台的价差）
    let mut stmt = c.prepare(
        "SELECT rr.run_id, rr.cluster, rr.display_price FROM run_results rr JOIN runs r ON r.id=rr.run_id
         WHERE r.started_at>=?1 AND rr.cluster IS NOT NULL AND rr.display_price IS NOT NULL",
    )?;
    let mut clusters: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    for (run, cl, p) in stmt.query_map([&since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?)))?.filter_map(|r| r.ok()) {
        clusters.entry((run, cl)).or_default().push(p);
    }
    let spreads: Vec<f64> = clusters
        .values()
        .filter(|v| v.len() >= 2)
        .filter_map(|v| {
            let mx = v.iter().cloned().fold(f64::MIN, f64::max);
            let mn = v.iter().cloned().fold(f64::MAX, f64::min);
            (mx > 0.0).then(|| (mx - mn) / mx * 100.0)
        })
        .collect();
    let avg_spread_pct = if spreads.is_empty() { 0.0 } else { (spreads.iter().sum::<f64>() / spreads.len() as f64 * 10.0).round() / 10.0 };

    let mut stmt = c.prepare("SELECT id, title, platform_id, first_price, last_price, currency FROM watches WHERE first_price IS NOT NULL AND last_price IS NOT NULL AND last_price<first_price")?;
    let mut top_drops: Vec<TopDrop> = stmt
        .query_map([], |r| {
            let from: f64 = r.get(3)?;
            let to: f64 = r.get(4)?;
            Ok(TopDrop { watch_id: r.get(0)?, title: r.get(1)?, platform_id: r.get(2)?, from, to, pct: ((to - from) / from * 1000.0).round() / 10.0, currency: r.get(5)? })
        })?
        .filter_map(|r| r.ok())
        .collect();
    top_drops.sort_by(|a, b| a.pct.total_cmp(&b.pct));
    top_drops.truncate(8);

    let mut daily_map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for d in 0..days.min(90) {
        let day = (chrono::Utc::now() - chrono::Duration::days(d)).format("%Y-%m-%d").to_string();
        daily_map.insert(day, (0, 0));
    }
    let mut stmt = c.prepare("SELECT substr(started_at,1,10), COUNT(*) FROM runs WHERE started_at>=?1 GROUP BY 1")?;
    for (d, n) in stmt.query_map([&since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.filter_map(|r| r.ok()) {
        daily_map.entry(d).or_default().0 = n;
    }
    let mut stmt = c.prepare("SELECT substr(first_seen_at,1,10), COUNT(*) FROM listings WHERE first_seen_at>=?1 GROUP BY 1")?;
    for (d, n) in stmt.query_map([&since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.filter_map(|r| r.ok()) {
        daily_map.entry(d).or_default().1 = n;
    }
    let daily = daily_map.into_iter().map(|(day, (runs, listings))| DailyCount { day, runs, listings }).collect();

    Ok(MarketOverview {
        days,
        currency: display.into(),
        kpis: MarketKpis { tracked, drops, monitors, runs, listings, avg_spread_pct },
        platform_medians,
        price_bands,
        platform_share,
        top_drops,
        daily,
    })
}

fn day_points(rows: Vec<(String, f64)>) -> Vec<TrendPoint> {
    let mut by_day: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (t, p) in rows {
        by_day.entry(t.chars().take(10).collect()).or_default().push(p);
    }
    by_day
        .into_iter()
        .map(|(d, mut v)| {
            let mn = v.iter().cloned().fold(f64::MAX, f64::min);
            let mx = v.iter().cloned().fold(f64::MIN, f64::max);
            let count = v.len() as u32;
            TrendPoint { t: format!("{d}T12:00:00.000Z"), min: mn, median: (median(&mut v) * 100.0).round() / 100.0, max: mx, count }
        })
        .collect()
}

pub fn trends(db: &Db, fx: &Fx, watch_ids: &[i64], monitor_ids: &[i64], days: i64, display: &str) -> AppResult<Vec<TrendSeries>> {
    let since = iso_ago(days);
    let c = db.conn();
    let mut out = vec![];
    for id in watch_ids {
        let Ok((title, lid, cur)) = c.query_row("SELECT title, listing_id, currency FROM watches WHERE id=?1", [id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
        }) else {
            continue;
        };
        let rows: Vec<(String, f64)> = match lid {
            Some(l) => {
                let mut stmt = c.prepare("SELECT captured_at, price, currency FROM price_points WHERE listing_id=?1 AND captured_at>=?2 ORDER BY captured_at")?;
                let v: Vec<(String, f64)> = stmt
                    .query_map(params![l, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, String>(2)?)))?
                    .filter_map(|r| r.ok())
                    .filter_map(|(t, p, pc)| fx.convert(p, &pc, &cur).map(|v| (t, v)))
                    .collect();
                v
            }
            None => vec![],
        };
        out.push(TrendSeries { key: format!("w{id}"), label: title, kind: "watch".into(), currency: cur, points: day_points(rows) });
    }
    for id in monitor_ids {
        let Ok(name) = c.query_row("SELECT name FROM monitors WHERE id=?1", [id], |r| r.get::<_, String>(0)) else { continue };
        let mut stmt = c.prepare(
            "SELECT r.started_at, rr.display_price, r.currency FROM run_results rr JOIN runs r ON r.id=rr.run_id
             WHERE r.monitor_id=?1 AND r.started_at>=?2 AND rr.display_price IS NOT NULL AND rr.verdict IN ('exact','close')",
        )?;
        let rows: Vec<(String, f64)> = stmt
            .query_map(params![id, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, Option<String>>(2)?)))?
            .filter_map(|r| r.ok())
            .filter_map(|(t, p, cur)| fx.convert(p, cur.as_deref().unwrap_or(display), display).map(|v| (t, v)))
            .collect();
        out.push(TrendSeries { key: format!("m{id}"), label: name, kind: "monitor".into(), currency: display.into(), points: day_points(rows) });
    }
    Ok(out)
}

/// AI 分析报告（流式）。
pub fn report(state: &AppState, app: AppHandle, scope: String, days: i64) -> AppResult<String> {
    state.ai.require()?;
    let s = crate::settings::load(&state.db);
    let ov = overview(&state.db, &state.fx, days, &s.display_currency)?;
    let watches = crate::tracker::list(&state.db)?;
    let monitors = crate::monitors::list(&state.db)?;
    let watch_ids: Vec<i64> = watches.iter().map(|w| w.id).take(20).collect();
    let monitor_ids: Vec<i64> = monitors.iter().filter(|m| scope == "all" || scope == format!("monitor:{}", m.id)).map(|m| m.id).collect();
    let tr = trends(&state.db, &state.fx, if scope == "all" || scope == "watches" { &watch_ids } else { &[] }, &monitor_ids, days, &s.display_currency)?;
    let compact_trends: Vec<serde_json::Value> = tr
        .iter()
        .map(|t| {
            let first = t.points.first().map(|p| p.median);
            let last = t.points.last().map(|p| p.median);
            let low = t.points.iter().map(|p| p.min).fold(None, |a: Option<f64>, x| Some(a.map_or(x, |y| y.min(x))));
            serde_json::json!({"label": t.label, "kind": t.kind, "currency": t.currency, "first": first, "last": last, "lowest": low, "days": t.points.len()})
        })
        .collect();
    let data = serde_json::json!({
        "role": s.role, "days": days, "displayCurrency": s.display_currency, "scope": scope,
        "kpis": ov.kpis, "platformMedians": ov.platform_medians, "priceBands": ov.price_bands, "topDrops": ov.top_drops,
        "trends": compact_trends,
        "watches": watches.iter().take(20).map(|w| serde_json::json!({"title": w.title, "platform": w.platform_id, "currency": w.currency, "first": w.first_price, "last": w.last_price, "lowest": w.lowest, "target": w.target_price})).collect::<Vec<_>>(),
    });
    let report_id = uuid::Uuid::new_v4().to_string();
    let rid = report_id.clone();
    let ai = state.ai.clone();
    tauri::async_runtime::spawn(async move {
        let msgs = vec![Msg::system(prompts::REPORT), Msg::user(data.to_string())];
        let a2 = app.clone();
        let r2 = rid.clone();
        let res = ai
            .stream(&msgs, Opts { max_tokens: Some(2048), temperature: Some(0.3), ..Default::default() }, |d| {
                let _ = a2.emit("insight://delta", InsightDelta { report_id: r2.clone(), delta: d.to_string() });
            })
            .await;
        let done = match res {
            Ok(text) => InsightDone { report_id: rid, text, error: None },
            Err(e) => InsightDone { report_id: rid, text: String::new(), error: Some(e.message) },
        };
        let _ = app.emit("insight://done", done);
    });
    Ok(report_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_numbers_and_median() {
        assert_eq!(nice(137.0), 100.0);
        assert_eq!(nice(260.0), 200.0);
        assert_eq!(nice(4200.0), 5000.0);
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
    }

    #[test]
    fn overview_on_empty_db() {
        let db = Db::memory().unwrap();
        let fx = Fx::new(&db);
        let ov = overview(&db, &fx, 30, "CNY").unwrap();
        assert_eq!(ov.kpis.tracked, 0);
        assert_eq!(ov.daily.len(), 30);
    }
}
