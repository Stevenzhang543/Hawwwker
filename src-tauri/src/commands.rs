//! IPC 命令（与 src/api/index.ts 一一对应）。

use crate::ai::{copilot, planner, presets};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::platforms;
use crate::search::{pipeline, rules};
use crate::state::AppState;
use crate::{insights, monitors, notify, settings, tracker};
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

type S<'a> = State<'a, AppState>;

/* ------------------------------------------------------------------ app & settings */
#[tauri::command]
pub async fn app_bootstrap(state: S<'_>) -> AppResult<Bootstrap> {
    let st = state.inner().clone();
    // 浏览器正在运行时顺带刷新一次登录状态（不为此启动浏览器）
    let logins = if st.browser.alive().await { st.logins.refresh(&st.db, &st.browser, false).await } else { st.logins.states(&st.db) };
    Ok(Bootstrap {
        version: crate::VERSION.into(),
        data_dir: st.data_dir.display().to_string(),
        settings: settings::load(&st.db),
        ai: st.ai.status(),
        platforms: platforms::infos(&st.db),
        logins,
        unread: notify::unread(&st.db),
        running_runs: st.runs.running(),
        fx: st.fx.snapshot(),
    })
}

#[tauri::command]
pub fn settings_update(state: S<'_>, patch: Value) -> AppResult<AppSettings> {
    let s = settings::update(&state.db, patch)?;
    state.browser.configure(s.search.background, s.search.concurrency);
    Ok(s)
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: S<'_>) -> AppResult<String> {
    let dir = state.data_dir.display().to_string();
    app.opener().open_path(&dir, None::<&str>).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(dir)
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[tauri::command]
pub fn data_export(app: AppHandle, state: S<'_>, kind: String) -> AppResult<String> {
    let dir = state.data_dir.join("exports");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{kind}-{stamp}.csv"));
    let mut out = String::from("\u{feff}"); // Excel 识别 UTF-8
    match kind.as_str() {
        "watches" => {
            out.push_str("标题,平台,币种,当前价,首次价,最低,最高,目标价,状态,链接\n");
            for w in tracker::list(&state.db)? {
                let f = |v: Option<f64>| v.map(|x| format!("{x:.2}")).unwrap_or_default();
                out.push_str(&format!("{},{},{},{},{},{},{},{},{},{}\n", csv_escape(&w.title), w.platform_id, w.currency, f(w.last_price), f(w.first_price),
                    f(w.lowest), f(w.highest), f(w.target_price), w.status, csv_escape(&w.url)));
            }
        }
        "listings" => {
            out.push_str("标题,平台,价格,币种,销量,评分,店铺,成色,首次出现,最近出现,链接\n");
            let c = state.db.conn();
            let mut stmt = c.prepare("SELECT title, platform_id, price, currency, sales, rating, shop, condition, first_seen_at, last_seen_at, url FROM listings ORDER BY last_seen_at DESC LIMIT 20000")?;
            let rows = stmt.query_map([], |r| {
                Ok(format!("{},{},{},{},{},{},{},{},{},{},{}\n", csv_escape(&r.get::<_, String>(0)?), r.get::<_, String>(1)?,
                    r.get::<_, Option<f64>>(2)?.map(|x| format!("{x:.2}")).unwrap_or_default(), r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?.map(|x| x.to_string()).unwrap_or_default(), r.get::<_, Option<f64>>(5)?.map(|x| x.to_string()).unwrap_or_default(),
                    csv_escape(&r.get::<_, Option<String>>(6)?.unwrap_or_default()), r.get::<_, String>(7)?, r.get::<_, String>(8)?, r.get::<_, String>(9)?,
                    csv_escape(&r.get::<_, String>(10)?)))
            })?;
            for r in rows.flatten() {
                out.push_str(&r);
            }
        }
        "runs" => {
            out.push_str("时间,指令,状态,抓取,保留,最低价,币种\n");
            for r in pipeline::list_runs(&state.db, 5000)? {
                out.push_str(&format!("{},{},{},{},{},{},{}\n", r.started_at, csv_escape(&r.instruction), r.status, r.fetched, r.kept,
                    r.best_price.map(|x| format!("{x:.2}")).unwrap_or_default(), r.currency));
            }
        }
        _ => return Err(AppError::invalid("未知的导出类型")),
    }
    std::fs::write(&path, out)?;
    let p = path.display().to_string();
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(p)
}

#[tauri::command]
pub fn fx_rates(state: S<'_>) -> FxRates {
    state.fx.snapshot()
}

/* ------------------------------------------------------------------ ai */
#[tauri::command]
pub fn ai_get_config(state: S<'_>) -> AiConfig {
    state.ai.config()
}

#[tauri::command]
pub fn ai_save_config(state: S<'_>, input: AiConfigInput) -> AppResult<AiConfig> {
    state.ai.save_config(input)
}

#[tauri::command]
pub async fn ai_test(state: S<'_>) -> AppResult<AiStatus> {
    Ok(state.ai.test().await)
}

#[tauri::command]
pub fn ai_presets() -> Vec<AiPreset> {
    presets::list()
}

#[tauri::command]
pub fn ai_usage(state: S<'_>) -> AppResult<AiUsage> {
    let c = state.db.conn();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let t: UsageTotals = c
        .query_row("SELECT calls, prompt_tokens, completion_tokens FROM ai_usage WHERE day=?1", [&today], |r| {
            Ok(UsageTotals { calls: r.get(0)?, prompt_tokens: r.get(1)?, completion_tokens: r.get(2)? })
        })
        .unwrap_or_default();
    let total: UsageTotals = c
        .query_row("SELECT COALESCE(SUM(calls),0), COALESCE(SUM(prompt_tokens),0), COALESCE(SUM(completion_tokens),0) FROM ai_usage", [], |r| {
            Ok(UsageTotals { calls: r.get(0)?, prompt_tokens: r.get(1)?, completion_tokens: r.get(2)? })
        })
        .unwrap_or_default();
    let mut stmt = c.prepare("SELECT day, calls, prompt_tokens+completion_tokens FROM ai_usage ORDER BY day DESC LIMIT 30")?;
    let mut days: Vec<UsageDay> = stmt.query_map([], |r| Ok(UsageDay { day: r.get(0)?, calls: r.get(1)?, tokens: r.get(2)? }))?.filter_map(|r| r.ok()).collect();
    days.reverse();
    Ok(AiUsage { today: t, total, days })
}

/* ------------------------------------------------------------------ platforms & login */
#[tauri::command]
pub fn platforms_list(state: S<'_>) -> Vec<PlatformInfo> {
    platforms::infos(&state.db)
}

#[tauri::command]
pub fn platform_update(state: S<'_>, id: String, patch: PlatformPatch) -> AppResult<PlatformInfo> {
    platforms::update(&state.db, &id, patch)
}

#[tauri::command]
pub fn platform_add_custom(state: S<'_>, input: CustomPlatformInput) -> AppResult<PlatformInfo> {
    platforms::add_custom(&state.db, input)
}

#[tauri::command]
pub fn platform_remove_custom(state: S<'_>, id: String) -> AppResult<()> {
    platforms::remove_custom(&state.db, &id)
}

#[tauri::command]
pub async fn login_states(state: S<'_>, refresh: bool) -> AppResult<Vec<LoginState>> {
    let st = state.inner().clone();
    Ok(st.logins.refresh(&st.db, &st.browser, refresh).await)
}

#[tauri::command]
pub fn login_start(app: AppHandle, state: S<'_>, platform_id: String) -> LoginState {
    let st = state.inner();
    st.logins.start(app, st.db.clone(), st.browser.clone(), &platform_id)
}

#[tauri::command]
pub async fn login_confirm(app: AppHandle, state: S<'_>, platform_id: String) -> AppResult<LoginState> {
    let st = state.inner().clone();
    Ok(st.logins.confirm(&app, &st.db, &st.browser, &platform_id).await)
}

#[tauri::command]
pub fn login_cancel(app: AppHandle, state: S<'_>, platform_id: String) -> LoginState {
    state.logins.cancel(&app, &state.db, &platform_id)
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: S<'_>, platform_id: String) -> AppResult<LoginState> {
    let st = state.inner().clone();
    st.logins.logout(&app, &st.db, &st.browser, &platform_id).await.map_err(AppError::browser)
}

#[tauri::command]
pub async fn browser_info(state: S<'_>) -> AppResult<BrowserInfo> {
    Ok(BrowserInfo {
        executable: state.browser.executable(),
        running: state.browser.alive().await,
        profile_dir: state.browser.profile_dir().display().to_string(),
    })
}

#[tauri::command]
pub async fn browser_show(state: S<'_>) -> AppResult<()> {
    state.browser.show_any().await.map_err(AppError::browser)
}

#[tauri::command]
pub async fn browser_wipe(app: AppHandle, state: S<'_>) -> AppResult<()> {
    let st = state.inner().clone();
    st.browser.wipe_profile().await.map_err(AppError::browser)?;
    st.db.conn().execute("UPDATE login_meta SET has_auth=0, verified_at=NULL, cookie_expires=NULL", [])?;
    for s in st.logins.states(&st.db) {
        let _ = tauri::Emitter::emit(&app, "login://state", s);
    }
    Ok(())
}

/* ------------------------------------------------------------------ search */
#[tauri::command]
pub async fn search_plan(state: S<'_>, instruction: String) -> AppResult<SearchPlan> {
    let st = state.inner().clone();
    if instruction.trim().is_empty() {
        return Err(AppError::invalid("请输入要找的商品"));
    }
    let s = settings::load(&st.db);
    let all = platforms::all(&st.db);
    let ctx = planner::PlanCtx { platforms: &all, settings: &s };
    if st.ai.require().is_err() {
        return Ok(planner::rule_plan(&instruction, &rules::parse(&instruction), &ctx));
    }
    planner::plan(&st.ai, &instruction, &ctx).await
}

#[tauri::command]
pub fn search_start(app: AppHandle, state: S<'_>, instruction: String, plan: Option<SearchPlan>) -> AppResult<StartResult> {
    let (h, plan, _rx) = pipeline::start(state.inner(), Some(app), instruction, plan, "manual", None)?;
    Ok(StartResult { run_id: h.id.clone(), plan })
}

#[tauri::command]
pub fn search_cancel(state: S<'_>, run_id: String) -> AppResult<()> {
    if let Some(h) = state.runs.get(&run_id) {
        h.cancel.cancel();
    }
    Ok(())
}

#[tauri::command]
pub fn search_events(state: S<'_>, run_id: String, since: u64) -> AppResult<Vec<RunEvent>> {
    if let Some(h) = state.runs.get(&run_id) {
        return Ok(h.since(since));
    }
    // 不在内存中（例如程序重启后）：用数据库里的结果合成终态事件
    let s = settings::load(&state.db);
    let d = pipeline::run_detail(&state.db, &state.fx, &run_id, &s.display_currency)?;
    let mut bodies = vec![RunEventBody::Plan { plan: d.plan.clone() }];
    if !d.results.is_empty() {
        bodies.push(RunEventBody::Results { items: d.results.clone() });
    }
    if let Some(sm) = d.ai_summary.clone() {
        bodies.push(RunEventBody::Summary { summary: sm });
    }
    bodies.push(match d.summary.status.as_str() {
        "done" => RunEventBody::Done { stats: d.stats.clone().unwrap_or_default() },
        "cancelled" => RunEventBody::Cancelled,
        "running" => RunEventBody::Error { message: "运行状态丢失".into() },
        _ => RunEventBody::Error { message: d.summary.error.clone().unwrap_or_else(|| "搜索未完成".into()) },
    });
    let t = chrono::Utc::now().timestamp_millis();
    Ok(bodies.into_iter().enumerate().map(|(i, body)| RunEvent { body, seq: i as u64 + 1, t }).filter(|e| e.seq > since).collect())
}

#[tauri::command]
pub fn search_runs(state: S<'_>, limit: i64) -> AppResult<Vec<RunSummary>> {
    pipeline::list_runs(&state.db, limit.clamp(1, 500))
}

#[tauri::command]
pub fn search_run(state: S<'_>, run_id: String) -> AppResult<RunDetail> {
    let s = settings::load(&state.db);
    pipeline::run_detail(&state.db, &state.fx, &run_id, &s.display_currency)
}

#[tauri::command]
pub fn search_delete_run(state: S<'_>, run_id: String) -> AppResult<()> {
    if let Some(h) = state.runs.get(&run_id) {
        h.cancel.cancel();
    }
    pipeline::delete_run(&state.db, &run_id)
}

#[tauri::command]
pub fn listing_get(state: S<'_>, id: String) -> AppResult<ListingDetail> {
    let s = settings::load(&state.db);
    pipeline::listing_detail(&state.db, &state.fx, &id, &s.display_currency)
}

/* ------------------------------------------------------------------ watch */
#[tauri::command]
pub fn watch_list(state: S<'_>) -> AppResult<Vec<WatchItem>> {
    tracker::list(&state.db)
}

#[tauri::command]
pub fn watch_add(app: AppHandle, state: S<'_>, input: WatchAddInput) -> AppResult<WatchItem> {
    let w = tracker::add(&state.db, input)?;
    let _ = tauri::Emitter::emit(&app, "watch://updated", &w);
    Ok(w)
}

#[tauri::command]
pub fn watch_update(state: S<'_>, id: i64, patch: WatchPatch) -> AppResult<WatchItem> {
    tracker::update(&state.db, id, patch)
}

#[tauri::command]
pub fn watch_remove(state: S<'_>, id: i64) -> AppResult<()> {
    tracker::remove(&state.db, id)
}

#[tauri::command]
pub async fn watch_check(app: AppHandle, state: S<'_>, id: i64) -> AppResult<WatchItem> {
    let st = state.inner().clone();
    tracker::check(&st, Some(&app), id).await
}

#[tauri::command]
pub fn watch_history(state: S<'_>, id: i64, days: i64) -> AppResult<Vec<PricePoint>> {
    tracker::history(&state.db, id, days)
}

/* ------------------------------------------------------------------ monitors */
#[tauri::command]
pub fn monitor_list(state: S<'_>) -> AppResult<Vec<Monitor>> {
    monitors::list(&state.db)
}

#[tauri::command]
pub fn monitor_create(state: S<'_>, input: MonitorCreateInput) -> AppResult<Monitor> {
    monitors::create(&state.db, input)
}

#[tauri::command]
pub fn monitor_update(state: S<'_>, id: i64, patch: MonitorPatch) -> AppResult<Monitor> {
    monitors::update(&state.db, id, patch)
}

#[tauri::command]
pub fn monitor_delete(state: S<'_>, id: i64) -> AppResult<()> {
    monitors::delete(&state.db, id)
}

#[tauri::command]
pub fn monitor_run(app: AppHandle, state: S<'_>, id: i64) -> AppResult<IdResult> {
    let run_id = monitors::run(state.inner(), Some(app), id)?;
    Ok(IdResult { run_id: Some(run_id), message_id: None, report_id: None })
}

/* ------------------------------------------------------------------ notices */
#[tauri::command]
pub fn notice_list(state: S<'_>, limit: i64) -> AppResult<Vec<Notice>> {
    notify::list(&state.db, limit.clamp(1, 500))
}

#[tauri::command]
pub fn notice_read(state: S<'_>, id: i64) -> AppResult<()> {
    state.db.conn().execute("UPDATE notices SET read=1 WHERE id=?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn notice_read_all(state: S<'_>) -> AppResult<()> {
    state.db.conn().execute("UPDATE notices SET read=1 WHERE read=0", [])?;
    Ok(())
}

/* ------------------------------------------------------------------ insights */
#[tauri::command]
pub fn insights_overview(state: S<'_>, days: i64) -> AppResult<MarketOverview> {
    let s = settings::load(&state.db);
    insights::overview(&state.db, &state.fx, days.clamp(1, 365), &s.display_currency)
}

#[tauri::command]
pub fn insights_trends(state: S<'_>, watch_ids: Vec<i64>, monitor_ids: Vec<i64>, days: i64) -> AppResult<Vec<TrendSeries>> {
    let s = settings::load(&state.db);
    insights::trends(&state.db, &state.fx, &watch_ids, &monitor_ids, days.clamp(1, 365), &s.display_currency)
}

#[tauri::command]
pub fn insights_report(app: AppHandle, state: S<'_>, scope: String, days: i64) -> AppResult<IdResult> {
    let id = insights::report(state.inner(), app, scope, days.clamp(1, 365))?;
    Ok(IdResult { run_id: None, message_id: None, report_id: Some(id) })
}

/* ------------------------------------------------------------------ copilot */
#[tauri::command]
pub fn copilot_skills() -> Vec<CopilotSkill> {
    copilot::skills()
}

#[tauri::command]
pub fn copilot_history(state: S<'_>, session_id: String) -> AppResult<Vec<CopilotMessage>> {
    copilot::history(&state.db, &session_id)
}

#[tauri::command]
pub fn copilot_send(app: AppHandle, state: S<'_>, input: CopilotSendInput) -> AppResult<IdResult> {
    let role = settings::load(&state.db).role;
    let id = copilot::send(app, state.db.clone(), state.ai.clone(), role, input)?;
    Ok(IdResult { run_id: None, message_id: Some(id), report_id: None })
}

#[tauri::command]
pub fn copilot_reset(state: S<'_>, session_id: String) -> AppResult<()> {
    copilot::reset(&state.db, &session_id)
}
