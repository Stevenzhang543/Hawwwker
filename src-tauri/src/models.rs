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
    /// v26.03 检索深度：quick（不看评论）| standard（前 N 件读评论）| deep（更多商品 + 店铺评价）
    pub depth: String,
    /// standard / deep 下读取评论的商品数
    pub review_top: u32,
    /// 自动隐藏高风险商品（折叠到"可能不相关 / 高风险"）
    pub hide_high_risk: bool,
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
            depth: "standard".into(),
            review_top: 6,
            hide_high_risk: true,
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
    /// v26.03：已停用 / 不再推荐的模型 ID（界面提示更换）
    #[serde(default)]
    pub retired: Vec<String>,
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
    /// v26.03：最近一次主动验证（打开账号页确认未跳登录）的时间
    pub checked_at: Option<String>,
    /// v26.03：本机是否有会话备份（浏览器意外丢失 Cookie 时自动恢复）
    pub vaulted: bool,
}

/// v26.03：搜索前的登录预检结果。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginCheck {
    pub platform_id: String,
    /// required | recommended | optional
    pub need: String,
    /// 与 LoginState.status 相同
    pub status: String,
    /// 可以直接搜索（已登录，或不需要登录）
    pub ok: bool,
    /// 这次预检是否做了主动验证
    pub verified_now: bool,
    pub message: Option<String>,
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
    /// v26.03：平台由用户指定（选择器或在指令里点名），而不是 AI / 默认选择；指定的平台必须先登录
    pub platforms_explicit: bool,
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
            platforms_explicit: false,
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

/// v26.03：结果标签。tone：good | neutral | warn | bad
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemTag {
    /// flagship | official | self_operated | authorized | distributor | enterprise | individual | studio | overseas
    /// | secondhand | marketplace | free_shipping | good_deal | high_sales | well_reviewed | risk | …
    pub kind: String,
    pub label: String,
    pub tone: String,
}

/// v26.03：店铺信息与可信度。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ShopInfo {
    pub name: Option<String>,
    /// flagship 旗舰店 | official 官方店 | self_operated 平台自营 | authorized 专卖店/授权 | distributor 专营店
    /// | enterprise 企业店 | individual 个人卖家 | studio 小工作室 | overseas 海外/代购 | secondhand 二手商家
    /// | marketplace 第三方卖家 | unknown
    pub kind: String,
    pub label: String,
    /// 店铺评分，统一到 0–5
    pub rating: Option<f64>,
    /// 细项评分，例如 描述 / 服务 / 物流（0–5）或 好评率（0–100）
    pub scores: BTreeMap<String, f64>,
    /// 开店年限
    pub years: Option<f64>,
    pub followers: Option<u64>,
    /// 可信度 0–100（越高越可信）
    pub credibility: u32,
    /// 依据（中文短句）
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RiskFactor {
    /// counterfeit | price_anomaly | used | refurbished | accessory | function | spec_mismatch | shop | reviews | no_reviews | new_seller | overseas | other
    pub kind: String,
    pub label: String,
    /// 对风险分的贡献（正数加风险，负数减风险）
    pub weight: i32,
}

/// v26.03：商品风险评估。score 越高风险越大。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RiskInfo {
    pub score: u32,
    /// low | medium | high
    pub level: String,
    pub factors: Vec<RiskFactor>,
    /// 一句话结论
    pub summary: String,
    pub ai_checked: bool,
}

/// v26.03：评论检索与分析结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewInsight {
    /// 商品评分（0–5）
    pub rating: Option<f64>,
    /// 评论总数
    pub count: Option<u64>,
    /// 好评率（0–100）
    pub good_rate: Option<f64>,
    /// 实际读取的评论条数
    pub sampled: u32,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    /// 假货、质量、售后等风险信号
    pub red_flags: Vec<String>,
    /// 与用户需求的对应（例如"多条评论提到降噪效果好"）
    pub match_note: Option<String>,
    /// 评论综合分 0–100
    pub score: Option<u32>,
    /// product | product+shop | shop | none
    pub source: String,
    /// 代表性评论原文（≤5 条，截断）
    pub snippets: Vec<String>,
    /// 店铺评价摘要
    pub shop_note: Option<String>,
    /// none = 没读到评论（需要登录、页面无评论等）时的原因
    pub unavailable_reason: Option<String>,
    pub ai_checked: bool,
    pub fetched_at: Option<String>,
}

/// v26.03：平台判定结果中被跳过的平台。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformSkip {
    pub platform_id: String,
    /// login_required | disabled | not_relevant | region
    pub reason: String,
    pub message: String,
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
    /// v26.03：最终标签（旗舰店 / 官方店 / 平台自营 / 个人卖家 / 小工作室 / 包邮 / 好价 …）
    #[serde(default)]
    pub tags: Vec<ItemTag>,
    /// v26.03：店铺信息与可信度
    #[serde(default)]
    pub shop_info: Option<ShopInfo>,
    /// v26.03：商品风险（正品、功能、二手、价格异常…）
    #[serde(default)]
    pub risk: Option<RiskInfo>,
    /// v26.03：评论检索与分析（商品评论 + 店铺评价）
    #[serde(default)]
    pub review_insight: Option<ReviewInsight>,
    /// v26.03：最终细筛综合分 0–100（匹配、风险、店铺、评论、价格加权）
    #[serde(default)]
    pub final_score: Option<u32>,
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
    /// v26.03 平台判定：最终参与搜索的平台、跳过的平台与理由
    PlatformPlan { chosen: Vec<String>, skipped: Vec<PlatformSkip>, reason: String },
    /// v26.03 漏斗：每个筛选阶段的输入 / 保留数量
    Funnel { stage: String, label: String, input: u32, kept: u32, note: Option<String> },
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
    /// auto = true：用户明确要求"用 app 去做"，界面收到后直接执行（v26.03）
    Search {
        label: String,
        instruction: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        platforms: Option<Vec<String>>,
        #[serde(default)]
        auto: bool,
    },
    Watch {
        label: String,
        #[serde(rename = "listingId", default)]
        listing_id: String,
        #[serde(rename = "targetPrice", default)]
        target_price: Option<f64>,
        /// 直接用商品链接追踪（listingId 为空时）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default)]
        auto: bool,
    },
    Open {
        label: String,
        url: String,
        #[serde(default)]
        auto: bool,
    },
    Navigate {
        label: String,
        route: String,
        #[serde(default)]
        auto: bool,
    },
    /// v26.03：创建定时监控
    Monitor {
        label: String,
        instruction: String,
        #[serde(rename = "intervalMinutes", default)]
        interval_minutes: Option<u32>,
        #[serde(rename = "targetPrice", default)]
        target_price: Option<f64>,
        #[serde(default)]
        auto: bool,
    },
    /// v26.03：发起某个平台的登录
    Login {
        label: String,
        #[serde(rename = "platformId")]
        platform_id: String,
        #[serde(default)]
        auto: bool,
    },
    /// v26.03：修改设置。key：theme | displayCurrency | role | depth | region
    Setting {
        label: String,
        key: String,
        value: String,
        #[serde(default)]
        auto: bool,
    },
    /// v26.03：对当前搜索结果的操作。op：export_csv | copy_list | filter_exact | sort_landed | cancel
    Results {
        label: String,
        op: String,
        #[serde(default)]
        auto: bool,
    },
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
    /// v26.03：chat（默认）| run_followup（Copilot 发起的搜索完成后，App 自动请它点评结果；
    /// 这条输入以 role = "event" 保存，界面显示为事件而不是用户气泡）
    #[serde(default)]
    pub kind: Option<String>,
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
        assert_eq!(a, CopilotAction::Watch { label: "x".into(), listing_id: "l1".into(), target_price: Some(9.5), url: None, auto: false });
    }
}

/// v26.03：服务商 /models 接口返回的可用模型。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelList {
    pub models: Vec<String>,
    /// 错误时的说明（例如 Key 无效、服务商不支持列出模型）
    pub message: Option<String>,
}
