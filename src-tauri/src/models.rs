//! IPC 模型：与 `src/api/types.ts` 一一对应（serde camelCase）。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/* ------------------------------------------------------------------ 设置 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchPrefs {
    pub max_queries: u32,
    pub per_platform: u32,
    pub concurrency: u32,
    pub background: bool,
    pub login_wait_secs: u32,
    pub popup_on_login: bool,
    pub platform_timeout_secs: u32,
    pub result_limit: u32,
    pub default_region: String,
    pub keepalive: bool,
}

impl Default for SearchPrefs {
    fn default() -> Self {
        Self {
            max_queries: 2,
            per_platform: 24,
            concurrency: 3,
            background: true,
            login_wait_secs: 180,
            popup_on_login: false,
            platform_timeout_secs: 150,
            result_limit: 40,
            default_region: "all".into(),
            keepalive: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifyPrefs {
    pub system: bool,
    pub drop_percent: f64,
    pub webhook_kind: String,
    pub webhook_url: String,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self { system: true, drop_percent: 5.0, webhook_kind: "none".into(), webhook_url: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub role: String,
    pub display_currency: String,
    pub theme: String,
    pub motion: String,
    pub window_effect: String,
    pub sidebar_collapsed: bool,
    pub onboarded: bool,
    pub search: SearchPrefs,
    pub notify: NotifyPrefs,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            role: "consumer".into(),
            display_currency: "CNY".into(),
            theme: "system".into(),
            motion: "full".into(),
            window_effect: "mica".into(),
            sidebar_collapsed: false,
            onboarded: false,
            search: SearchPrefs::default(),
            notify: NotifyPrefs::default(),
        }
    }
}

/* ------------------------------------------------------------------ AI */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub protocol: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub fast_model: String,
    pub temperature: f64,
    pub timeout_secs: u64,
    pub max_concurrency: u32,
    pub has_key: bool,
    pub key_hint: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            protocol: "openai".into(),
            provider: "deepseek".into(),
            base_url: String::new(),
            model: String::new(),
            fast_model: String::new(),
            temperature: 0.2,
            timeout_secs: 60,
            max_concurrency: 3,
            has_key: false,
            key_hint: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigInput {
    pub protocol: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub fast_model: Option<String>,
    pub temperature: Option<f64>,
    pub timeout_secs: Option<u64>,
    pub max_concurrency: Option<u32>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AiStatus {
    pub configured: bool,
    pub verified: bool,
    pub model: String,
    pub provider: String,
    pub verified_at: Option<String>,
    pub latency_ms: Option<u64>,
    pub last_error: Option<String>,
    pub sample: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreset {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub fast_model: Option<String>,
    pub key_url: String,
    pub note: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    pub day: String,
    pub calls: i64,
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub today: UsageTotals,
    pub total: UsageTotals,
    pub days: Vec<UsageDay>,
}

/* ------------------------------------------------------------------ 平台 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub id: String,
    pub family: String,
    pub family_name: String,
    pub name: String,
    pub edition: String,
    pub region: String,
    pub country: String,
    pub kind: String,
    pub currency: String,
    pub locale: String,
    pub color: String,
    pub glyph: String,
    pub home: String,
    pub search_url: String,
    pub default_search_url: String,
    pub account_group: String,
    pub login: String,
    pub beta: bool,
    pub enabled: bool,
    pub custom: bool,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPatch {
    pub enabled: Option<bool>,
    pub search_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPlatformInput {
    pub name: String,
    pub region: String,
    pub currency: String,
    pub locale: String,
    pub search_url: String,
    pub link_pattern: Option<String>,
    pub home: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoginState {
    pub platform_id: String,
    pub status: String,
    pub flow: String,
    pub verified_at: Option<String>,
    pub last_wall_at: Option<String>,
    pub cookie_expires: Option<String>,
    pub message: Option<String>,
    pub shared_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInfo {
    pub executable: Option<String>,
    pub running: bool,
    pub profile_dir: String,
    /// pipe = 管道通信（不开端口）；port = 回退的本地端口
    pub transport: String,
}

/* ------------------------------------------------------------------ 搜索 */
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProductSpec {
    pub category: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub attributes: Vec<String>,
    pub must_have: Vec<String>,
    pub must_not: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformQuery {
    pub platform_id: String,
    pub q: String,
    #[serde(default)]
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorHint {
    pub interval_minutes: u32,
    #[serde(default)]
    pub target_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchPlan {
    pub instruction: String,
    pub goal: String,
    pub product: ProductSpec,
    pub keywords: Vec<String>,
    pub exclude: Vec<String>,
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    pub currency: String,
    pub condition: String,
    pub platforms: Vec<String>,
    pub queries: Vec<PlatformQuery>,
    pub sort: String,
    pub limit: u32,
    pub monitor: Option<MonitorHint>,
    pub notes: Vec<String>,
    pub parser: String,
}

impl Default for SearchPlan {
    fn default() -> Self {
        Self {
            instruction: String::new(),
            goal: "buy".into(),
            product: ProductSpec::default(),
            keywords: vec![],
            exclude: vec![],
            price_min: None,
            price_max: None,
            currency: "CNY".into(),
            condition: "any".into(),
            platforms: vec![],
            queries: vec![],
            sort: "relevance".into(),
            limit: 40,
            monitor: None,
            notes: vec![],
            parser: "rules".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub instruction: String,
    pub status: String,
    pub trigger: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub fetched: i64,
    pub kept: i64,
    pub platforms: Vec<String>,
    pub best_price: Option<f64>,
    pub currency: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStat {
    pub platform_id: String,
    pub state: String,
    pub count: u32,
    pub ms: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInfo {
    pub score: u32,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub flags: Vec<String>,
    pub ai_checked: bool,
}

impl Default for MatchInfo {
    fn default() -> Self {
        Self { score: 50, verdict: "related".into(), reasons: vec![], flags: vec![], ai_checked: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlsoOn {
    pub listing_id: String,
    pub platform_id: String,
    pub price: Option<f64>,
    pub currency: String,
    pub url: String,
}

/// 价格判断：当前价相对本机记录的历史价格处于什么位置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DealInfo {
    /// lowest = 历史最低 · low = 近期低位 · normal = 正常 · high = 偏高 · new = 数据积累中
    pub level: String,
    pub label: String,
    pub basis: String,
    pub low: Option<f64>,
    pub median: Option<f64>,
    pub high: Option<f64>,
    pub points: u32,
    pub days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultItem {
    pub id: String,
    pub platform_id: String,
    pub external_id: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub price: Option<f64>,
    pub currency: String,
    pub display_price: Option<f64>,
    pub original_price: Option<f64>,
    /// 运费（0 = 包邮），与 price 同币种
    pub shipping: Option<f64>,
    /// 券后 / 满减后价
    pub coupon_price: Option<f64>,
    /// 到手价 =（券后价或标价）+ 运费
    pub landed_price: Option<f64>,
    /// 到手价换算到显示币种
    pub display_landed: Option<f64>,
    pub deal: Option<DealInfo>,
    /// true = 渐进式结果（规则初判，AI 核对中）
    pub provisional: bool,
    /// 用户反馈：irrelevant = 标记为不相关
    pub feedback: Option<String>,
    pub sales: Option<i64>,
    pub rating: Option<f64>,
    pub reviews: Option<i64>,
    pub shop: Option<String>,
    pub location: Option<String>,
    pub condition: String,
    #[serde(rename = "match")]
    pub match_info: MatchInfo,
    pub cluster_id: Option<String>,
    pub also_on: Vec<AlsoOn>,
    pub watched: bool,
    pub rank: u32,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub spark: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BestPick {
    pub id: String,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSummary {
    pub headline: String,
    pub best_pick: Option<BestPick>,
    pub bullets: Vec<String>,
    pub price_insight: String,
    pub caveats: Vec<String>,
    pub next_actions: Vec<CopilotAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunStats {
    pub fetched: u32,
    pub kept: u32,
    pub hidden: u32,
    pub duplicates: u32,
    pub dropped: BTreeMap<String, u32>,
    pub elapsed_ms: u64,
    pub platforms: Vec<PlatformStat>,
    pub ai_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventBody {
    Stage { stage: String, label: String, progress: f64 },
    Plan { plan: SearchPlan },
    Platform {
        #[serde(rename = "platformId")]
        platform_id: String,
        state: String,
        count: Option<u32>,
        message: Option<String>,
        ms: Option<u64>,
    },
    Filter { fetched: u32, kept: u32, dropped: BTreeMap<String, u32>, duplicates: u32 },
    Verify { checked: u32, matched: u32, hidden: u32 },
    /// 渐进式结果：某个平台搜完立即推送（规则初判）
    Partial {
        #[serde(rename = "platformId")]
        platform_id: String,
        items: Vec<ResultItem>,
    },
    Results { items: Vec<ResultItem> },
    Summary { summary: AiSummary },
    Log { level: String, message: String },
    Done { stats: RunStats },
    Error { message: String },
    Cancelled,
}

impl RunEventBody {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunEventBody::Done { .. } | RunEventBody::Error { .. } | RunEventBody::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    #[serde(flatten)]
    pub body: RunEventBody,
    pub seq: u64,
    pub t: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEventPayload {
    pub run_id: String,
    pub event: RunEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub summary: RunSummary,
    pub plan: SearchPlan,
    pub results: Vec<ResultItem>,
    pub ai_summary: Option<AiSummary>,
    pub stats: Option<RunStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricePoint {
    pub t: String,
    pub price: f64,
    pub currency: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingDetail {
    pub item: ResultItem,
    pub deal: Option<DealInfo>,
    pub history: Vec<PricePoint>,
    pub also_on: Vec<AlsoOn>,
    pub watch_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub run_id: String,
    pub plan: SearchPlan,
}

/* ------------------------------------------------------------------ 追踪与监控 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchItem {
    pub id: i64,
    pub listing_id: Option<String>,
    pub platform_id: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub currency: String,
    pub target_price: Option<f64>,
    pub last_price: Option<f64>,
    pub first_price: Option<f64>,
    pub lowest: Option<f64>,
    pub highest: Option<f64>,
    pub change_pct: Option<f64>,
    pub interval_minutes: i64,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub last_checked_at: Option<String>,
    pub next_check_at: Option<String>,
    pub note: String,
    pub created_at: String,
    pub spark: Vec<f64>,
    /// 商户：我的售价（与追踪的竞品当前价对比）
    pub my_price: Option<f64>,
    pub deal: Option<DealInfo>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchAddInput {
    pub listing_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub platform_id: Option<String>,
    pub target_price: Option<f64>,
    pub interval_minutes: Option<i64>,
    pub note: Option<String>,
}

/// `targetPrice: null` 与"未提供"需要区分：外层 Option = 是否提供，内层 = 值。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub target_price: Option<Option<f64>>,
    pub interval_minutes: Option<i64>,
    pub enabled: Option<bool>,
    pub note: Option<String>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub my_price: Option<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub instruction: String,
    pub plan: SearchPlan,
    pub interval_minutes: i64,
    pub enabled: bool,
    pub target_price: Option<f64>,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub last_status: String,
    pub last_new_count: i64,
    pub run_count: i64,
    pub last_run_id: Option<String>,
    pub best_price: Option<f64>,
    pub currency: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorCreateInput {
    pub instruction: String,
    pub plan: Option<SearchPlan>,
    pub name: Option<String>,
    pub interval_minutes: i64,
    pub target_price: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MonitorPatch {
    pub name: Option<String>,
    pub interval_minutes: Option<i64>,
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub target_price: Option<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: i64,
    pub kind: String,
    pub level: String,
    pub title: String,
    pub body: String,
    pub link: String,
    /// 外部链接（例如到价提醒的商品页，"去购买"）
    pub url: String,
    pub read: bool,
    pub created_at: String,
}

/* ------------------------------------------------------------------ 洞察 */
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub t: String,
    pub min: f64,
    pub median: f64,
    pub max: f64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendSeries {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub currency: String,
    pub points: Vec<TrendPoint>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MarketKpis {
    pub tracked: i64,
    pub drops: i64,
    pub monitors: i64,
    pub runs: i64,
    pub listings: i64,
    pub avg_spread_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMedian {
    pub platform_id: String,
    pub median: f64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelCount {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCount {
    pub platform_id: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopDrop {
    pub watch_id: i64,
    pub title: String,
    pub platform_id: String,
    pub from: f64,
    pub to: f64,
    pub pct: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCount {
    pub day: String,
    pub runs: i64,
    pub listings: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOverview {
    pub days: i64,
    pub currency: String,
    pub kpis: MarketKpis,
    pub platform_medians: Vec<PlatformMedian>,
    pub price_bands: Vec<LabelCount>,
    pub platform_share: Vec<PlatformCount>,
    pub top_drops: Vec<TopDrop>,
    pub daily: Vec<DailyCount>,
}

/* ------------------------------------------------------------------ Copilot */
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CopilotAction {
    Search { label: String, instruction: String },
    Watch {
        label: String,
        #[serde(rename = "listingId")]
        listing_id: String,
        #[serde(rename = "targetPrice", default)]
        target_price: Option<f64>,
    },
    Open { label: String, url: String },
    Navigate { label: String, route: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub actions: Vec<CopilotAction>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotSkill {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub starter: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotSendInput {
    pub session_id: String,
    pub text: String,
    pub skill: Option<String>,
    pub context: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotDelta {
    pub session_id: String,
    pub message_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotDone {
    pub session_id: String,
    pub message: CopilotMessage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightDelta {
    pub report_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightDone {
    pub report_id: String,
    pub text: String,
    pub error: Option<String>,
}

/* ------------------------------------------------------------------ v26.02 新增 */
/// 粘贴的商品链接识别结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    pub url: String,
    pub platform_id: Option<String>,
    pub platform_name: Option<String>,
    pub title: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub image: Option<String>,
    /// 建议的"全网找同款"搜索指令
    pub instruction: String,
    /// 是否已在追踪
    pub watch_id: Option<i64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalAi {
    pub found: bool,
    pub base_url: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub region: String,
    pub ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestSample {
    pub title: String,
    pub price: Option<f64>,
    pub currency: String,
    pub landed: Option<f64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestResult {
    pub platform_id: String,
    pub ok: bool,
    pub count: u32,
    pub with_price: u32,
    pub samples: Vec<SelfTestSample>,
    pub ms: u64,
    pub login_required: bool,
    pub message: Option<String>,
}

/* ------------------------------------------------------------------ 杂项 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRates {
    pub base: String,
    pub date: String,
    pub source: String,
    pub rates: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: String,
    pub data_dir: String,
    pub settings: AppSettings,
    pub ai: AiStatus,
    pub platforms: Vec<PlatformInfo>,
    pub logins: Vec<LoginState>,
    pub unread: i64,
    pub running_runs: Vec<String>,
    pub fx: FxRates,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
}

pub fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_serializes_flat() {
        let ev = RunEvent {
            body: RunEventBody::Platform {
                platform_id: "amazon_de".into(),
                state: "done".into(),
                count: Some(3),
                message: None,
                ms: Some(10),
            },
            seq: 4,
            t: 1,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "platform");
        assert_eq!(v["platformId"], "amazon_de");
        assert_eq!(v["seq"], 4);
        let c = serde_json::to_value(RunEvent { body: RunEventBody::Cancelled, seq: 1, t: 0 }).unwrap();
        assert_eq!(c["type"], "cancelled");
    }

    #[test]
    fn watch_patch_distinguishes_null() {
        let p: WatchPatch = serde_json::from_str(r#"{"targetPrice":null}"#).unwrap();
        assert_eq!(p.target_price, Some(None));
        let p: WatchPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(p.target_price, None);
    }

    #[test]
    fn copilot_action_tagged() {
        let a: CopilotAction =
            serde_json::from_str(r#"{"type":"watch","label":"x","listingId":"l1","targetPrice":9.5}"#).unwrap();
        assert_eq!(a, CopilotAction::Watch { label: "x".into(), listing_id: "l1".into(), target_price: Some(9.5) });
    }
}
