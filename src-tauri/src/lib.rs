//! Hawwwker 货客 v26.01 — Tauri 内核。
#![allow(clippy::too_many_arguments, clippy::large_enum_variant)]

pub mod ai;
pub mod browser;
pub mod commands;
pub mod db;
pub mod deal;
pub mod error;
pub mod fx;
pub mod insights;
pub mod models;
pub mod monitors;
pub mod notify;
pub mod platforms;
pub mod scheduler;
pub mod search;
pub mod selftest;
pub mod settings;
pub mod state;
pub mod tracker;

use state::AppState;
use tauri::Manager;

pub const VERSION: &str = "26.03";

/// 命令行：`hawwwker --selftest-sites <报告.md> [平台id…]`
/// 用独立的临时浏览器目录对平台做真实小搜索，写出报告后退出（不打开主窗口、不读取你的登录会话）。
fn selftest_cli(args: &[String]) -> bool {
    let Some(pos) = args.iter().position(|a| a == "--selftest-sites") else { return false };
    let out = args.get(pos + 1).cloned().unwrap_or_else(|| "hawwwker-selftest.md".into());
    let wanted: Vec<String> = args.iter().skip(pos + 2).cloned().collect();
    tauri::async_runtime::block_on(async move {
        let dir = std::env::temp_dir().join(format!("hawwwker-selftest-{}", std::process::id()));
        let browser = std::sync::Arc::new(browser::manager::Browser::new(dir.clone()));
        browser.configure(true, 3);
        let list: Vec<platforms::Platform> = platforms::BUILTIN
            .iter()
            .filter(|p| wanted.is_empty() || wanted.iter().any(|w| w == &p.id))
            .cloned()
            .collect();
        let futs = list.iter().map(|p| selftest::run_one(&browser, p, None));
        let results = futures_util::future::join_all(futs).await;
        let md = selftest::report_markdown(&results, &list);
        let _ = std::fs::write(&out, md);
        let _ = std::fs::write(format!("{out}.json"), serde_json::to_string_pretty(&results).unwrap_or_default());
        browser.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    });
    true
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if selftest_cli(&args) {
        return;
    }
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
            // 便携版启动器通过 HAWWWKER_DATA_DIR 把数据放在 exe 旁边；安装版使用系统应用数据目录
            let dir = match std::env::var_os("HAWWWKER_DATA_DIR").filter(|v| !v.is_empty()) {
                Some(d) => std::path::PathBuf::from(d),
                None => app.path().app_data_dir()?,
            };
            log::info!("data dir: {}", dir.display());
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
            commands::ai_list_models,
            commands::ai_usage,
            commands::platforms_list,
            commands::platform_update,
            commands::platform_add_custom,
            commands::platform_remove_custom,
            commands::login_states,
            commands::login_preflight,
            commands::login_check_all,
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
            commands::link_inspect,
            commands::feedback_set,
            commands::run_export,
            commands::ai_detect_local,
            commands::platforms_recommend,
            commands::platform_selftest,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Hawwwker")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let st = app.state::<AppState>().inner().clone();
                st.runs.cancel_all();
                tauri::async_runtime::block_on(async move {
                    // 1) 备份与会话 Cookie 持久化：最多 4 秒（浏览器无响应时不拖住退出）
                    let backup = async {
                        if st.browser.alive().await {
                            // v26.03：退出前备份已登录（且没有正在登录）的平台会话
                            let candidates = st.logins.backup_candidates(&st.db);
                            let refs: Vec<&platforms::Platform> = candidates.iter().collect();
                            crate::browser::vault::snapshot(&st.db, &st.browser, &refs).await;
                            let mut domains: Vec<String> = platforms::all(&st.db).into_iter().flat_map(|p| p.cookie_domains).collect();
                            domains.sort();
                            domains.dedup();
                            let _ = st.browser.persist_session_cookies(&domains).await;
                        }
                    };
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(4), backup).await;
                    // 2) 正常关闭浏览器（让它把 Cookie 数据库写盘）：单独的预算，不被上一步挤占
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(7), st.browser.shutdown()).await;
                });
            }
        });
}
