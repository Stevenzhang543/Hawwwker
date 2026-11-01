//! 搜索管线：理解 → 多平台并行搜索 → 抽取 → 过滤 → AI 校验 → 聚类排序 → 入库 → AI 总结。
//! 每次运行都有取消令牌、总超时与终态事件；任何异常都会把运行记录落为 error（修复 v3.4 的"永远 RUNNING"）。

use super::filter::{self, clusters, price_in, rule_match};
use super::parse::Candidate;
use super::{rules, RunHandle};
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
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let fut = execute(&st, app.as_ref(), &h, p0, needs_ai);
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

async fn execute(st: &AppState, app: Option<&AppHandle>, h: &Arc<RunHandle>, mut plan: SearchPlan, needs_ai: bool) -> AppResult<RunOutcome> {
    let t0 = Instant::now();
    let s = settings::load(&st.db);
    let all = platforms::all(&st.db);
    let mut ai_calls = 0u32;

    /* ---------------- ① 理解 ---------------- */
    stage(h, app, "plan", "AI 理解需求", 0.04);
    if needs_ai {
        let ctx = PlanCtx { platforms: &all, settings: &s };
        plan = planner::plan(&st.ai, &plan.instruction, &ctx).await?;
        ai_calls += 1;
    }
    st.db.conn().execute("UPDATE runs SET plan=?2 WHERE id=?1", params![h.id, serde_json::to_string(&plan)?])?;
    h.emit(app, RunEventBody::Plan { plan: plan.clone() });

    /* ---------------- ② 多平台搜索 ---------------- */
    stage(h, app, "search", "多平台并行搜索", 0.12);
    let selected: Vec<Platform> = plan.platforms.iter().filter_map(|id| all.iter().find(|p| &p.id == id).cloned()).collect();
    for p in selected.iter() {
        platform_state(h, app, &p.id, "queued", None, None, None);
    }
    let total = selected.len().max(1) as f64;
    let done_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let wait_secs = s.search.login_wait_secs as u64;
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
            // 已登录但过期：允许弹窗等待；未登录：只有开启"搜索时允许弹窗"才等
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
            stage(&h, app, "search", "多平台并行搜索", 0.12 + 0.43 * (n as f64 / total));
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

    /* ---------------- ③ 抽取 ---------------- */
    stage(h, app, "extract", "整理商品信息", 0.58);
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

    /* ---------------- ④ AI 校验 ---------------- */
    stage(h, app, "verify", "AI 逐条核对是否符合需求", 0.66);
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
        if m.verdict == "mismatch" {
            hidden += 1;
        } else if m.verdict == "exact" || m.verdict == "close" {
            matched += 1;
        }
    }
    h.emit(app, RunEventBody::Verify { checked: scored.len() as u32, matched, hidden });

    /* ---------------- ⑤ 聚类与排序 ---------------- */
    stage(h, app, "rank", "同款聚合与排序", 0.8);
    let display = s.display_currency.clone();
    let rank_of = |v: &str| match v {
        "exact" => 0,
        "close" => 1,
        "related" => 2,
        _ => 3,
    };
    let dp = |c: &Candidate| price_in(c, &display, &st.fx);
    scored.sort_by(|a, b| {
        let (ra, rb) = (rank_of(&a.1.verdict), rank_of(&b.1.verdict));
        let mismatch_order = (ra == 3).cmp(&(rb == 3));
        if mismatch_order != std::cmp::Ordering::Equal {
            return mismatch_order;
        }
        let price_cmp = |x: Option<f64>, y: Option<f64>, asc: bool| match (x, y) {
            (Some(x), Some(y)) => if asc { x.total_cmp(&y) } else { y.total_cmp(&x) },
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };
        match plan.sort.as_str() {
            "price_asc" => ra.min(2).cmp(&rb.min(2)).then(price_cmp(dp(&a.0), dp(&b.0), true)),
            "price_desc" => ra.min(2).cmp(&rb.min(2)).then(price_cmp(dp(&a.0), dp(&b.0), false)),
            "sales" => b.0.sales.unwrap_or(0).cmp(&a.0.sales.unwrap_or(0)).then(ra.cmp(&rb)),
            "rating" => b.0.rating.unwrap_or(0.0).total_cmp(&a.0.rating.unwrap_or(0.0)).then(ra.cmp(&rb)),
            _ => ra.cmp(&rb).then(b.1.score.cmp(&a.1.score)).then(price_cmp(dp(&a.0), dp(&b.0), true)),
        }
    });
    let limit = plan.limit as usize;
    let mut visible: Vec<(Candidate, MatchInfo, String)> = vec![];
    let mut tail: Vec<(Candidate, MatchInfo, String)> = vec![];
    for x in scored.into_iter() {
        if x.1.verdict == "mismatch" {
            if tail.len() < 15 {
                tail.push(x);
            }
        } else if visible.len() < limit {
            visible.push(x);
        }
    }
    visible.extend(tail);
    let cl = clusters(&visible.iter().map(|(c, _, k)| (c.platform_id.clone(), k.clone(), c.title.clone())).collect::<Vec<_>>());

    /* ---------------- 入库 ---------------- */
    let run_started: String = st.db.conn().query_row("SELECT started_at FROM runs WHERE id=?1", [&h.id], |r| r.get(0)).unwrap_or_else(|_| now_iso());
    let mut items: Vec<ResultItem> = vec![];
    let mut new_ids: Vec<String> = vec![];
    {
        let conn = st.db.conn();
        let now = now_iso();
        for (rank, ((c, m, _), cluster)) in visible.iter().zip(cl.iter()).enumerate() {
            let id: String = conn.query_row(
                "INSERT INTO listings(id, platform_id, external_id, title, url, image, price, currency, original_price, sales, rating, reviews,
                   shop, location, condition, first_seen_at, last_seen_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,NULL,?14,?15,?15)
                 ON CONFLICT(platform_id, external_id) DO UPDATE SET title=excluded.title,
                   url=CASE WHEN excluded.url<>'' THEN excluded.url ELSE listings.url END,
                   image=COALESCE(excluded.image, listings.image), price=COALESCE(excluded.price, listings.price),
                   currency=excluded.currency, original_price=excluded.original_price, sales=COALESCE(excluded.sales, listings.sales),
                   rating=COALESCE(excluded.rating, listings.rating), reviews=COALESCE(excluded.reviews, listings.reviews),
                   shop=COALESCE(excluded.shop, listings.shop),
                   condition=CASE WHEN excluded.condition<>'unknown' THEN excluded.condition ELSE listings.condition END,
                   last_seen_at=excluded.last_seen_at
                 RETURNING id",
                params![uuid::Uuid::new_v4().to_string(), c.platform_id, c.external_id, c.title, c.url, c.image, c.price, c.currency,
                        c.original_price, c.sales, c.rating, c.reviews, c.shop, c.condition, now],
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
            let (first_seen, watched): (String, bool) = conn.query_row(
                "SELECT l.first_seen_at, EXISTS(SELECT 1 FROM watches w WHERE w.listing_id=l.id) FROM listings l WHERE l.id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )?;
            if first_seen >= run_started {
                new_ids.push(id.clone());
            }
            let spark: Vec<f64> = {
                let mut stmt = conn.prepare("SELECT price FROM (SELECT price, captured_at FROM price_points WHERE listing_id=?1 ORDER BY captured_at DESC LIMIT 12) ORDER BY captured_at")?;
                let rows = stmt.query_map([&id], |r| r.get::<_, f64>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            let display_price = c.price.and_then(|p| st.fx.convert(p, &c.currency, &display));
            items.push(ResultItem {
                id: id.clone(),
                platform_id: c.platform_id.clone(),
                external_id: c.external_id.clone(),
                title: c.title.clone(),
                url: c.url.clone(),
                image: c.image.clone(),
                price: c.price,
                currency: c.currency.clone(),
                display_price,
                original_price: c.original_price,
                sales: c.sales,
                rating: c.rating,
                reviews: c.reviews,
                shop: c.shop.clone(),
                location: None,
                condition: c.condition.clone(),
                match_info: m.clone(),
                cluster_id: cluster.clone(),
                also_on: vec![],
                watched,
                rank: rank as u32 + 1,
                first_seen_at: first_seen,
                last_seen_at: now.clone(),
                spark,
            });
        }
        // 同款"也在"
        let mut by_cluster: HashMap<String, Vec<AlsoOn>> = HashMap::new();
        for it in items.iter() {
            if let Some(cid) = &it.cluster_id {
                by_cluster.entry(cid.clone()).or_default().push(AlsoOn { listing_id: it.id.clone(), platform_id: it.platform_id.clone(), price: it.price, currency: it.currency.clone(), url: it.url.clone() });
            }
        }
        for it in items.iter_mut() {
            if let Some(cid) = &it.cluster_id {
                it.also_on = by_cluster.get(cid).map(|v| v.iter().filter(|a| a.listing_id != it.id).cloned().collect()).unwrap_or_default();
            }
        }
        for it in items.iter() {
            conn.execute(
                "INSERT OR REPLACE INTO run_results(run_id, listing_id, rank, score, verdict, reasons, flags, cluster, ai_checked, also_on, display_price)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![h.id, it.id, it.rank, it.match_info.score, it.match_info.verdict, serde_json::to_string(&it.match_info.reasons)?,
                        serde_json::to_string(&it.match_info.flags)?, it.cluster_id, it.match_info.ai_checked as i64,
                        serde_json::to_string(&it.also_on)?, it.display_price],
            )?;
        }
    }
    h.emit(app, RunEventBody::Results { items: items.clone() });

    /* ---------------- ⑥ 总结 ---------------- */
    let best_price = items
        .iter()
        .filter(|r| r.match_info.verdict == "exact" || r.match_info.verdict == "close")
        .filter_map(|r| r.display_price)
        .fold(None, |a: Option<f64>, p| Some(a.map_or(p, |x| x.min(p))));
    stage(h, app, "summarize", "AI 总结与推荐", 0.9);
    let summary = if items.iter().any(|r| r.match_info.verdict != "mismatch") {
        ai_calls += 1;
        match verify::summarize(&st.ai, &plan, &items, &display).await {
            Ok(sm) => {
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
                EXISTS(SELECT 1 FROM watches w WHERE w.listing_id=l.id)
         FROM run_results rr JOIN listings l ON l.id=rr.listing_id WHERE rr.run_id=?1 ORDER BY rr.rank",
    )?;
    let rows = stmt.query_map([run_id], |r| {
        let price: Option<f64> = r.get(6)?;
        let currency: String = r.get(7)?;
        Ok(ResultItem {
            id: r.get(0)?,
            platform_id: r.get(1)?,
            external_id: r.get(2)?,
            title: r.get(3)?,
            url: r.get(4)?,
            image: r.get(5)?,
            display_price: price.and_then(|p| fx.convert(p, &currency, display)),
            price,
            currency,
            original_price: r.get(8)?,
            sales: r.get(9)?,
            rating: r.get(10)?,
            reviews: r.get(11)?,
            shop: r.get(12)?,
            location: r.get(13)?,
            condition: r.get(14)?,
            first_seen_at: r.get(15)?,
            last_seen_at: r.get(16)?,
            rank: r.get(17)?,
            match_info: MatchInfo {
                score: r.get(18)?,
                verdict: r.get(19)?,
                reasons: serde_json::from_str(&r.get::<_, String>(20)?).unwrap_or_default(),
                flags: serde_json::from_str(&r.get::<_, String>(21)?).unwrap_or_default(),
                ai_checked: r.get::<_, i64>(23)? != 0,
            },
            cluster_id: r.get(22)?,
            also_on: serde_json::from_str(&r.get::<_, String>(24)?).unwrap_or_default(),
            watched: r.get::<_, i64>(25)? != 0,
            spark: vec![],
        })
    })?;
    let mut items: Vec<ResultItem> = rows.filter_map(|r| r.ok()).collect();
    for it in items.iter_mut() {
        it.spark = spark(&c, &it.id);
    }
    Ok(items)
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
                "SELECT id, platform_id, external_id, title, url, image, price, currency, original_price, sales, rating, reviews, shop, location, condition, first_seen_at, last_seen_at FROM listings WHERE id=?1",
                [id],
                |r| {
                    let price: Option<f64> = r.get(6)?;
                    let currency: String = r.get(7)?;
                    Ok(ResultItem {
                        id: r.get(0)?, platform_id: r.get(1)?, external_id: r.get(2)?, title: r.get(3)?, url: r.get(4)?, image: r.get(5)?,
                        display_price: price.and_then(|p| fx.convert(p, &currency, display)), price, currency, original_price: r.get(8)?,
                        sales: r.get(9)?, rating: r.get(10)?, reviews: r.get(11)?, shop: r.get(12)?, location: r.get(13)?, condition: r.get(14)?,
                        first_seen_at: r.get(15)?, last_seen_at: r.get(16)?, rank: 0, match_info: MatchInfo::default(), cluster_id: None,
                        also_on: vec![], watched: false, spark: vec![],
                    })
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
    let also_on = item.also_on.clone();
    Ok(ListingDetail { item, history, also_on, watch_id })
}
