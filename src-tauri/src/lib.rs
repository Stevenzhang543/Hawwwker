//! Hawwwker 货客 v26.01 — Tauri 内核。
#![allow(clippy::too_many_arguments, clippy::large_enum_variant)]

pub mod ai;
pub mod browser;
pub mod commands;
pub mod db;
pub mod error;
pub mod fx;
pub mod insights;
pub mod models;
pub mod monitors;
pub mod notify;
pub mod platforms;
pub mod scheduler;
pub mod search;
pub mod settings;
pub mod state;
pub mod tracker;

use state::AppState;
use tauri::Manager;

pub const VERSION: &str = "26.01";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let state = AppState::new(dir)?;
            match state.db.recover_after_crash() {
                Ok(n) if n > 0 => log::info!("marked {n} interrupted runs"),
                Ok(_) => {}
                Err(e) => log::warn!("recover failed: {}", e.message),
            }
            state.browser.cleanup_trash();
            app.manage(state.clone());
            scheduler::spawn(state, app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_bootstrap,
            commands::settings_update,
            commands::open_data_dir,
            commands::data_export,
            commands::fx_rates,
            commands::ai_get_config,
            commands::ai_save_config,
            commands::ai_test,
            commands::ai_presets,
            commands::ai_usage,
            commands::platforms_list,
            commands::platform_update,
            commands::platform_add_custom,
            commands::platform_remove_custom,
            commands::login_states,
            commands::login_start,
            commands::login_confirm,
            commands::login_cancel,
            commands::logout,
            commands::browser_info,
            commands::browser_show,
            commands::browser_wipe,
            commands::search_plan,
            commands::search_start,
            commands::search_cancel,
            commands::search_events,
            commands::search_runs,
            commands::search_run,
            commands::search_delete_run,
            commands::listing_get,
            commands::watch_list,
            commands::watch_add,
            commands::watch_update,
            commands::watch_remove,
            commands::watch_check,
            commands::watch_history,
            commands::monitor_list,
            commands::monitor_create,
            commands::monitor_update,
            commands::monitor_delete,
            commands::monitor_run,
            commands::notice_list,
            commands::notice_read,
            commands::notice_read_all,
            commands::insights_overview,
            commands::insights_trends,
            commands::insights_report,
            commands::copilot_skills,
            commands::copilot_history,
            commands::copilot_send,
            commands::copilot_reset,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Hawwwker")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let st = app.state::<AppState>().inner().clone();
                st.runs.cancel_all();
                tauri::async_runtime::block_on(async move {
                    let work = async {
                        if st.browser.alive().await {
                            let mut domains: Vec<String> = platforms::all(&st.db).into_iter().flat_map(|p| p.cookie_domains).collect();
                            domains.sort();
                            domains.dedup();
                            let _ = st.browser.persist_session_cookies(&domains).await;
                        }
                        st.browser.shutdown().await;
                    };
                    // 浏览器无响应时也不要拖住退出
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(8), work).await;
                });
            }
        });
}
