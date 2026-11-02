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
        transport: state.browser.transport().to_string(),
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

/* ------------------------------------------------------------------ v26.02 */
/// 粘贴商品链接：识别平台，打开商品页读取标题与价格，给出"全网找同款"指令。
#[tauri::command]
pub async fn link_inspect(state: S<'_>, url: String) -> AppResult<LinkInfo> {
    let st = state.inner().clone();
    let url = url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::invalid("请粘贴以 http:// 或 https:// 开头的商品链接"));
    }
    let all = platforms::all(&st.db);
    let plat = all.iter().find(|p| p.host_matches(&url)).cloned();
    let watch_id: Option<i64> = st.db.conn().query_row("SELECT id FROM watches WHERE url=?1 LIMIT 1", [&url], |r| r.get(0)).ok();
    let mut info = LinkInfo {
        url: url.clone(),
        platform_id: plat.as_ref().map(|p| p.id.clone()),
        platform_name: plat.as_ref().map(|p| format!("{} {}", p.name, p.edition)),
        watch_id,
        ..Default::default()
    };
    match crate::browser::extract::probe(&st.browser, plat.as_ref(), &url).await {
        Ok(pr) => {
            if pr.login_wall {
                info.message = Some("商品页要求登录，可先到「平台 · 账号」登录；也可以直接用链接追踪".into());
            }
            let title = pr.title.trim().to_string();
            if !title.is_empty() && !title.starts_with("http") {
                info.title = Some(title.chars().take(160).collect());
            }
            info.price = pr.price.or_else(|| {
                plat.as_ref().and_then(|p| pr.big_prices.iter().find_map(|t| crate::search::parse::prices_in(t, p).into_iter().next().map(|h| h.value)))
            });
            info.currency = pr.currency.clone().or_else(|| plat.as_ref().map(|p| p.currency.clone()));
            info.image = Some(pr.image.clone()).filter(|i| i.starts_with("http"));
        }
        Err(e) => info.message = Some(format!("没能打开商品页：{e}")),
    }
    let short: String = info.title.clone().unwrap_or_default().chars().take(60).collect();
    info.instruction = if short.is_empty() { String::new() } else { format!("全网找同款比价：{short}") };
    Ok(info)
}

/// 结果反馈：irrelevant = 标记不相关（同一需求以后自动折叠）；clear = 撤销。
#[tauri::command]
pub fn feedback_set(state: S<'_>, listing_id: String, run_id: String, kind: String) -> AppResult<()> {
    if kind != "irrelevant" && kind != "clear" {
        return Err(AppError::invalid("未知的反馈类型"));
    }
    let (platform_id, external_id) = if let Some(rest) = listing_id.strip_prefix("p:") {
        let mut it = rest.splitn(2, ':');
        (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string())
    } else {
        state.db.conn().query_row("SELECT platform_id, external_id FROM listings WHERE id=?1", [&listing_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
    };
    let plan: SearchPlan = state
        .db
        .conn()
        .query_row("SELECT plan FROM runs WHERE id=?1", [&run_id], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|p| serde_json::from_str(&p).ok())
        .ok_or_else(|| AppError::not_found("搜索记录"))?;
    crate::deal::set_feedback(&state.db, &platform_id, &external_id, &crate::deal::scope_of(&plan), &kind)?;
    Ok(())
}

/// 导出一次搜索：csv = 写文件并返回路径；text = 返回可直接粘贴的比价清单。
#[tauri::command]
pub fn run_export(app: AppHandle, state: S<'_>, run_id: String, format: String) -> AppResult<String> {
    let s = settings::load(&state.db);
    let d = pipeline::run_detail(&state.db, &state.fx, &run_id, &s.display_currency)?;
    let items: Vec<&ResultItem> = d.results.iter().filter(|r| r.match_info.verdict != "mismatch" && r.feedback.is_none()).collect();
    let all = platforms::all(&state.db);
    let pname = |id: &str| all.iter().find(|p| p.id == id).map(|p| format!("{} {}", p.name, p.edition)).unwrap_or_else(|| id.to_string());
    let money = |v: Option<f64>| v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "-".into());
    if format == "text" {
        let mut out = format!("【Hawwwker 比价】{}\n", d.summary.instruction);
        if let Some(ai) = &d.ai_summary {
            out.push_str(&format!("结论：{}\n", ai.headline));
        }
        out.push_str(&format!("（到手价已换算为 {}，含运费与券后价）\n\n", s.display_currency));
        for (i, r) in items.iter().take(15).enumerate() {
            out.push_str(&format!(
                "{}. {} | {} | 到手 {} {} | 标价 {} {} | {}\n   {}\n",
                i + 1,
                pname(&r.platform_id),
                r.title.chars().take(50).collect::<String>(),
                s.display_currency,
                money(r.display_landed.or(r.display_price)),
                r.currency,
                money(r.price),
                r.deal.as_ref().map(|x| x.label.clone()).unwrap_or_default(),
                r.url
            ));
        }
        return Ok(out);
    }
    let dir = state.data_dir.join("exports");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("search-{}.csv", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let mut out = String::from("\u{feff}排名,平台,标题,标价,币种,运费,券后价,到手价,到手价(显示币种),匹配,匹配分,价格判断,店铺,链接\n");
    for r in items {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.rank,
            csv_escape(&pname(&r.platform_id)),
            csv_escape(&r.title),
            money(r.price),
            r.currency,
            money(r.shipping),
            money(r.coupon_price),
            money(r.landed_price),
            money(r.display_landed),
            r.match_info.verdict,
            r.match_info.score,
            r.deal.as_ref().map(|x| x.label.clone()).unwrap_or_default(),
            csv_escape(r.shop.as_deref().unwrap_or("")),
            csv_escape(&r.url)
        ));
    }
    std::fs::write(&path, out)?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}

/// 检测本机 AI 服务（Ollama / LM Studio），用于零成本上手。
#[tauri::command]
pub async fn ai_detect_local() -> LocalAi {
    let client = match reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_millis(1500)).build() {
        Ok(c) => c,
        Err(_) => return LocalAi::default(),
    };
    if let Ok(r) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if let Ok(v) = r.json::<Value>().await {
            let models: Vec<String> = v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect()).unwrap_or_default();
            return LocalAi { found: true, base_url: "http://localhost:11434/v1".into(), models };
        }
    }
    if let Ok(r) = client.get("http://127.0.0.1:1234/v1/models").send().await {
        if let Ok(v) = r.json::<Value>().await {
            let models: Vec<String> = v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default();
            return LocalAi { found: true, base_url: "http://localhost:1234/v1".into(), models };
        }
    }
    LocalAi::default()
}

/// 按所在地区推荐平台（引导第 4 步）。
#[tauri::command]
pub fn platforms_recommend(timezone: String, language: String) -> Recommendation {
    let tz = timezone.to_lowercase();
    let cn_core = ["taobao", "jd", "pinduoduo", "xianyu"];
    let (region, mut ids, reason): (&str, Vec<&str>, &str) = if tz.starts_with("asia/shanghai") || tz.starts_with("asia/chongqing") || tz.starts_with("asia/urumqi") {
        ("cn", vec!["taobao", "jd", "pinduoduo", "xianyu", "xiaohongshu", "douyin"], "你在中国大陆：推荐国内主流电商与种草平台")
    } else if tz.starts_with("europe/berlin") || tz.starts_with("europe/vienna") || tz.starts_with("europe/zurich") {
        ("all", vec!["amazon_de", "ebay_de", "idealo", "kleinanzeigen"], "你在德语区：推荐 Amazon.de、eBay.de、idealo 比价与 Kleinanzeigen 二手，并保留国内平台")
    } else if tz.starts_with("europe/paris") {
        ("all", vec!["amazon_fr", "aliexpress", "temu"], "你在法国：推荐 Amazon.fr 与跨境平台，并保留国内平台")
    } else if tz.starts_with("europe/london") {
        ("all", vec!["amazon_co_uk", "ebay_co_uk", "aliexpress"], "你在英国：推荐 Amazon.co.uk、eBay.co.uk，并保留国内平台")
    } else if tz.starts_with("asia/tokyo") {
        ("all", vec!["amazon_co_jp", "rakuten", "mercari_jp"], "你在日本：推荐 Amazon.co.jp、乐天与 Mercari，并保留国内平台")
    } else if tz.starts_with("america/") {
        ("all", vec!["amazon_com", "ebay_com", "walmart", "bestbuy"], "你在美洲：推荐 Amazon.com、eBay、Walmart、Best Buy，并保留国内平台")
    } else if tz.starts_with("europe/") {
        ("all", vec!["amazon_de", "ebay_de", "aliexpress", "temu"], "你在欧洲：推荐欧洲站点与跨境平台，并保留国内平台")
    } else {
        ("all", vec!["amazon_com", "aliexpress", "temu"], "推荐通用国际站点，并保留国内平台")
    };
    if region != "cn" && language.to_lowercase().starts_with("zh") {
        ids.extend(cn_core);
    }
    Recommendation { region: region.into(), ids: ids.into_iter().map(String::from).collect(), reason: reason.into() }
}

/// 平台自检：在真实网络上做一次小搜索，看能否识别结果与价格。
#[tauri::command]
pub async fn platform_selftest(state: S<'_>, ids: Vec<String>, query: Option<String>) -> AppResult<Vec<SelfTestResult>> {
    let st = state.inner().clone();
    let all = platforms::all(&st.db);
    let mut out = vec![];
    for id in ids.iter().take(12) {
        let Some(p) = all.iter().find(|p| &p.id == id) else { continue };
        out.push(crate::selftest::run_one(&st.browser, p, query.as_deref()).await);
    }
    Ok(out)
}
