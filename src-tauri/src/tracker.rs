//! 价格追踪：追踪项 CRUD、定时打开商品页读价（JSON-LD → meta → 最大字号价格 → AI 兜底）、降价/目标价提醒。

use crate::ai::prompts;
use crate::browser::{extract, login};
use crate::db::{iso_in, now_iso, Db};
use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, PricePoint, WatchAddInput, WatchItem, WatchPatch};
use crate::platforms;
use crate::search::parse::prices_in;
use crate::search::pipeline::spark;
use crate::state::AppState;
use rusqlite::{params, OptionalExtension, Row};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

const COLS: &str = "id, listing_id, platform_id, title, url, image, currency, target_price, last_price, first_price, lowest, highest,
    interval_minutes, enabled, status, last_error, last_checked_at, next_check_at, note, created_at, my_price";

fn row(r: &Row) -> rusqlite::Result<WatchItem> {
    let last: Option<f64> = r.get(8)?;
    let first: Option<f64> = r.get(9)?;
    Ok(WatchItem {
        id: r.get(0)?,
        listing_id: r.get(1)?,
        platform_id: r.get(2)?,
        title: r.get(3)?,
        url: r.get(4)?,
        image: r.get(5)?,
        currency: r.get(6)?,
        target_price: r.get(7)?,
        last_price: last,
        first_price: first,
        lowest: r.get(10)?,
        highest: r.get(11)?,
        change_pct: match (first, last) {
            (Some(f), Some(l)) if f > 0.0 => Some(((l - f) / f * 1000.0).round() / 10.0),
            _ => None,
        },
        interval_minutes: r.get(12)?,
        enabled: r.get::<_, i64>(13)? != 0,
        status: r.get(14)?,
        last_error: r.get(15)?,
        last_checked_at: r.get(16)?,
        next_check_at: r.get(17)?,
        note: r.get(18)?,
        created_at: r.get(19)?,
        spark: vec![],
        my_price: r.get(20)?,
        deal: None,
    })
}

pub fn get(db: &Db, id: i64) -> AppResult<WatchItem> {
    let c = db.conn();
    let mut w = c
        .query_row(&format!("SELECT {COLS} FROM watches WHERE id=?1"), [id], row)
        .optional()?
        .ok_or_else(|| AppError::not_found("追踪项"))?;
    if let Some(l) = &w.listing_id {
        w.spark = spark(&c, l);
        w.deal = w.last_price.and_then(|p| crate::deal::for_listing(&c, l, p, &w.currency));
    }
    Ok(w)
}

pub fn list(db: &Db) -> AppResult<Vec<WatchItem>> {
    let c = db.conn();
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM watches ORDER BY created_at DESC"))?;
    let mut items: Vec<WatchItem> = stmt.query_map([], row)?.filter_map(|r| r.ok()).collect();
    for w in items.iter_mut() {
        if let Some(l) = &w.listing_id {
            w.spark = spark(&c, l);
            w.deal = w.last_price.and_then(|p| crate::deal::for_listing(&c, l, p, &w.currency));
        }
    }
    Ok(items)
}

pub fn add(db: &Db, input: WatchAddInput) -> AppResult<WatchItem> {
    let interval = input.interval_minutes.unwrap_or(360).clamp(30, 10080);
    let now = now_iso();
    let (listing_id, platform_id, title, url, image, currency, price) = if let Some(lid) = input.listing_id.as_ref() {
        let c = db.conn();
        if let Some(existing) = c.query_row("SELECT id FROM watches WHERE listing_id=?1", [lid], |r| r.get::<_, i64>(0)).optional()? {
            drop(c);
            return get(db, existing);
        }
        c.query_row("SELECT id, platform_id, title, url, image, currency, price FROM listings WHERE id=?1", [lid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<f64>>(6)?))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("商品"))?
    } else {
        let url = input.url.clone().unwrap_or_default().trim().to_string();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::invalid("请粘贴商品页链接（http/https 开头）"));
        }
        let all = platforms::all(db);
        let plat = input.platform_id.clone().and_then(|id| all.iter().find(|p| p.id == id).cloned()).or_else(|| all.iter().find(|p| p.host_matches(&url)).cloned());
        let (pid, cur) = plat.map(|p| (p.id, p.currency)).unwrap_or_else(|| ("web".into(), crate::settings::load(db).display_currency));
        let title = input.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| url.split("://").nth(1).unwrap_or(&url).chars().take(80).collect());
        let ext = {
            use sha2::{Digest, Sha256};
            format!("u{}", hex::encode(&Sha256::digest(url.as_bytes())[..8]))
        };
        let c = db.conn();
        let lid: String = c.query_row(
            "INSERT INTO listings(id, platform_id, external_id, title, url, currency, first_seen_at, last_seen_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)
             ON CONFLICT(platform_id, external_id) DO UPDATE SET last_seen_at=excluded.last_seen_at RETURNING id",
            params![uuid::Uuid::new_v4().to_string(), pid, ext, title, url, cur, now],
            |r| r.get(0),
        )?;
        if let Some(existing) = c.query_row("SELECT id FROM watches WHERE listing_id=?1", [&lid], |r| r.get::<_, i64>(0)).optional()? {
            drop(c);
            return get(db, existing);
        }
        (lid, pid, title, url, None, cur, None)
    };
    let id = {
        let c = db.conn();
        c.execute(
            "INSERT INTO watches(listing_id, platform_id, title, url, image, currency, target_price, last_price, first_price, lowest, highest,
               interval_minutes, enabled, status, next_check_at, note, created_at, last_checked_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8,?8,?8,?9,1,?10,?11,?12,?13,CASE WHEN ?8 IS NULL THEN NULL ELSE ?13 END)",
            params![listing_id, platform_id, title, url, image, currency, input.target_price, price, interval,
                    if price.is_some() { "ok" } else { "new" }, if price.is_some() { iso_in(interval) } else { now.clone() },
                    input.note.unwrap_or_default(), now],
        )?;
        c.last_insert_rowid()
    };
    get(db, id)
}

pub fn update(db: &Db, id: i64, patch: WatchPatch) -> AppResult<WatchItem> {
    let w = get(db, id)?;
    let c = db.conn();
    if let Some(tp) = patch.target_price {
        c.execute("UPDATE watches SET target_price=?2, last_alert_price=NULL WHERE id=?1", params![id, tp])?;
    }
    if let Some(i) = patch.interval_minutes {
        let i = i.clamp(30, 10080);
        c.execute("UPDATE watches SET interval_minutes=?2, next_check_at=?3 WHERE id=?1", params![id, i, iso_in(i)])?;
    }
    if let Some(e) = patch.enabled {
        c.execute("UPDATE watches SET enabled=?2 WHERE id=?1", params![id, e as i64])?;
    }
    if let Some(n) = patch.note {
        c.execute("UPDATE watches SET note=?2 WHERE id=?1", params![id, n.chars().take(500).collect::<String>()])?;
    }
    if let Some(mp) = patch.my_price {
        c.execute("UPDATE watches SET my_price=?2 WHERE id=?1", params![id, mp])?;
    }
    if let Some(t) = patch.title.filter(|t| !t.trim().is_empty()) {
        c.execute("UPDATE watches SET title=?2 WHERE id=?1", params![id, t.trim()])?;
    }
    drop(c);
    let _ = w;
    get(db, id)
}

pub fn remove(db: &Db, id: i64) -> AppResult<()> {
    db.conn().execute("DELETE FROM watches WHERE id=?1", [id])?;
    Ok(())
}

pub fn history(db: &Db, id: i64, days: i64) -> AppResult<Vec<PricePoint>> {
    let w = get(db, id)?;
    let Some(lid) = w.listing_id else { return Ok(vec![]) };
    let c = db.conn();
    let mut stmt = c.prepare("SELECT captured_at, price, currency, source FROM price_points WHERE listing_id=?1 AND captured_at>=?2 ORDER BY captured_at")?;
    let rows = stmt.query_map(params![lid, crate::db::iso_ago(days.clamp(1, 3650))], |r| Ok(PricePoint { t: r.get(0)?, price: r.get(1)?, currency: r.get(2)?, source: r.get(3)? }))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn set_status(db: &Db, id: i64, status: &str) {
    let _ = db.conn().execute("UPDATE watches SET status=?2 WHERE id=?1", params![id, status]);
}

/// 立即读价一次。
pub async fn check(state: &AppState, app: Option<&AppHandle>, id: i64) -> AppResult<WatchItem> {
    let w = get(&state.db, id)?;
    if w.status == "checking" {
        return Ok(w);
    }
    set_status(&state.db, id, "checking");
    if let Some(app) = app {
        let _ = app.emit("watch://updated", get(&state.db, id)?);
    }
    let plat = platforms::get(&state.db, &w.platform_id);
    let result: Result<(f64, String, Option<String>), String> = async {
        let pr = extract::probe(&state.browser, plat.as_ref(), &w.url).await?;
        if pr.login_wall {
            login::mark_wall(&state.db, &w.platform_id);
            return Err("商品页要求登录，请在「平台 · 账号与登录」重新登录".into());
        }
        let default_cur = plat.as_ref().map(|p| p.currency.clone()).unwrap_or_else(|| w.currency.clone());
        if let Some(p) = pr.price {
            return Ok((p, pr.currency.clone().unwrap_or(default_cur), Some(pr.title.clone())));
        }
        if let Some(p) = plat.as_ref() {
            for t in pr.big_prices.iter() {
                if let Some(hit) = prices_in(t, p).into_iter().next() {
                    return Ok((hit.value, hit.currency, Some(pr.title.clone())));
                }
            }
        }
        if !pr.text.trim().is_empty() && state.ai.configured() {
            let v: Value = state.ai.chat_json(prompts::PROBE_PRICE, &pr.text, true).await.map_err(|e| e.message)?;
            if let Some(p) = v["price"].as_f64().filter(|x| *x > 0.0) {
                let cur = v["currency"].as_str().filter(|s| s.len() == 3).map(|s| s.to_uppercase()).unwrap_or(default_cur);
                return Ok((p, cur, v["title"].as_str().map(String::from)));
            }
        }
        Err("没有在商品页识别到价格（可能已下架或页面结构变化）".into())
    }
    .await;
    let settings: AppSettings = crate::settings::load(&state.db);
    // 页面币种与追踪项币种不同（例如站点切换了显示币种）时，换算后再比较，避免混币计算涨跌
    let result = result.map(|(p, cur, title)| {
        if cur != w.currency {
            if let Some(conv) = state.fx.convert(p, &cur, &w.currency) {
                return (conv, w.currency.clone(), title);
            }
        }
        (p, cur, title)
    });
    match result {
        Ok((price, currency, title)) => {
            let now = now_iso();
            let (alert, prev) = {
                let c = state.db.conn();
                let prev: Option<(Option<f64>, Option<f64>, Option<f64>)> = c
                    .query_row("SELECT last_price, target_price, last_alert_price FROM watches WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .optional()?;
                let (last, target, last_alert) = prev.unwrap_or((None, None, None));
                if let Some(lid) = &w.listing_id {
                    c.execute("INSERT INTO price_points(listing_id, price, currency, source, captured_at) VALUES(?1,?2,?3,'detail',?4)", params![lid, price, currency, now])?;
                    c.execute("UPDATE listings SET price=?2, currency=?3, last_seen_at=?4 WHERE id=?1", params![lid, price, currency, now])?;
                }
                c.execute(
                    "UPDATE watches SET last_price=?2, currency=?3, first_price=COALESCE(first_price, ?2),
                       lowest=CASE WHEN lowest IS NULL OR ?2<lowest THEN ?2 ELSE lowest END,
                       highest=CASE WHEN highest IS NULL OR ?2>highest THEN ?2 ELSE highest END,
                       status='ok', last_error=NULL, fail_count=0, last_checked_at=?4, next_check_at=?5 WHERE id=?1",
                    params![id, price, currency, now, iso_in(w.interval_minutes)],
                )?;
                // 通过链接添加的追踪项，标题还是网址时，用商品页标题替换
                let placeholder = w.title.contains('/') && !w.title.contains(' ');
                if let Some(t) = title.filter(|t| placeholder && t.trim().chars().count() >= 6) {
                    c.execute("UPDATE watches SET title=?2 WHERE id=?1", params![id, t.chars().take(200).collect::<String>()])?;
                }
                let alert = if target.is_some_and(|t| price <= t) && last_alert.is_none_or(|a| price < a) {
                    Some(("target_hit", format!("「{}」到达目标价", w.title.chars().take(30).collect::<String>())))
                } else if last.is_some_and(|l| l > 0.0 && (l - price) / l * 100.0 >= settings.notify.drop_percent) && last_alert.is_none_or(|a| price < a) {
                    Some(("price_drop", format!("「{}」降价了", w.title.chars().take(30).collect::<String>())))
                } else {
                    None
                };
                if alert.is_some() {
                    c.execute("UPDATE watches SET last_alert_price=?2 WHERE id=?1", params![id, price])?;
                }
                (alert, last)
            };
            if let (Some((kind, title)), Some(app)) = (alert, app) {
                let body = match prev {
                    Some(l) => format!("{} {:.2} → {:.2}（{}）", currency, l, price, platforms::get(&state.db, &w.platform_id).map(|p| format!("{} {}", p.name, p.edition)).unwrap_or_default()),
                    None => format!("当前 {} {:.2}", currency, price),
                };
                crate::notify::push(app, &state.db, kind, "success", &title, &body, &format!("/track/watchlist?id={id}"), &w.url).await;
            }
        }
        Err(e) => {
            let fails: i64 = {
                let c = state.db.conn();
                c.execute(
                    "UPDATE watches SET status='error', last_error=?2, fail_count=fail_count+1, last_checked_at=?3, next_check_at=?4 WHERE id=?1",
                    params![id, e, now_iso(), iso_in(w.interval_minutes.min(180))],
                )?;
                c.query_row("SELECT fail_count FROM watches WHERE id=?1", [id], |r| r.get(0)).unwrap_or(0)
            };
            if fails == 3 {
                if let Some(app) = app {
                    crate::notify::push(app, &state.db, "error", "warning", &format!("「{}」连续 3 次读价失败", w.title.chars().take(30).collect::<String>()), &e, &format!("/track/watchlist?id={id}"), "").await;
                }
            }
        }
    }
    let item = get(&state.db, id)?;
    if let Some(app) = app {
        let _ = app.emit("watch://updated", &item);
    }
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_by_url_detects_platform_and_dedupes() {
        let db = Db::memory().unwrap();
        let w = add(&db, WatchAddInput { url: Some("https://www.amazon.de/dp/B09Y2MYL5C".into()), target_price: Some(250.0), ..Default::default() }).unwrap();
        assert_eq!(w.platform_id, "amazon_de");
        assert_eq!(w.currency, "EUR");
        assert_eq!(w.status, "new");
        let again = add(&db, WatchAddInput { url: Some("https://www.amazon.de/dp/B09Y2MYL5C".into()), ..Default::default() }).unwrap();
        assert_eq!(again.id, w.id);
        assert!(add(&db, WatchAddInput { url: Some("ftp://x".into()), ..Default::default() }).is_err());
        let u = update(&db, w.id, WatchPatch { target_price: Some(None), interval_minutes: Some(5), ..Default::default() }).unwrap();
        assert_eq!(u.target_price, None);
        assert_eq!(u.interval_minutes, 30);
        remove(&db, w.id).unwrap();
        assert!(get(&db, w.id).is_err());
    }
}
