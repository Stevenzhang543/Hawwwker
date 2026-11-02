//! 搜索管线（v26.03）：AI 理解需求 → 平台判定 → 多平台搜索 → 粗筛（需求匹配度）→ 商品风险分析 → 店铺可信度
//! → 评论检索 → 评论分析与匹配 → 最终细筛 → AI 总结。每个阶段完成后推送 Results（同一 id 原位更新）与 Funnel。
//! 每次运行都有取消令牌、总超时与终态事件；任何异常都会把运行记录落为 error（修复 v3.4 的"永远 RUNNING"）。

use super::filter::{self, clusters, rule_match};
use super::parse::Candidate;
use super::reviews;
use super::risk;
use super::shop::{self, ShopInput};
use super::{rules, RunHandle};
use crate::ai::analysis;
use crate::ai::planner::{self, PlanCtx};
use crate::ai::verify;
use crate::browser::extract::{self, Outcome, SearchOpts};
use crate::browser::login;
use crate::db::{now_iso, Db};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::platforms::{self, Platform};
use crate::settings;
use crate::state::AppState;
use futures_util::future::join_all;
use rusqlite::{params, OptionalExtension};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// 运行结束后的摘要（供监控调度使用）。
#[derive(Debug, Clone, Default)]
pub struct RunOutcome {
    pub status: String,
    pub results: Vec<ResultItem>,
    pub new_ids: Vec<String>,
    pub best_price: Option<f64>,
    pub currency: String,
    pub error: Option<String>,
}

fn stage(h: &RunHandle, app: Option<&AppHandle>, stage: &str, label: &str, progress: f64) {
    h.emit(app, RunEventBody::Stage { stage: stage.into(), label: label.into(), progress });
}

/// 创建运行并在后台执行；立即返回运行 id 与（暂定）计划。
pub fn start(state: &AppState, app: Option<AppHandle>, instruction: String, plan: Option<SearchPlan>, trigger: &str, monitor_id: Option<i64>) -> AppResult<(Arc<RunHandle>, SearchPlan, tokio::sync::oneshot::Receiver<RunOutcome>)> {
    state.ai.require()?;
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() && plan.as_ref().is_none_or(|p| p.keywords.is_empty()) {
        return Err(AppError::invalid("先说说你想找什么，例如：「Switch 2 哪里买最便宜」"));
    }
    let s = settings::load(&state.db);
    let all = platforms::all(&state.db);
    let ctx = PlanCtx { platforms: &all, settings: &s };
    let (provisional, needs_ai) = match plan {
        Some(mut p) => {
            if p.instruction.trim().is_empty() {
                p.instruction = instruction.clone();
            }
            (planner::normalize(p, &ctx, None), false)
        }
        None => (planner::rule_plan(&instruction, &rules::parse(&instruction), &ctx), true),
    };
    if provisional.platforms.is_empty() {
        return Err(AppError::invalid("没有可用的平台：请在「平台 · 平台目录」启用至少一个平台"));
    }
    let run_id = uuid::Uuid::new_v4().to_string();
    state.db.conn().execute(
        "INSERT INTO runs(id, instruction, plan, status, trigger, monitor_id, started_at, currency) VALUES(?1,?2,?3,'running',?4,?5,?6,?7)",
        params![run_id, provisional.instruction, serde_json::to_string(&provisional)?, trigger, monitor_id, now_iso(), s.display_currency],
    )?;
    let handle = state.runs.create(&run_id);
    let (tx, rx) = tokio::sync::oneshot::channel();
    let st = state.clone();
    let h = handle.clone();
    let p0 = provisional.clone();
    let trigger_s = trigger.to_string();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let fut = execute(&st, app.as_ref(), &h, p0, needs_ai, trigger_s);
        let res = tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(15 * 60), fut) => r.unwrap_or_else(|_| Err(AppError::internal("搜索超过 15 分钟，已自动结束"))),
            _ = h.cancel.cancelled() => Err(AppError::cancelled()),
        };
        let outcome = match res {
            Ok(o) => o,
            Err(e) => {
                let cancelled = e.code == "CANCELLED";
                let status = if cancelled { "cancelled" } else { "error" };
                let _ = st.db.conn().execute(
                    "UPDATE runs SET status=?2, finished_at=?3, error=?4 WHERE id=?1",
                    params![h.id, status, now_iso(), if cancelled { None } else { Some(e.message.clone()) }],
                );
                if cancelled {
                    h.emit(app.as_ref(), RunEventBody::Cancelled);
                } else {
                    h.emit(app.as_ref(), RunEventBody::Error { message: e.message.clone() });
                }
                RunOutcome { status: status.into(), error: Some(e.message), ..Default::default() }
            }
        };
        log::info!("run {} finished: {} in {:?}", h.id, outcome.status, started.elapsed());
        let _ = tx.send(outcome);
    });
    Ok((handle, provisional, rx))
}

fn platform_state(h: &RunHandle, app: Option<&AppHandle>, id: &str, state: &str, count: Option<u32>, message: Option<String>, ms: Option<u64>) {
    h.emit(app, RunEventBody::Platform { platform_id: id.into(), state: state.into(), count, message, ms });
}

/// 平台判定结果。
#[derive(Debug, Clone, Default)]
pub struct PlatformChoice {
    pub chosen: Vec<String>,
    pub skipped: Vec<PlatformSkip>,
    pub reason: String,
}

/// 平台判定（v26.03）：用户指定的平台全部保留（未登录的已在发起时拦截）；AI / 默认选择的平台里，
/// 跳过未启用的与"必须登录但没登录"的；找二手时补充已启用的二手平台。
pub fn choose_platforms(plan: &SearchPlan, all: &[Platform], status: &dyn Fn(&str) -> String) -> PlatformChoice {
    let mut out = PlatformChoice::default();
    let explicit = plan.platforms_explicit;
    let name = |p: &Platform| if all.iter().filter(|o| o.name == p.name).count() > 1 { format!("{} {}", p.name, p.edition) } else { p.name.clone() };
    for id in plan.platforms.iter() {
        if out.chosen.contains(id) {
            continue;
        }
        let Some(p) = all.iter().find(|p| &p.id == id) else { continue };
        if !p.enabled && !explicit {
            out.skipped.push(PlatformSkip { platform_id: id.clone(), reason: "disabled".into(), message: format!("{} 在平台目录中未启用", name(p)) });
            continue;
        }
        if p.login == "required" && !explicit && status(id) != "logged_in" {
            out.skipped.push(PlatformSkip { platform_id: id.clone(), reason: "login_required".into(), message: format!("{} 需要登录，本次已跳过（登录后可重新搜索）", name(p)) });
            continue;
        }
        out.chosen.push(id.clone());
    }
    // 找二手：补充已启用、可直接使用的二手平台
    if !explicit && plan.condition == "used" && !out.chosen.iter().any(|id| all.iter().any(|p| &p.id == id && p.kind == "c2c")) {
        let region_of_chosen: Vec<&str> = out.chosen.iter().filter_map(|id| all.iter().find(|p| &p.id == id)).map(|p| p.region.as_str()).collect();
        for p in all.iter().filter(|p| p.enabled && p.kind == "c2c") {
            let region_ok = region_of_chosen.is_empty() || region_of_chosen.contains(&p.region.as_str());
            let login_ok = p.login != "required" || status(&p.id) == "logged_in";
            if region_ok && login_ok && out.chosen.len() < 12 {
                out.chosen.push(p.id.clone());
            }
        }
    }
    let names: Vec<String> = out.chosen.iter().filter_map(|id| all.iter().find(|p| &p.id == id)).map(name).collect();
    let mut reason = if explicit {
        format!("按你指定的平台搜索：{}", names.join("、"))
    } else {
        let what = plan.product.category.trim();
        let what = if what.is_empty() { plan.keywords.first().map(|s| s.as_str()).unwrap_or("这件商品") } else { what };
        format!("按「{}」与地区选择 {} 个平台：{}", what.chars().take(16).collect::<String>(), names.len(), names.join("、"))
    };
    let login_skips: Vec<&PlatformSkip> = out.skipped.iter().filter(|s| s.reason == "login_required").collect();
    if !login_skips.is_empty() {
        reason.push_str(&format!("；{} 个平台需要先登录，已跳过", login_skips.len()));
    }
    if risk::wants_authentic(plan) {
        reason.push_str("；你强调正品，结果会优先平台自营 / 旗舰店 / 官方店");
    }
    out.reason = reason;
    out
}

/// 管线中的一条结果。
struct Work {
    c: Candidate,
    m: MatchInfo,
    it: ResultItem,
    rule_factors: Vec<RiskFactor>,
    ai_adjust: i32,
    review_raw: Option<reviews::ReviewRaw>,
}

fn emit_results(h: &RunHandle, app: Option<&AppHandle>, works: &[Work]) {
    h.emit(app, RunEventBody::Results { items: works.iter().map(|w| w.it.clone()).collect() });
}

fn funnel(h: &RunHandle, app: Option<&AppHandle>, stage: &str, label: &str, input: u32, kept: u32, note: Option<String>) {
    h.emit(app, RunEventBody::Funnel { stage: stage.into(), label: label.into(), input, kept, note });
}

fn shop_input<'a>(c: &'a Candidate, raw: Option<&'a reviews::ReviewRaw>, badges: &'a [String]) -> ShopInput<'a> {
    ShopInput {
        shop: c.shop.as_deref().or_else(|| raw.and_then(|r| r.shop_name.as_deref())),
        badges,
        title: &c.title,
        condition: &c.condition,
        seller_text: raw.and_then(|r| r.seller_text.as_deref()),
        rating: None,
        scores: raw.map(|r| &r.shop_scores).filter(|s| !s.is_empty()),
        years: raw.and_then(|r| r.shop_years),
        followers: raw.and_then(|r| r.shop_followers),
        sales: c.sales,
    }
}

fn refresh_tags(w: &mut Work) {
    w.it.tags = risk::tags(
        w.it.shop_info.as_ref(),
        w.it.shipping,
        w.it.coupon_price.is_some(),
        w.it.deal.as_ref(),
        w.it.sales,
        w.it.review_insight.as_ref(),
        w.it.rating,
        &w.it.condition,
        w.it.risk.as_ref(),
    );
}

async fn execute(st: &AppState, app: Option<&AppHandle>, h: &Arc<RunHandle>, mut plan: SearchPlan, needs_ai: bool, trigger: String) -> AppResult<RunOutcome> {
    let t0 = Instant::now();
    let s = settings::load(&st.db);
    let all = platforms::all(&st.db);
    let mut ai_calls = 0u32;
    let display = s.display_currency.clone();

    /* ---------------- ① 理解需求 ---------------- */
    stage(h, app, "plan", "AI 理解需求", 0.03);
    if needs_ai {
        let ctx = PlanCtx { platforms: &all, settings: &s };
        plan = planner::plan(&st.ai, &plan.instruction, &ctx).await?;
        ai_calls += 1;
    }
    h.emit(app, RunEventBody::Plan { plan: plan.clone() });

    /* ---------------- ② 平台判定 ---------------- */
    stage(h, app, "platforms", "平台判定", 0.08);
    let choice = choose_platforms(&plan, &all, &|id| st.logins.current(&st.db, id).status);
    h.emit(app, RunEventBody::PlatformPlan { chosen: choice.chosen.clone(), skipped: choice.skipped.clone(), reason: choice.reason.clone() });
    if choice.chosen.is_empty() {
        let login: Vec<&PlatformSkip> = choice.skipped.iter().filter(|s| s.reason == "login_required").collect();
        if !login.is_empty() {
            let names: Vec<String> = login.iter().filter_map(|s| all.iter().find(|p| p.id == s.platform_id)).map(|p| p.name.clone()).collect();
            return Err(AppError::login_required(&names, login.iter().map(|s| s.platform_id.clone()).collect()));
        }
        return Err(AppError::invalid("没有可用的平台：请在「平台 · 平台目录」启用至少一个平台"));
    }
    if choice.chosen != plan.platforms {
        plan.platforms = choice.chosen.clone();
        let ctx = PlanCtx { platforms: &all, settings: &s };
        let explicit = plan.platforms_explicit;
        plan = planner::normalize(plan, &ctx, None);
        plan.platforms_explicit = explicit;
        h.emit(app, RunEventBody::Plan { plan: plan.clone() });
    }
    st.db.conn().execute("UPDATE runs SET plan=?2 WHERE id=?1", params![h.id, serde_json::to_string(&plan)?])?;

    /* ---------------- ③ 多平台搜索 ---------------- */
    stage(h, app, "search", "多平台搜索", 0.12);
    let selected: Vec<Platform> = plan.platforms.iter().filter_map(|id| all.iter().find(|p| &p.id == id).cloned()).collect();
    for p in selected.iter() {
        platform_state(h, app, &p.id, "queued", None, None, None);
    }
    let total = selected.len().max(1) as f64;
    let done_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let wait_secs = s.search.login_wait_secs as u64;
    // "不相关"反馈：同一需求下用户标记过的商品
    let scope = crate::deal::scope_of(&plan);
    let irrelevant = crate::deal::irrelevant_set(&st.db, &scope);
    let (plan_ref, irr_ref, disp_ref) = (&plan, &irrelevant, &display);
    let futs = selected.iter().map(|p| {
        let queries: Vec<String> = plan.queries.iter().filter(|q| q.platform_id == p.id).map(|q| q.q.clone()).collect();
        let done_count = done_count.clone();
        let st = st.clone();
        let h = h.clone();
        let sort = plan.sort.clone();
        let popup = s.search.popup_on_login;
        let target = s.search.per_platform as usize;
        let timeout = Duration::from_secs(s.search.platform_timeout_secs as u64);
        async move {
            let started = Instant::now();
            let meta_status = st.logins.current(&st.db, &p.id).status;
            // 已登录但过期 / 遇到验证：允许弹窗等待用户处理；未登录：只有开启"搜索时允许弹窗"才等
            let wait_user = if meta_status == "logged_in" || popup { wait_secs } else { 0 };
            let notify = |state: &str, msg: &str| {
                platform_state(&h, app, &p.id, state, None, Some(msg.to_string()), None);
            };
            let opts = SearchOpts { target, max_rounds: 6, wait_user_secs: wait_user };
            let effective_timeout = timeout + Duration::from_secs(wait_user);
            let outcome = match tokio::time::timeout(effective_timeout, extract::search_platform(&st.browser, p, &queries, &sort, opts, &notify, &h.cancel)).await {
                Ok(o) => o,
                Err(_) => Outcome::Error(format!("{} 超过 {} 秒未完成，已跳过", p.name, effective_timeout.as_secs())),
            };
            let ms = started.elapsed().as_millis() as u64;
            let n = done_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let (state, cands, msg) = match outcome {
                Outcome::Ok(c) => {
                    if p.login != "optional" || meta_status == "logged_in" {
                        login::mark_verified(&st.db, &p.id);
                    }
                    ("done", c, None)
                }
                Outcome::LoginRequired(m) => {
                    login::mark_wall(&st.db, &p.id);
                    if let Some(app) = app {
                        let _ = tauri::Emitter::emit(app, "login://state", st.logins.current(&st.db, &p.id));
                    }
                    ("login_required", vec![], Some(m))
                }
                Outcome::Error(e) => ("error", vec![], Some(e)),
                Outcome::Cancelled => ("skipped", vec![], Some("已取消".to_string())),
            };
            platform_state(&h, app, &p.id, state, Some(cands.len() as u32), msg.clone(), Some(ms));
            // 渐进式结果：这个平台的结果先按规则初判显示，后续阶段完成后整体替换
            if !cands.is_empty() {
                let partial = provisional_items(&cands, plan_ref, disp_ref, &st.fx, irr_ref, p);
                if !partial.is_empty() {
                    h.emit(app, RunEventBody::Partial { platform_id: p.id.clone(), items: partial });
                }
            }
            stage(&h, app, "search", "多平台搜索", 0.12 + 0.38 * (n as f64 / total));
            (p.clone(), cands, PlatformStat { platform_id: p.id.clone(), state: state.into(), count: 0, ms, message: msg })
        }
    });
    let results = join_all(futs).await;
    if h.cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    let mut stats: Vec<PlatformStat> = vec![];
    let mut per_platform: Vec<(Platform, Vec<Candidate>)> = vec![];
    for (p, c, mut stat) in results {
        stat.count = c.len() as u32;
        stats.push(stat);
        per_platform.push((p, c));
    }
    let fetched: u32 = per_platform.iter().map(|(_, c)| c.len() as u32).sum();
    if fetched == 0 {
        let reasons: Vec<String> = stats.iter().filter_map(|s| s.message.clone()).take(3).collect();
        let hint = if stats.iter().any(|s| s.state == "login_required") { "部分平台需要先登录（平台 · 账号与登录）。" } else { "" };
        return Err(AppError::browser(format!("所有平台都没有返回结果。{hint}{}", reasons.join("；"))));
    }
    let ok_platforms = stats.iter().filter(|s| s.state == "done").count();
    funnel(h, app, "search", "多平台搜索", fetched, fetched, Some(format!("{ok_platforms} / {} 个平台返回结果", stats.len())));

    /* ---------------- ④ 粗筛：需求匹配度 ---------------- */
    stage(h, app, "screen", "粗筛：需求匹配度", 0.52);
    let extract_futs = per_platform.iter_mut().map(|(p, c)| {
        let ai = st.ai.clone();
        let label = format!("{} {}", p.name, p.edition);
        async move { verify::extract(&ai, &label, c).await }
    });
    ai_calls += join_all(extract_futs).await.into_iter().sum::<u32>();
    let mut all_cands: Vec<Candidate> = per_platform.into_iter().flat_map(|(_, c)| c).collect();
    for c in all_cands.iter_mut() {
        c.title = c.title.trim().to_string();
    }
    let out = filter::filter(all_cands, &plan, &st.fx);
    let kept_n = out.kept.len() as u32;
    h.emit(app, RunEventBody::Filter { fetched, kept: kept_n, dropped: out.dropped.clone(), duplicates: out.duplicates });
    if !out.relaxed.is_empty() {
        let names: Vec<&str> = out.relaxed.iter().map(|r| match r.as_str() { "price" => "价格", "condition" => "成色", _ => "排除词" }).collect();
        h.emit(app, RunEventBody::Log { level: "warn".into(), message: format!("按全部条件没有结果，已放宽：{}", names.join("、")) });
    }
    let mut cands = out.kept;
    // 控制成本：最多校验 120 条（按平台轮转挑选，保证每个平台都有代表）
    if cands.len() > 120 {
        let mut by_p: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
        for c in cands.drain(..) {
            by_p.entry(c.platform_id.clone()).or_default().push(c);
        }
        let mut round = 0;
        while cands.len() < 120 && by_p.values().any(|v| v.len() > round) {
            for v in by_p.values() {
                if let Some(c) = v.get(round) {
                    if cands.len() < 120 {
                        cands.push(c.clone());
                    }
                }
            }
            round += 1;
        }
    }
    let (verdicts, calls) = verify::verify(&st.ai, &st.db, &st.fx, &plan, &cands).await;
    ai_calls += calls;
    let mut matched = 0u32;
    let mut hidden = 0u32;
    let mut scored: Vec<(Candidate, MatchInfo, String)> = cands
        .into_iter()
        .zip(verdicts)
        .map(|(c, v)| {
            let (m, key) = match v {
                Some(v) => (MatchInfo { score: v.score, verdict: v.verdict, reasons: v.reasons, flags: v.flags, ai_checked: true }, v.product_key),
                None => (rule_match(&c, &plan), String::new()),
            };
            (c, m, key)
        })
        .collect();
    for (c, m, _) in scored.iter_mut() {
        if c.is_ad && !m.flags.iter().any(|f| f == "ad") {
            m.flags.push("ad".into());
        }
        if irrelevant.contains(&(c.platform_id.clone(), c.external_id.clone())) {
            m.verdict = "mismatch".into();
            m.score = m.score.min(10);
            m.reasons = vec!["你之前标记为不相关".into()];
        }
        if m.verdict == "mismatch" {
            hidden += 1;
        } else if m.verdict == "exact" || m.verdict == "close" {
            matched += 1;
        }
    }
    h.emit(app, RunEventBody::Verify { checked: scored.len() as u32, matched, hidden });
    // 粗筛：匹配度优先，保留进入后续分析的候选（≥ 最终条数，至少 24 条）
    scored.sort_by(|a, b| verdict_rank(&a.1.verdict).cmp(&verdict_rank(&b.1.verdict)).then(b.1.score.cmp(&a.1.score)));
    let keep_n = (plan.limit as usize).max(24);
    let mut kept: Vec<(Candidate, MatchInfo, String)> = vec![];
    let mut tail: Vec<(Candidate, MatchInfo, String)> = vec![];
    for x in scored.into_iter() {
        if x.1.verdict == "mismatch" {
            if tail.len() < 15 {
                tail.push(x);
            }
        } else if kept.len() < keep_n {
            kept.push(x);
        }
    }
    let n_ok = kept.len() as u32;
    kept.extend(tail);
    let cl = clusters(&kept.iter().map(|(c, _, k)| (c.platform_id.clone(), k.clone(), c.title.clone())).collect::<Vec<_>>());

    // 入库，得到稳定的 listing id（后续阶段原位更新）
    let run_started: String = st.db.conn().query_row("SELECT started_at FROM runs WHERE id=?1", [&h.id], |r| r.get(0)).unwrap_or_else(|_| now_iso());
    let mut works: Vec<Work> = vec![];
    let mut new_ids: Vec<String> = vec![];
    {
        let conn = st.db.conn();
        let now = now_iso();
        for (rank, ((c, m, _key), cluster)) in kept.into_iter().zip(cl).enumerate() {
            let id = upsert_listing(&conn, &c, &now)?;
            let (first_seen, watched): (String, bool) = conn.query_row(
                "SELECT l.first_seen_at, EXISTS(SELECT 1 FROM watches w WHERE w.listing_id=l.id) FROM listings l WHERE l.id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )?;
            if first_seen >= run_started {
                new_ids.push(id.clone());
            }
            let mut it = base_item(&c, id.clone(), m.clone(), &display, &st.fx, &now);
            it.provisional = false;
            it.cluster_id = cluster;
            it.watched = watched;
            it.rank = rank as u32 + 1;
            it.first_seen_at = first_seen;
            it.spark = spark(&conn, &id);
            it.location = c.location.clone();
            it.deal = c.price.and_then(|p| crate::deal::for_listing(&conn, &id, p, &c.currency));
            if irrelevant.contains(&(c.platform_id.clone(), c.external_id.clone())) {
                it.feedback = Some("irrelevant".into());
            }
            works.push(Work { c, m, it, rule_factors: vec![], ai_adjust: 0, review_raw: None });
        }
    }
    set_also_on(&mut works);
    let dropped_n: u32 = out.dropped.values().sum();
    funnel(h, app, "screen", "粗筛：需求匹配度", fetched, n_ok, Some(format!("去重 {} · 条件过滤 {} · AI 判定不相关 {}", out.duplicates, dropped_n, hidden)));
    emit_results(h, app, &works);

    /* ---------------- ⑤ 商品风险分析 ---------------- */
    stage(h, app, "risk", "商品风险分析", 0.6);
    let landed = |it: &ResultItem| it.display_landed.or(it.display_price);
    let ref_price = risk::median(
        works.iter().filter(|w| (w.m.verdict == "exact" || w.m.verdict == "close") && w.c.condition != "used").filter_map(|w| landed(&w.it)).collect(),
    );
    for w in works.iter_mut() {
        w.rule_factors = risk::rule_factors(&w.c, &w.m, &plan, landed(&w.it), ref_price);
    }
    let ai_input: Vec<serde_json::Value> = works
        .iter()
        .enumerate()
        .filter(|(_, w)| w.m.verdict != "mismatch")
        .take(40)
        .map(|(i, w)| {
            serde_json::json!({
                "i": i, "platform": w.c.platform_id, "title": w.c.title, "price": w.c.price, "currency": w.c.currency,
                "displayLanded": landed(&w.it), "refPrice": ref_price, "shop": w.c.shop, "badges": w.c.badges, "sales": w.c.sales,
                "rating": w.c.rating, "reviews": w.c.reviews, "condition": w.c.condition, "verdict": w.m.verdict, "flags": w.m.flags,
                "ruleFactors": w.rule_factors.iter().map(|f| f.label.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let (risk_ai, calls) = analysis::risk(&st.ai, &plan, ai_input).await;
    ai_calls += calls;
    if h.cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    for (i, w) in works.iter_mut().enumerate() {
        let ai = risk_ai.as_ref().and_then(|m| m.get(&i));
        let mut factors = w.rule_factors.clone();
        if let Some(a) = ai {
            for f in a.factors.iter() {
                if !factors.iter().any(|x| x.label == f.label) {
                    factors.push(f.clone());
                }
            }
            w.ai_adjust = a.adjust;
        }
        w.it.risk = Some(risk::compose(factors, w.ai_adjust, ai.map(|a| a.summary.clone()), ai.is_some()));
        refresh_tags(w);
    }
    let level_count = |works: &[Work], lv: &str| works.iter().filter(|w| w.m.verdict != "mismatch" && w.it.risk.as_ref().is_some_and(|r| r.level == lv)).count() as u32;
    let (high, medium) = (level_count(&works, "high"), level_count(&works, "medium"));
    let ai_note = if risk_ai.is_some() { "" } else { "（AI 暂不可用，按规则判断）" };
    funnel(h, app, "risk", "商品风险分析", n_ok, n_ok - high, Some(format!("高风险 {high} 件 · 需留意 {medium} 件{ai_note}")));
    emit_results(h, app, &works);

    /* ---------------- ⑥ 店铺可信度 ---------------- */
    stage(h, app, "shop", "店铺可信度", 0.66);
    let authentic = risk::wants_authentic(&plan);
    {
        let conn = st.db.conn();
        for w in works.iter_mut() {
            let Some(p) = all.iter().find(|p| p.id == w.c.platform_id) else { continue };
            let shop = shop::classify(p, &shop_input(&w.c, None, &w.c.badges));
            if let (Some(r), Some(f)) = (w.it.risk.as_ref(), risk::shop_factor(&shop, authentic)) {
                w.it.risk = Some(risk::add_factors(r, vec![f], w.ai_adjust));
            }
            let _ = conn.execute(
                "UPDATE listings SET shop_json=?2, badges=?3 WHERE id=?1",
                params![w.it.id, serde_json::to_string(&shop).unwrap_or_default(), serde_json::to_string(&w.c.badges).unwrap_or_default()],
            );
            w.it.shop_info = Some(shop);
            refresh_tags(w);
        }
    }
    let trusted = works.iter().filter(|w| w.m.verdict != "mismatch" && w.it.shop_info.as_ref().is_some_and(|s| ["self_operated", "flagship", "official"].contains(&s.kind.as_str()))).count();
    let individual = works.iter().filter(|w| w.m.verdict != "mismatch" && w.it.shop_info.as_ref().is_some_and(|s| s.kind == "individual")).count();
    funnel(h, app, "shop", "店铺可信度", n_ok, n_ok, Some(format!("自营 / 旗舰 / 官方 {trusted} 件 · 个人卖家 {individual} 件")));
    emit_results(h, app, &works);

    /* ---------------- ⑦ 评论检索 ---------------- */
    let mut top_n = match s.search.depth.as_str() {
        "quick" => 0,
        "deep" => (s.search.review_top as usize * 2).clamp(1, 24),
        _ => (s.search.review_top as usize).clamp(1, 12),
    };
    if trigger == "monitor" {
        top_n = top_n.min(3);
    }
    let mut read_ok: Vec<usize> = vec![];
    if top_n == 0 {
        stage(h, app, "reviews", "评论检索（快速模式，已跳过）", 0.72);
        funnel(h, app, "reviews", "评论检索", 0, 0, Some("快速模式不读取评论；可在设置中改为「标准」或「深入」".into()));
        stage(h, app, "review_ai", "评论分析与匹配（已跳过）", 0.86);
    } else {
        stage(h, app, "reviews", "评论检索", 0.72);
        let mut order: Vec<usize> = (0..works.len())
            .filter(|&i| {
                let w = &works[i];
                w.m.verdict != "mismatch" && !w.it.url.is_empty() && w.it.risk.as_ref().is_none_or(|r| r.level != "high") && w.it.feedback.is_none()
            })
            .collect();
        let prelim = |w: &Work| {
            w.m.score as f64 * 0.5 + (100.0 - w.it.risk.as_ref().map(|r| r.score).unwrap_or(30) as f64) * 0.25 + w.it.shop_info.as_ref().map(|s| s.credibility).unwrap_or(50) as f64 * 0.25
        };
        order.sort_by(|&a, &b| prelim(&works[b]).total_cmp(&prelim(&works[a])));
        order.truncate(top_n);
        let mut jobs: Vec<(String, Platform, String)> = vec![];
        let mut cached_n = 0u32;
        for &i in order.iter() {
            let w = &mut works[i];
            match reviews::cached(&st.db, &w.it.id, 24) {
                Some(raw) => {
                    w.review_raw = Some(raw);
                    cached_n += 1;
                }
                None => {
                    if let Some(p) = all.iter().find(|p| p.id == w.c.platform_id) {
                        jobs.push((w.it.id.clone(), p.clone(), w.it.url.clone()));
                    }
                }
            }
        }
        let budget = Duration::from_secs(match (s.search.depth.as_str(), trigger.as_str()) {
            (_, "monitor") => 60,
            ("deep", _) => 170,
            _ => 100,
        });
        let progress = |n: usize, t: usize| {
            stage(h, app, "reviews", &format!("评论检索 {n}/{t}"), 0.72 + 0.12 * (n as f64 / t.max(1) as f64));
        };
        let fetched_reviews = reviews::fetch_many(&st.browser, jobs, Duration::from_secs(28), budget, (s.search.concurrency as usize).clamp(1, 3), &h.cancel, &progress).await;
        if h.cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        let mut fail_reason: HashMap<String, String> = HashMap::new();
        for (id, r) in fetched_reviews {
            match r {
                Ok(raw) => {
                    if let Some(w) = works.iter_mut().find(|w| w.it.id == id) {
                        w.review_raw = Some(raw);
                    }
                }
                Err(e) => {
                    fail_reason.insert(id, e);
                }
            }
        }
        for &i in order.iter() {
            let w = &mut works[i];
            match w.review_raw.clone() {
                Some(raw) => {
                    let ins = reviews::rule_insight(&raw);
                    if raw.has_product() {
                        read_ok.push(i);
                    }
                    // 详情页的店铺信息补充店铺分类与可信度
                    if raw.has_shop() || raw.seller_text.is_some() {
                        if let Some(p) = all.iter().find(|p| p.id == w.c.platform_id) {
                            let mut badges = w.c.badges.clone();
                            badges.extend(raw.shop_badges.iter().cloned());
                            let mut shop = shop::classify(p, &shop_input(&w.c, Some(&raw), &badges));
                            if shop.kind == "unknown" {
                                if let Some(old) = w.it.shop_info.as_ref() {
                                    shop.kind = old.kind.clone();
                                    shop.label = old.label.clone();
                                }
                            }
                            if let Some(r) = w.it.risk.as_ref() {
                                let mut factors: Vec<RiskFactor> = r.factors.iter().filter(|f| f.kind != "shop").cloned().collect();
                                factors.extend(risk::shop_factor(&shop, authentic));
                                w.it.risk = Some(risk::compose(factors, w.ai_adjust, r.ai_checked.then(|| r.summary.clone()), r.ai_checked));
                            }
                            let _ = st.db.conn().execute("UPDATE listings SET shop_json=?2 WHERE id=?1", params![w.it.id, serde_json::to_string(&shop).unwrap_or_default()]);
                            w.it.shop_info = Some(shop);
                        }
                    }
                    reviews::store(&st.db, &w.it.id, &raw, &ins);
                    w.it.review_insight = Some(ins);
                }
                None => {
                    let reason = fail_reason.get(&w.it.id).cloned().unwrap_or_else(|| "未读取".into());
                    w.it.review_insight = Some(ReviewInsight { source: "none".into(), unavailable_reason: Some(reason), ..Default::default() });
                }
            }
            refresh_tags(w);
        }
        let mut note = format!("读取 {} 件商品的评论", read_ok.len());
        if cached_n > 0 {
            note.push_str(&format!("（{cached_n} 件来自 24 小时内的记录）"));
        }
        let unavailable = order.len() - read_ok.len();
        if unavailable > 0 {
            let why = order
                .iter()
                .filter_map(|&i| works[i].it.review_insight.as_ref().and_then(|r| r.unavailable_reason.clone()))
                .next()
                .unwrap_or_default();
            note.push_str(&format!(" · {unavailable} 件未读到（{why}）"));
        }
        funnel(h, app, "reviews", "评论检索", order.len() as u32, read_ok.len() as u32, Some(note));
        emit_results(h, app, &works);

        /* ---------------- ⑧ 评论分析与匹配 ---------------- */
        stage(h, app, "review_ai", "评论分析与匹配", 0.86);
        let ai_input: Vec<serde_json::Value> = order
            .iter()
            .filter_map(|&i| {
                let w = &works[i];
                let raw = w.review_raw.as_ref()?;
                (raw.has_product() || raw.has_shop()).then(|| {
                    serde_json::json!({
                        "i": i, "platform": w.c.platform_id, "title": w.c.title, "rating": raw.rating, "count": raw.count, "goodRate": raw.good_rate,
                        "reviews": raw.texts.iter().take(12).map(|t| t.chars().take(160).collect::<String>()).collect::<Vec<_>>(),
                        "shop": w.it.shop_info.as_ref().map(|s| serde_json::json!({"name": s.name, "type": s.label, "scores": s.scores, "years": s.years})),
                    })
                })
            })
            .collect();
        let analyzed = ai_input.len() as u32;
        let (rev_ai, calls) = analysis::reviews(&st.ai, &plan, ai_input).await;
        ai_calls += calls;
        if h.cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        let mut flagged = 0u32;
        for &i in order.iter() {
            let w = &mut works[i];
            let Some(ins) = w.it.review_insight.as_mut() else { continue };
            if let Some(a) = rev_ai.as_ref().and_then(|m| m.get(&i)) {
                if a.score.is_some() {
                    ins.score = a.score;
                }
                ins.pros = a.pros.clone();
                ins.cons = a.cons.clone();
                if !a.red_flags.is_empty() || ins.ai_checked {
                    ins.red_flags = a.red_flags.clone();
                }
                ins.match_note = a.match_note.clone();
                if a.shop_note.is_some() {
                    ins.shop_note = a.shop_note.clone();
                }
                ins.ai_checked = true;
                if let Some(raw) = w.review_raw.as_ref() {
                    reviews::store(&st.db, &w.it.id, raw, ins);
                }
            }
            if !ins.red_flags.is_empty() {
                flagged += 1;
            }
            let extra = risk::review_factors(ins);
            if let Some(r) = w.it.risk.as_ref() {
                w.it.risk = Some(risk::add_factors(r, extra, w.ai_adjust));
            }
            refresh_tags(w);
        }
        let ai_note = if rev_ai.is_some() || analyzed == 0 { "" } else { "（AI 暂不可用，按评分与关键词判断）" };
        funnel(h, app, "review_ai", "评论分析与匹配", analyzed, analyzed - flagged.min(analyzed), Some(format!("{flagged} 件发现评论风险信号{ai_note}")));
        emit_results(h, app, &works);
    }

    /* ---------------- ⑨ 最终细筛 ---------------- */
    stage(h, app, "final", "最终细筛", 0.9);
    for w in works.iter_mut() {
        let ps = risk::price_score(w.it.deal.as_ref(), landed(&w.it), ref_price);
        w.it.final_score = Some(risk::final_score(&w.m, w.it.risk.as_ref(), w.it.shop_info.as_ref(), w.it.review_insight.as_ref(), ps));
        refresh_tags(w);
    }
    let hide_high = s.search.hide_high_risk;
    let class = |w: &Work| -> u8 {
        if w.m.verdict == "mismatch" {
            2
        } else if hide_high && w.it.risk.as_ref().is_some_and(|r| r.level == "high") {
            1
        } else {
            0
        }
    };
    let price_cmp = |x: Option<f64>, y: Option<f64>, asc: bool| match (x, y) {
        (Some(x), Some(y)) => if asc { x.total_cmp(&y) } else { y.total_cmp(&x) },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    };
    works.sort_by(|a, b| {
        class(a).cmp(&class(b)).then_with(|| match plan.sort.as_str() {
            "price_asc" => verdict_rank(&a.m.verdict).min(2).cmp(&verdict_rank(&b.m.verdict).min(2)).then(price_cmp(landed(&a.it), landed(&b.it), true)),
            "price_desc" => verdict_rank(&a.m.verdict).min(2).cmp(&verdict_rank(&b.m.verdict).min(2)).then(price_cmp(landed(&a.it), landed(&b.it), false)),
            "sales" => b.it.sales.unwrap_or(0).cmp(&a.it.sales.unwrap_or(0)),
            "rating" => b.it.rating.unwrap_or(0.0).total_cmp(&a.it.rating.unwrap_or(0.0)),
            _ => b.it.final_score.unwrap_or(0).cmp(&a.it.final_score.unwrap_or(0)).then(b.m.score.cmp(&a.m.score)),
        })
    });
    let limit = plan.limit as usize;
    let (mut n0, mut n1) = (0usize, 0usize);
    works.retain(|w| match class(w) {
        0 => {
            n0 += 1;
            n0 <= limit
        }
        1 => {
            n1 += 1;
            n1 <= 10
        }
        _ => true,
    });
    for (i, w) in works.iter_mut().enumerate() {
        w.it.rank = i as u32 + 1;
    }
    set_also_on(&mut works);
    {
        let conn = st.db.conn();
        for w in works.iter() {
            let it = &w.it;
            conn.execute(
                "INSERT OR REPLACE INTO run_results(run_id, listing_id, rank, score, verdict, reasons, flags, cluster, ai_checked, also_on, display_price, risk_json, tags_json, final_score)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![h.id, it.id, it.rank, it.match_info.score, it.match_info.verdict, serde_json::to_string(&it.match_info.reasons)?,
                        serde_json::to_string(&it.match_info.flags)?, it.cluster_id, it.match_info.ai_checked as i64,
                        serde_json::to_string(&it.also_on)?, it.display_price, it.risk.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()),
                        serde_json::to_string(&it.tags)?, it.final_score],
            )?;
        }
    }
    let final_ok = works.iter().filter(|w| class(w) == 0).count() as u32;
    let folded = works.iter().filter(|w| class(w) == 1).count();
    let mut note = "按综合分排序：匹配 40% · 风险 20% · 店铺 15% · 评论 15% · 价格 10%".to_string();
    if folded > 0 {
        note.push_str(&format!("；{folded} 件高风险已折叠"));
    }
    funnel(h, app, "final", "最终细筛", n_ok, final_ok, Some(note));
    let items: Vec<ResultItem> = works.iter().map(|w| w.it.clone()).collect();
    h.emit(app, RunEventBody::Results { items: items.clone() });

    /* ---------------- ⑩ 总结 ---------------- */
    let best_price = items
        .iter()
        .filter(|r| (r.match_info.verdict == "exact" || r.match_info.verdict == "close") && r.risk.as_ref().is_none_or(|x| x.level != "high"))
        .filter_map(|r| r.display_landed.or(r.display_price))
        .fold(None, |a: Option<f64>, p| Some(a.map_or(p, |x| x.min(p))));
    stage(h, app, "summarize", "AI 总结与推荐", 0.94);
    let top = items.iter().find(|r| r.match_info.verdict != "mismatch" && r.risk.as_ref().is_none_or(|x| x.level != "high"));
    let summary = if items.iter().any(|r| r.match_info.verdict != "mismatch") {
        ai_calls += 1;
        match verify::summarize(&st.ai, &plan, &items, &display).await {
            Ok(mut sm) => {
                if sm.best_pick.is_none() {
                    sm.best_pick = top.map(|t| BestPick { id: t.id.clone(), why: best_why(t) });
                }
                h.emit(app, RunEventBody::Summary { summary: sm.clone() });
                Some(sm)
            }
            Err(e) => {
                h.emit(app, RunEventBody::Log { level: "warn".into(), message: format!("AI 总结暂时不可用：{}", e.message) });
                None
            }
        }
    } else {
        None
    };

    let run_stats = RunStats {
        fetched,
        kept: items.iter().filter(|r| r.match_info.verdict != "mismatch").count() as u32,
        hidden,
        duplicates: out.duplicates,
        dropped: out.dropped,
        elapsed_ms: t0.elapsed().as_millis() as u64,
        platforms: stats,
        ai_calls,
    };
    st.db.conn().execute(
        "UPDATE runs SET status='done', finished_at=?2, fetched=?3, kept=?4, summary=?5, stats=?6, best_price=?7, currency=?8 WHERE id=?1",
        params![h.id, now_iso(), run_stats.fetched, run_stats.kept, summary.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()),
                serde_json::to_string(&run_stats)?, best_price, display],
    )?;
    stage(h, app, "done", "完成", 1.0);
    h.emit(app, RunEventBody::Done { stats: run_stats });
    Ok(RunOutcome { status: "done".into(), results: items, new_ids, best_price, currency: display, error: None })
}

fn verdict_rank(v: &str) -> u8 {
    match v {
        "exact" => 0,
        "close" => 1,
        "related" => 2,
        _ => 3,
    }
}

fn best_why(r: &ResultItem) -> String {
    let mut parts = vec![format!("综合分 {}", r.final_score.unwrap_or(0))];
    if let Some(s) = r.shop_info.as_ref().filter(|s| s.kind != "unknown") {
        parts.push(s.label.clone());
    }
    if let Some(x) = r.risk.as_ref() {
        parts.push(format!("风险{}", match x.level.as_str() { "low" => "低", "medium" => "中", _ => "高" }));
    }
    if let Some(sc) = r.review_insight.as_ref().and_then(|v| v.score) {
        parts.push(format!("评论 {sc} 分"));
    }
    parts.join(" · ")
}

/// 同款"也在"：只在当前结果之间互相引用。
fn set_also_on(works: &mut [Work]) {
    let mut by_cluster: HashMap<String, Vec<AlsoOn>> = HashMap::new();
    for w in works.iter() {
        if let Some(cid) = &w.it.cluster_id {
            by_cluster.entry(cid.clone()).or_default().push(AlsoOn { listing_id: w.it.id.clone(), platform_id: w.it.platform_id.clone(), price: w.it.price, currency: w.it.currency.clone(), url: w.it.url.clone() });
        }
    }
    for w in works.iter_mut() {
        if let Some(cid) = &w.it.cluster_id {
            w.it.also_on = by_cluster.get(cid).map(|v| v.iter().filter(|a| a.listing_id != w.it.id).cloned().collect()).unwrap_or_default();
        }
    }
}

/// 写入 / 更新商品，并记录价格点（同日同价合并）；返回 listing id。
fn upsert_listing(conn: &rusqlite::Connection, c: &Candidate, now: &str) -> AppResult<String> {
    let id: String = conn.query_row(
        "INSERT INTO listings(id, platform_id, external_id, title, url, image, price, currency, original_price, sales, rating, reviews,
           shop, location, condition, first_seen_at, last_seen_at, shipping, coupon_price, badges)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16,?17,?18,?19)
         ON CONFLICT(platform_id, external_id) DO UPDATE SET title=excluded.title,
           url=CASE WHEN excluded.url<>'' THEN excluded.url ELSE listings.url END,
           image=COALESCE(excluded.image, listings.image), price=COALESCE(excluded.price, listings.price),
           currency=excluded.currency, original_price=excluded.original_price, sales=COALESCE(excluded.sales, listings.sales),
           rating=COALESCE(excluded.rating, listings.rating), reviews=COALESCE(excluded.reviews, listings.reviews),
           shop=COALESCE(excluded.shop, listings.shop), location=COALESCE(excluded.location, listings.location),
           condition=CASE WHEN excluded.condition<>'unknown' THEN excluded.condition ELSE listings.condition END,
           last_seen_at=excluded.last_seen_at, shipping=excluded.shipping, coupon_price=excluded.coupon_price, badges=excluded.badges
         RETURNING id",
        params![uuid::Uuid::new_v4().to_string(), c.platform_id, c.external_id, c.title, c.url, c.image, c.price, c.currency,
                c.original_price, c.sales, c.rating, c.reviews, c.shop, c.location, c.condition, now, c.shipping, c.coupon_price,
                serde_json::to_string(&c.badges)?],
        |r| r.get(0),
    )?;
    if let Some(price) = c.price {
        let last: Option<(f64, String, String)> = conn
            .query_row("SELECT price, currency, captured_at FROM price_points WHERE listing_id=?1 ORDER BY captured_at DESC LIMIT 1", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let same_day = last.as_ref().is_some_and(|(p, cur, t)| (*p - price).abs() < 0.005 && cur == &c.currency && t.get(..10) == now.get(..10));
        if !same_day {
            conn.execute("INSERT INTO price_points(listing_id, price, currency, source, captured_at) VALUES(?1,?2,?3,'search',?4)", params![id, price, c.currency, now])?;
        }
    }
    Ok(id)
}

/* ---------------- 结果条目 ---------------- */
/// 由候选构造结果条目（到手价、换算价）；排名、走势、价格判断由调用方补充。
pub fn base_item(c: &Candidate, id: String, m: MatchInfo, display: &str, fx: &crate::fx::Fx, now: &str) -> ResultItem {
    let landed = c.landed();
    ResultItem {
        id,
        platform_id: c.platform_id.clone(),
        external_id: c.external_id.clone(),
        title: c.title.clone(),
        url: c.url.clone(),
        image: c.image.clone(),
        price: c.price,
        currency: c.currency.clone(),
        display_price: c.price.and_then(|p| fx.convert(p, &c.currency, display)),
        original_price: c.original_price,
        shipping: c.shipping,
        coupon_price: c.coupon_price,
        landed_price: landed,
        display_landed: landed.and_then(|p| fx.convert(p, &c.currency, display)),
        deal: None,
        provisional: true,
        feedback: None,
        tags: vec![],
        shop_info: None,
        risk: None,
        review_insight: None,
        final_score: None,
        sales: c.sales,
        rating: c.rating,
        reviews: c.reviews,
        shop: c.shop.clone(),
        location: None,
        condition: c.condition.clone(),
        match_info: m,
        cluster_id: None,
        also_on: vec![],
        watched: false,
        rank: 0,
        first_seen_at: now.to_string(),
        last_seen_at: now.to_string(),
        spark: vec![],
    }
}

/// 渐进式结果：单个平台的候选经过过滤与规则初判后的条目（id 以 "p:" 开头，最终结果会替换它们）。
fn provisional_items(cands: &[Candidate], plan: &SearchPlan, display: &str, fx: &crate::fx::Fx, irrelevant: &std::collections::HashSet<(String, String)>, p: &Platform) -> Vec<ResultItem> {
    let now = now_iso();
    let out = filter::filter(cands.to_vec(), plan, fx);
    let mut items: Vec<ResultItem> = out
        .kept
        .iter()
        .filter(|c| !irrelevant.contains(&(c.platform_id.clone(), c.external_id.clone())))
        .map(|c| {
            let mut it = base_item(c, format!("p:{}:{}", c.platform_id, c.external_id), rule_match(c, plan), display, fx, &now);
            // 规则即可判断的店铺类型先显示（之后各阶段原位更新）
            let shop = shop::classify(p, &shop_input(c, None, &c.badges));
            it.tags = risk::tags(Some(&shop), it.shipping, it.coupon_price.is_some(), None, it.sales, None, it.rating, &it.condition, None);
            it.shop_info = Some(shop);
            it.location = c.location.clone();
            it
        })
        .collect();
    items.sort_by_key(|x| std::cmp::Reverse(x.match_info.score));
    items.truncate(40);
    items
}

/* ---------------- 历史与回放 ---------------- */
fn summary_row(r: &rusqlite::Row) -> rusqlite::Result<RunSummary> {
    let plan: SearchPlan = serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default();
    Ok(RunSummary {
        id: r.get(0)?,
        instruction: r.get(1)?,
        status: r.get(3)?,
        trigger: r.get(4)?,
        started_at: r.get(5)?,
        finished_at: r.get(6)?,
        fetched: r.get(7)?,
        kept: r.get(8)?,
        error: r.get(9)?,
        best_price: r.get(10)?,
        currency: r.get::<_, Option<String>>(11)?.unwrap_or_else(|| "CNY".into()),
        platforms: plan.platforms,
    })
}

const SUMMARY_COLS: &str = "id, instruction, plan, status, trigger, started_at, finished_at, fetched, kept, error, best_price, currency";

pub fn list_runs(db: &Db, limit: i64) -> AppResult<Vec<RunSummary>> {
    let c = db.conn();
    let mut stmt = c.prepare(&format!("SELECT {SUMMARY_COLS} FROM runs ORDER BY started_at DESC LIMIT ?1"))?;
    let rows = stmt.query_map([limit], summary_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn load_items(db: &Db, run_id: &str, display: &str, fx: &crate::fx::Fx) -> AppResult<Vec<ResultItem>> {
    let c = db.conn();
    let mut stmt = c.prepare(
        "SELECT l.id, l.platform_id, l.external_id, l.title, l.url, l.image, l.price, l.currency, l.original_price, l.sales, l.rating,
                l.reviews, l.shop, l.location, l.condition, l.first_seen_at, l.last_seen_at,
                rr.rank, rr.score, rr.verdict, rr.reasons, rr.flags, rr.cluster, rr.ai_checked, rr.also_on,
                EXISTS(SELECT 1 FROM watches w WHERE w.listing_id=l.id), l.shipping, l.coupon_price,
                l.shop_json, l.review_json, rr.risk_json, rr.tags_json, rr.final_score
         FROM run_results rr JOIN listings l ON l.id=rr.listing_id WHERE rr.run_id=?1 ORDER BY rr.rank",
    )?;
    let rows = stmt.query_map([run_id], |r| {
        let cand = Candidate {
            platform_id: r.get(1)?,
            external_id: r.get(2)?,
            title: r.get(3)?,
            url: r.get(4)?,
            image: r.get(5)?,
            price: r.get(6)?,
            currency: r.get(7)?,
            original_price: r.get(8)?,
            sales: r.get(9)?,
            rating: r.get(10)?,
            reviews: r.get(11)?,
            shop: r.get(12)?,
            condition: r.get(14)?,
            shipping: r.get(26)?,
            coupon_price: r.get(27)?,
            ..Default::default()
        };
        let m = MatchInfo {
            score: r.get(18)?,
            verdict: r.get(19)?,
            reasons: serde_json::from_str(&r.get::<_, String>(20)?).unwrap_or_default(),
            flags: serde_json::from_str(&r.get::<_, String>(21)?).unwrap_or_default(),
            ai_checked: r.get::<_, i64>(23)? != 0,
        };
        let mut it = base_item(&cand, r.get(0)?, m, display, fx, "");
        it.provisional = false;
        it.location = r.get(13)?;
        it.first_seen_at = r.get(15)?;
        it.last_seen_at = r.get(16)?;
        it.rank = r.get(17)?;
        it.cluster_id = r.get(22)?;
        it.also_on = serde_json::from_str(&r.get::<_, String>(24)?).unwrap_or_default();
        it.watched = r.get::<_, i64>(25)? != 0;
        apply_enrichment(&mut it, r.get(28)?, r.get(29)?, r.get(30)?, r.get(31)?, r.get(32)?);
        Ok(it)
    })?;
    let mut items: Vec<ResultItem> = rows.filter_map(|r| r.ok()).collect();
    let scope: Option<String> = c
        .query_row("SELECT plan FROM runs WHERE id=?1", [run_id], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|p| serde_json::from_str::<SearchPlan>(&p).ok())
        .map(|p| crate::deal::scope_of(&p));
    drop(stmt);
    drop(c);
    let irrelevant = scope.map(|sc| crate::deal::irrelevant_set(db, &sc)).unwrap_or_default();
    let c = db.conn();
    for it in items.iter_mut() {
        it.spark = spark(&c, &it.id);
        it.deal = it.price.and_then(|p| crate::deal::for_listing(&c, &it.id, p, &it.currency));
        if irrelevant.contains(&(it.platform_id.clone(), it.external_id.clone())) {
            it.feedback = Some("irrelevant".into());
        }
    }
    Ok(items)
}

/// v26.03：店铺 / 评论（listings）与风险 / 标签 / 综合分（run_results）；旧记录没有这些列值时保持为空。
fn apply_enrichment(it: &mut ResultItem, shop_json: Option<String>, review_json: Option<String>, risk_json: Option<String>, tags_json: Option<String>, final_score: Option<i64>) {
    it.shop_info = shop_json.and_then(|j| serde_json::from_str(&j).ok());
    it.review_insight = reviews::stored_insight(review_json);
    it.risk = risk_json.and_then(|j| serde_json::from_str(&j).ok());
    it.tags = tags_json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    it.final_score = final_score.map(|v| v.clamp(0, 100) as u32);
}

pub fn spark(c: &rusqlite::Connection, listing_id: &str) -> Vec<f64> {
    let Ok(mut stmt) = c.prepare("SELECT price FROM (SELECT price, captured_at FROM price_points WHERE listing_id=?1 ORDER BY captured_at DESC LIMIT 12) ORDER BY captured_at") else {
        return vec![];
    };
    stmt.query_map([listing_id], |r| r.get::<_, f64>(0)).map(|rows| rows.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}

pub fn run_detail(db: &Db, fx: &crate::fx::Fx, run_id: &str, display: &str) -> AppResult<RunDetail> {
    let (summary, plan, ai_summary, stats) = {
        let c = db.conn();
        c.query_row(&format!("SELECT {SUMMARY_COLS}, summary, stats FROM runs WHERE id=?1"), [run_id], |r| {
            let s = summary_row(r)?;
            let plan: SearchPlan = serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default();
            let ai: Option<AiSummary> = r.get::<_, Option<String>>(12)?.and_then(|x| serde_json::from_str(&x).ok());
            let st: Option<RunStats> = r.get::<_, Option<String>>(13)?.and_then(|x| serde_json::from_str(&x).ok());
            Ok((s, plan, ai, st))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("搜索记录"))?
    };
    let results = load_items(db, run_id, display, fx)?;
    Ok(RunDetail { summary, plan, results, ai_summary, stats })
}

pub fn delete_run(db: &Db, run_id: &str) -> AppResult<()> {
    let c = db.conn();
    c.execute("DELETE FROM run_results WHERE run_id=?1", [run_id])?;
    c.execute("DELETE FROM runs WHERE id=?1", [run_id])?;
    Ok(())
}

pub fn listing_detail(db: &Db, fx: &crate::fx::Fx, id: &str, display: &str) -> AppResult<ListingDetail> {
    let c = db.conn();
    let run: Option<String> = c
        .query_row("SELECT run_id FROM run_results rr JOIN runs r ON r.id=rr.run_id WHERE rr.listing_id=?1 ORDER BY r.started_at DESC LIMIT 1", [id], |r| r.get(0))
        .optional()?;
    drop(c);
    let item = match run {
        Some(run_id) => load_items(db, &run_id, display, fx)?.into_iter().find(|x| x.id == id),
        None => None,
    };
    let c = db.conn();
    let item = match item {
        Some(i) => i,
        None => c
            .query_row(
                "SELECT id, platform_id, external_id, title, url, image, price, currency, original_price, sales, rating, reviews, shop, location,
                        condition, first_seen_at, last_seen_at, shipping, coupon_price, shop_json, review_json FROM listings WHERE id=?1",
                [id],
                |r| {
                    let cand = Candidate {
                        platform_id: r.get(1)?, external_id: r.get(2)?, title: r.get(3)?, url: r.get(4)?, image: r.get(5)?,
                        price: r.get(6)?, currency: r.get(7)?, original_price: r.get(8)?, sales: r.get(9)?, rating: r.get(10)?,
                        reviews: r.get(11)?, shop: r.get(12)?, condition: r.get(14)?, shipping: r.get(17)?, coupon_price: r.get(18)?,
                        ..Default::default()
                    };
                    let mut it = base_item(&cand, r.get(0)?, MatchInfo::default(), display, fx, "");
                    it.provisional = false;
                    it.location = r.get(13)?;
                    it.first_seen_at = r.get(15)?;
                    it.last_seen_at = r.get(16)?;
                    apply_enrichment(&mut it, r.get(19)?, r.get(20)?, None, None, None);
                    Ok(it)
                },
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("商品"))?,
    };
    let mut stmt = c.prepare("SELECT captured_at, price, currency, source FROM price_points WHERE listing_id=?1 ORDER BY captured_at")?;
    let history: Vec<PricePoint> = stmt
        .query_map([id], |r| Ok(PricePoint { t: r.get(0)?, price: r.get(1)?, currency: r.get(2)?, source: r.get(3)? }))?
        .filter_map(|r| r.ok())
        .collect();
    let watch_id: Option<i64> = c.query_row("SELECT id FROM watches WHERE listing_id=?1 LIMIT 1", [id], |r| r.get(0)).optional()?;
    let deal = item.price.and_then(|p| crate::deal::for_listing(&c, id, p, &item.currency));
    let also_on = item.also_on.clone();
    Ok(ListingDetail { item, deal, history, also_on, watch_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::BUILTIN;

    fn all() -> Vec<Platform> {
        BUILTIN.iter().cloned().map(|mut p| {
            p.enabled = ["taobao", "jd", "xianyu", "amazon_de", "ebay_de", "kleinanzeigen"].contains(&p.id.as_str());
            p
        }).collect()
    }

    #[test]
    fn platform_choice_skips_unlogged_required_unless_explicit() {
        let all = all();
        let status = |id: &str| if id == "jd" { "logged_in".to_string() } else { "logged_out".to_string() };
        let plan = SearchPlan { platforms: vec!["taobao".into(), "jd".into(), "amazon_de".into()], product: ProductSpec { category: "耳机".into(), ..Default::default() }, ..Default::default() };
        let c = choose_platforms(&plan, &all, &status);
        assert_eq!(c.chosen, vec!["jd".to_string(), "amazon_de".to_string()]);
        assert_eq!(c.skipped.len(), 1);
        assert_eq!(c.skipped[0].reason, "login_required");
        assert!(c.reason.contains("需要先登录"));
        // 用户点名的平台全部保留（发起时已拦截未登录）
        let explicit = SearchPlan { platforms_explicit: true, ..plan.clone() };
        assert_eq!(choose_platforms(&explicit, &all, &status).chosen.len(), 3);
    }

    #[test]
    fn platform_choice_adds_secondhand_for_used() {
        let all = all();
        let status = |_: &str| "logged_in".to_string();
        let plan = SearchPlan { platforms: vec!["amazon_de".into()], condition: "used".into(), instruction: "正品 二手 索尼耳机".into(), ..Default::default() };
        let c = choose_platforms(&plan, &all, &status);
        assert!(c.chosen.contains(&"kleinanzeigen".to_string()), "{:?}", c.chosen);
        assert!(!c.chosen.contains(&"xianyu".to_string()), "region filtered: {:?}", c.chosen);
        assert!(c.reason.contains("正品"));
    }

    #[test]
    fn enrichment_roundtrip_through_db() {
        let db = Db::memory().unwrap();
        let fx = crate::fx::Fx::new(&db);
        let c = Candidate { platform_id: "jd".into(), external_id: "100".into(), title: "索尼 WH-1000XM5".into(), price: Some(2299.0), currency: "CNY".into(), condition: "new".into(), badges: vec!["自营".into()], ..Default::default() };
        let conn = db.conn();
        let id = upsert_listing(&conn, &c, &now_iso()).unwrap();
        conn.execute("INSERT INTO runs(id, instruction, plan, status, trigger, started_at) VALUES('r1','x','{}','done','manual',?1)", [now_iso()]).unwrap();
        let shop = ShopInfo { kind: "self_operated".into(), label: "平台自营".into(), credibility: 92, ..Default::default() };
        conn.execute("UPDATE listings SET shop_json=?2 WHERE id=?1", params![id, serde_json::to_string(&shop).unwrap()]).unwrap();
        let risk = risk::compose(vec![], 0, None, false);
        let tags = vec![ItemTag { kind: "self_operated".into(), label: "平台自营".into(), tone: "good".into() }];
        conn.execute(
            "INSERT INTO run_results(run_id, listing_id, rank, score, verdict, reasons, flags, cluster, ai_checked, also_on, display_price, risk_json, tags_json, final_score)
             VALUES('r1',?1,1,90,'exact','[]','[]',NULL,1,'[]',2299,?2,?3,88)",
            params![id, serde_json::to_string(&risk).unwrap(), serde_json::to_string(&tags).unwrap()],
        ).unwrap();
        drop(conn);
        let raw = reviews::ReviewRaw { rating: Some(4.8), count: Some(1200), fetched_at: now_iso(), ..Default::default() };
        reviews::store(&db, &id, &raw, &reviews::rule_insight(&raw));
        let items = load_items(&db, "r1", "CNY", &fx).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.shop_info.as_ref().unwrap().label, "平台自营");
        assert_eq!(it.risk.as_ref().unwrap().level, "low");
        assert_eq!(it.tags[0].label, "平台自营");
        assert_eq!(it.final_score, Some(88));
        assert_eq!(it.review_insight.as_ref().unwrap().count, Some(1200));
        assert!(reviews::cached(&db, &id, 24).is_some());
        let detail = listing_detail(&db, &fx, &id, "CNY").unwrap();
        assert_eq!(detail.item.final_score, Some(88));
    }
}
