//! 调度器：每 30 秒扫描到期的追踪项与监控并派发为独立任务（不阻塞），定期保活与刷新汇率。

use crate::db::now_iso;
use crate::state::AppState;
use crate::{browser, monitors, settings, tracker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::Semaphore;

pub fn spawn(state: AppState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        let watch_sem = Arc::new(Semaphore::new(2));
        let monitor_busy = Arc::new(AtomicBool::new(false));
        let mut last_keepalive = Instant::now();
        loop {
            // 汇率
            if state.fx.needs_refresh(&state.db) {
                if let Err(e) = state.fx.refresh(&state.db, &state.http).await {
                    log::info!("fx refresh skipped: {e}");
                }
            }
            // 到期的追踪项
            let due_watches: Vec<i64> = {
                let c = state.db.conn();
                c.prepare("SELECT id FROM watches WHERE enabled=1 AND status<>'checking' AND (next_check_at IS NULL OR next_check_at<=?1) ORDER BY next_check_at LIMIT 4")
                    .and_then(|mut s| s.query_map([now_iso()], |r| r.get(0)).map(|rows| rows.filter_map(|r| r.ok()).collect()))
                    .unwrap_or_default()
            };
            for id in due_watches {
                let Ok(permit) = watch_sem.clone().try_acquire_owned() else { break };
                let st = state.clone();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _p = permit;
                    if let Err(e) = tracker::check(&st, Some(&app), id).await {
                        log::warn!("watch {id} check failed: {}", e.message);
                    }
                });
            }
            // 到期的监控（同一时刻只跑一个，且 AI 必须可用）
            if state.ai.require().is_ok() && !monitor_busy.load(Ordering::SeqCst) {
                let due: Option<i64> = state
                    .db
                    .conn()
                    .query_row(
                        "SELECT id FROM monitors WHERE enabled=1 AND last_status<>'running' AND (next_run_at IS NULL OR next_run_at<=?1) ORDER BY next_run_at LIMIT 1",
                        [now_iso()],
                        |r| r.get(0),
                    )
                    .ok();
                if let Some(id) = due {
                    match monitors::run(&state, Some(app.clone()), id) {
                        Ok(run_id) => {
                            monitor_busy.store(true, Ordering::SeqCst);
                            let busy = monitor_busy.clone();
                            let st = state.clone();
                            tauri::async_runtime::spawn(async move {
                                for _ in 0..(16 * 60 / 5) {
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                    if st.runs.get(&run_id).is_none_or(|h| h.is_finished()) {
                                        break;
                                    }
                                }
                                busy.store(false, Ordering::SeqCst);
                            });
                        }
                        Err(e) => {
                            log::warn!("monitor {id} failed to start: {}", e.message);
                            let _ = state.db.conn().execute(
                                "UPDATE monitors SET last_status='error', next_run_at=?2 WHERE id=?1",
                                rusqlite::params![id, crate::db::iso_in(60)],
                            );
                        }
                    }
                }
            }
            // 保活
            let s = settings::load(&state.db);
            if s.search.keepalive && last_keepalive.elapsed() > Duration::from_secs(6 * 3600) && state.runs.running().is_empty() {
                last_keepalive = Instant::now();
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let touched = browser::keepalive(&st.db, &st.browser).await;
                    if !touched.is_empty() {
                        log::info!("keepalive: {:?}", touched);
                    }
                });
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}
