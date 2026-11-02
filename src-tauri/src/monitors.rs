//! 定时监控：保存的搜索计划，按间隔重跑；发现新的匹配商品或最低价低于目标价时提醒。

use crate::db::{iso_in, now_iso, Db};
use crate::error::{AppError, AppResult};
use crate::models::{Monitor, MonitorCreateInput, MonitorPatch, SearchPlan};
use crate::search::pipeline;
use crate::state::AppState;
use rusqlite::{params, OptionalExtension, Row};
use tauri::AppHandle;

const COLS: &str = "id, name, instruction, plan, interval_minutes, enabled, target_price, last_run_at, next_run_at, last_status,
    last_new_count, run_count, last_run_id, best_price, currency, created_at";

fn row(r: &Row) -> rusqlite::Result<Monitor> {
    Ok(Monitor {
        id: r.get(0)?,
        name: r.get(1)?,
        instruction: r.get(2)?,
        plan: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
        interval_minutes: r.get(4)?,
        enabled: r.get::<_, i64>(5)? != 0,
        target_price: r.get(6)?,
        last_run_at: r.get(7)?,
        next_run_at: r.get(8)?,
        last_status: r.get(9)?,
        last_new_count: r.get(10)?,
        run_count: r.get(11)?,
        last_run_id: r.get(12)?,
        best_price: r.get(13)?,
        currency: r.get(14)?,
        created_at: r.get(15)?,
    })
}

pub fn get(db: &Db, id: i64) -> AppResult<Monitor> {
    db.conn()
        .query_row(&format!("SELECT {COLS} FROM monitors WHERE id=?1"), [id], row)
        .optional()?
        .ok_or_else(|| AppError::not_found("监控"))
}

pub fn list(db: &Db) -> AppResult<Vec<Monitor>> {
    let c = db.conn();
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM monitors ORDER BY created_at DESC"))?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create(db: &Db, input: MonitorCreateInput) -> AppResult<Monitor> {
    let instruction = input.instruction.trim().to_string();
    let plan: SearchPlan = input.plan.unwrap_or_else(|| SearchPlan { instruction: instruction.clone(), ..Default::default() });
    if instruction.is_empty() && plan.keywords.is_empty() {
        return Err(AppError::invalid("监控需要一条搜索指令"));
    }
    let name = input.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        let base = if !plan.keywords.is_empty() { plan.keywords.join(" ") } else { instruction.clone() };
        base.chars().take(30).collect()
    });
    let interval = input.interval_minutes.clamp(30, 10080);
    let currency = crate::settings::load(db).display_currency;
    let id = {
        let c = db.conn();
        c.execute(
            "INSERT INTO monitors(name, instruction, plan, interval_minutes, enabled, target_price, next_run_at, last_status, currency, created_at)
             VALUES(?1,?2,?3,?4,1,?5,?6,'idle',?7,?8)",
            params![name, instruction, serde_json::to_string(&plan)?, interval, input.target_price, iso_in(interval), currency, now_iso()],
        )?;
        c.last_insert_rowid()
    };
    get(db, id)
}

pub fn update(db: &Db, id: i64, patch: MonitorPatch) -> AppResult<Monitor> {
    get(db, id)?;
    let c = db.conn();
    if let Some(n) = patch.name.filter(|n| !n.trim().is_empty()) {
        c.execute("UPDATE monitors SET name=?2 WHERE id=?1", params![id, n.trim()])?;
    }
    if let Some(i) = patch.interval_minutes {
        let i = i.clamp(30, 10080);
        c.execute("UPDATE monitors SET interval_minutes=?2, next_run_at=?3 WHERE id=?1", params![id, i, iso_in(i)])?;
    }
    if let Some(e) = patch.enabled {
        c.execute("UPDATE monitors SET enabled=?2 WHERE id=?1", params![id, e as i64])?;
    }
    if let Some(t) = patch.target_price {
        c.execute("UPDATE monitors SET target_price=?2 WHERE id=?1", params![id, t])?;
    }
    drop(c);
    get(db, id)
}

pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    db.conn().execute("DELETE FROM monitors WHERE id=?1", [id])?;
    Ok(())
}

/// 运行一次监控：返回运行 id；结果评估在后台完成。
pub fn run(state: &AppState, app: Option<AppHandle>, id: i64) -> AppResult<String> {
    let m = get(&state.db, id)?;
    if m.last_status == "running" {
        if let Some(rid) = m.last_run_id.clone().filter(|r| state.runs.get(r).is_some_and(|h| !h.is_finished())) {
            return Ok(rid);
        }
    }
    let plan = if m.plan.keywords.is_empty() { None } else { Some(m.plan.clone()) };
    let (handle, _plan, rx) = pipeline::start(state, app.clone(), m.instruction.clone(), plan, "monitor", Some(id))?;
    state.db.conn().execute("UPDATE monitors SET last_status='running', last_run_id=?2 WHERE id=?1", params![id, handle.id])?;
    let st = state.clone();
    let run_id = handle.id.clone();
    let rid = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let run_id = rid;
        let outcome = rx.await.unwrap_or_default();
        let ok = outcome.status == "done";
        let fresh: Vec<&crate::models::ResultItem> = outcome
            .results
            .iter()
            .filter(|r| outcome.new_ids.contains(&r.id) && (r.match_info.verdict == "exact" || r.match_info.verdict == "close"))
            .collect();
        let first_run = m.run_count == 0;
        let _ = st.db.conn().execute(
            "UPDATE monitors SET last_status=?2, last_run_at=?3, next_run_at=?4, run_count=run_count+1, last_new_count=?5,
               best_price=COALESCE(?6, best_price), currency=COALESCE(NULLIF(?7,''), currency) WHERE id=?1",
            params![id, if ok { "ok" } else { "error" }, now_iso(), iso_in(m.interval_minutes), if first_run { 0 } else { fresh.len() as i64 },
                    outcome.best_price, outcome.currency],
        );
        let Some(app) = app else { return };
        let link = format!("/discover/history?run={run_id}");
        if !ok {
            if outcome.status != "cancelled" {
                crate::notify::push(&app, &st.db, "error", "warning", &format!("监控「{}」本次运行失败", m.name), outcome.error.as_deref().unwrap_or(""), &link, "").await;
            }
            return;
        }
        // 目标价按监控创建时的币种理解；显示币种变化时先换算
        let best_in_m = outcome.best_price.and_then(|b| st.fx.convert(b, &outcome.currency, &m.currency));
        let prev_in_m = m.best_price;
        if let (Some(t), Some(best)) = (m.target_price, best_in_m) {
            if best <= t && prev_in_m.is_none_or(|prev| best < prev) {
                crate::notify::push(&app, &st.db, "target_hit", "success", &format!("「{}」出现目标价以内的商品", m.name), &format!("最低 {} {:.2}（目标 {:.2}）", outcome.currency, best, t), &link, "").await;
                return;
            }
        }
        if !first_run && !fresh.is_empty() {
            let body = fresh.iter().take(3).map(|r| format!("· {}", r.title.chars().take(36).collect::<String>())).collect::<Vec<_>>().join("\n");
            crate::notify::push(&app, &st.db, "new_items", "info", &format!("「{}」发现 {} 个新商品", m.name, fresh.len()), &body, &link, "").await;
        }
    });
    Ok(run_id)
}
