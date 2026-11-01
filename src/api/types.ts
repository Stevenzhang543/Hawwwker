/**
 * Hawwwker v26.01 — IPC 契约（与 src-tauri/src/models.rs 一一对应，serde camelCase）。
 * 修改这里时必须同步修改 Rust 侧模型。
 */

export type Role = "consumer" | "merchant" | "analyst";
export type Region = "cn" | "intl";
export type ThemeMode = "system" | "light" | "dark";
export type MotionMode = "full" | "reduced";
export type WindowEffect = "mica" | "acrylic" | "none";

/* ------------------------------------------------------------------ 设置 */
export interface SearchPrefs {
  /** 每个平台最多使用的搜索词数量 */
  maxQueries: number;
  /** 每个平台目标抓取条数 */
  perPlatform: number;
  /** 不同平台并行标签页数（1-4） */
  concurrency: number;
  /** 自动化浏览器放在屏幕外运行 */
  background: boolean;
  /** 已登录但会话过期时，允许弹出窗口等待用户处理（秒）；0 = 不弹窗 */
  loginWaitSecs: number;
  /** 未登录平台遇到登录墙时也弹窗 */
  popupOnLogin: boolean;
  /** 单个平台总超时（秒） */
  platformTimeoutSecs: number;
  /** 默认最终结果条数 */
  resultLimit: number;
  /** 默认地区 */
  defaultRegion: "all" | Region;
  /** 保活：定期访问已登录平台首页 */
  keepalive: boolean;
}

export interface NotifyPrefs {
  system: boolean;
  /** 降价提醒阈值（百分比） */
  dropPercent: number;
  webhookKind: "none" | "wecom" | "feishu" | "dingtalk" | "slack" | "discord" | "generic";
  webhookUrl: string;
}

export interface AppSettings {
  role: Role;
  displayCurrency: string;
  theme: ThemeMode;
  motion: MotionMode;
  windowEffect: WindowEffect;
  sidebarCollapsed: boolean;
  onboarded: boolean;
  search: SearchPrefs;
  notify: NotifyPrefs;
}

export type AppSettingsPatch = Partial<Omit<AppSettings, "search" | "notify">> & {
  search?: Partial<SearchPrefs>;
  notify?: Partial<NotifyPrefs>;
};

/* ------------------------------------------------------------------ AI */
export type AiProtocol = "openai" | "anthropic";

export interface AiConfig {
  protocol: AiProtocol;
  provider: string; // 预设 id，如 deepseek / qwen / openai / anthropic / ollama / custom
  baseUrl: string;
  model: string;
  fastModel: string;
  temperature: number;
  timeoutSecs: number;
  maxConcurrency: number;
  hasKey: boolean;
  /** API Key 尾号提示，例如 "••••a1b2"；从不返回完整 Key */
  keyHint: string;
}

export interface AiConfigInput {
  protocol: AiProtocol;
  provider: string;
  baseUrl: string;
  model: string;
  fastModel?: string;
  temperature?: number;
  timeoutSecs?: number;
  maxConcurrency?: number;
  /** undefined = 保持原 Key；"" = 清除 */
  apiKey?: string;
}

export interface AiStatus {
  configured: boolean;
  verified: boolean;
  model: string;
  provider: string;
  verifiedAt?: string | null;
  latencyMs?: number | null;
  lastError?: string | null;
  sample?: string | null;
}

export interface AiPreset {
  id: string;
  name: string;
  protocol: AiProtocol;
  baseUrl: string;
  models: string[];
  fastModel?: string;
  keyUrl: string;
  note: string;
  region: Region | "local";
}

export interface AiUsage {
  today: { calls: number; promptTokens: number; completionTokens: number };
  total: { calls: number; promptTokens: number; completionTokens: number };
  days: { day: string; calls: number; tokens: number }[];
}

/* ------------------------------------------------------------------ 平台 */
export type PlatformKind = "retail" | "marketplace" | "c2c" | "social" | "wholesale" | "compare";
export type LoginNeed = "required" | "recommended" | "optional";

export interface PlatformInfo {
  id: string;
  family: string;
  familyName: string;
  name: string;
  edition: string;
  region: Region;
  country: string;
  kind: PlatformKind;
  currency: string;
  locale: string;
  color: string;
  glyph: string;
  home: string;
  searchUrl: string;
  defaultSearchUrl: string;
  accountGroup: string;
  login: LoginNeed;
  beta: boolean;
  enabled: boolean;
  custom: boolean;
  note: string;
}

export interface PlatformPatch {
  enabled?: boolean;
  /** 空字符串 = 恢复默认模板 */
  searchUrl?: string;
}

export interface CustomPlatformInput {
  name: string;
  region: Region;
  currency: string;
  locale: string;
  searchUrl: string;
  linkPattern?: string;
  home?: string;
  kind?: PlatformKind;
}

export type LoginStatus = "unknown" | "logged_out" | "logged_in" | "expired" | "not_needed";
export type LoginFlow = "idle" | "opening" | "waiting" | "verifying" | "success" | "cancelled" | "timeout" | "failed";

export interface LoginState {
  platformId: string;
  status: LoginStatus;
  flow: LoginFlow;
  verifiedAt?: string | null;
  lastWallAt?: string | null;
  cookieExpires?: string | null;
  message?: string | null;
  /** 与此平台共享登录的其他平台 id */
  sharedWith: string[];
}

export interface BrowserInfo {
  executable?: string | null;
  running: boolean;
  profileDir: string;
}

/* ------------------------------------------------------------------ 搜索 */
export type Goal = "buy" | "compare" | "track" | "research" | "gift" | "source";
export type Condition = "any" | "new" | "used";
export type SortKey = "relevance" | "price_asc" | "price_desc" | "sales" | "rating" | "recent";

export interface ProductSpec {
  category: string;
  brand?: string | null;
  model?: string | null;
  attributes: string[];
  mustHave: string[];
  mustNot: string[];
}

export interface PlatformQuery {
  platformId: string;
  q: string;
  lang: string;
}

export interface MonitorHint {
  intervalMinutes: number;
  targetPrice?: number | null;
}

export interface SearchPlan {
  instruction: string;
  goal: Goal;
  product: ProductSpec;
  keywords: string[];
  exclude: string[];
  priceMin?: number | null;
  priceMax?: number | null;
  currency: string;
  condition: Condition;
  platforms: string[];
  queries: PlatformQuery[];
  sort: SortKey;
  limit: number;
  monitor?: MonitorHint | null;
  notes: string[];
  parser: "ai" | "ai+rules" | "rules";
}

export type RunStatus = "running" | "done" | "error" | "cancelled" | "interrupted";
export type Stage = "plan" | "search" | "extract" | "verify" | "rank" | "summarize" | "done";
export type PlatformRunState = "queued" | "searching" | "waiting_user" | "done" | "error" | "skipped" | "login_required";
export type Verdict = "exact" | "close" | "related" | "mismatch";

export interface RunSummary {
  id: string;
  instruction: string;
  status: RunStatus;
  trigger: "manual" | "monitor";
  startedAt: string;
  finishedAt?: string | null;
  fetched: number;
  kept: number;
  platforms: string[];
  bestPrice?: number | null;
  currency: string;
  error?: string | null;
}

export interface PlatformStat {
  platformId: string;
  state: PlatformRunState;
  count: number;
  ms: number;
  message?: string | null;
}

export interface MatchInfo {
  score: number;
  verdict: Verdict;
  reasons: string[];
  flags: string[];
  aiChecked: boolean;
}

export interface AlsoOn {
  listingId: string;
  platformId: string;
  price?: number | null;
  currency: string;
  url: string;
}

export interface ResultItem {
  id: string;
  platformId: string;
  externalId: string;
  title: string;
  url: string;
  image?: string | null;
  price?: number | null;
  currency: string;
  /** 换算到显示币种后的价格 */
  displayPrice?: number | null;
  originalPrice?: number | null;
  sales?: number | null;
  rating?: number | null;
  reviews?: number | null;
  shop?: string | null;
  location?: string | null;
  condition: "new" | "used" | "unknown";
  match: MatchInfo;
  clusterId?: string | null;
  alsoOn: AlsoOn[];
  watched: boolean;
  rank: number;
  firstSeenAt: string;
  lastSeenAt: string;
  /** 最近价格（用于迷你走势），按时间升序 */
  spark: number[];
}

export interface AiSummary {
  headline: string;
  bestPick?: { id: string; why: string } | null;
  bullets: string[];
  priceInsight: string;
  caveats: string[];
  nextActions: CopilotAction[];
}

export interface RunStats {
  fetched: number;
  kept: number;
  hidden: number;
  duplicates: number;
  dropped: Record<string, number>;
  elapsedMs: number;
  platforms: PlatformStat[];
  aiCalls: number;
}

export type RunEventBody =
  | { type: "stage"; stage: Stage; label: string; progress: number }
  | { type: "plan"; plan: SearchPlan }
  | { type: "platform"; platformId: string; state: PlatformRunState; count?: number | null; message?: string | null; ms?: number | null }
  | { type: "filter"; fetched: number; kept: number; dropped: Record<string, number>; duplicates: number }
  | { type: "verify"; checked: number; matched: number; hidden: number }
  | { type: "results"; items: ResultItem[] }
  | { type: "summary"; summary: AiSummary }
  | { type: "log"; level: "info" | "warn" | "error"; message: string }
  | { type: "done"; stats: RunStats }
  | { type: "error"; message: string }
  | { type: "cancelled" };

export type RunEvent = RunEventBody & { seq: number; t: number };

export interface RunDetail {
  summary: RunSummary;
  plan: SearchPlan;
  results: ResultItem[];
  aiSummary?: AiSummary | null;
  stats?: RunStats | null;
}

export interface PricePoint {
  t: string;
  price: number;
  currency: string;
  source: "search" | "detail";
}

export interface ListingDetail {
  item: ResultItem;
  history: PricePoint[];
  alsoOn: AlsoOn[];
  watchId?: number | null;
}

/* ------------------------------------------------------------------ 追踪与监控 */
export type WatchStatus = "new" | "ok" | "checking" | "error";

export interface WatchItem {
  id: number;
  listingId?: string | null;
  platformId: string;
  title: string;
  url: string;
  image?: string | null;
  currency: string;
  targetPrice?: number | null;
  lastPrice?: number | null;
  firstPrice?: number | null;
  lowest?: number | null;
  highest?: number | null;
  /** 相对首次记录的变化百分比 */
  changePct?: number | null;
  intervalMinutes: number;
  enabled: boolean;
  status: WatchStatus;
  lastError?: string | null;
  lastCheckedAt?: string | null;
  nextCheckAt?: string | null;
  note: string;
  createdAt: string;
  spark: number[];
}

export interface WatchAddInput {
  listingId?: string;
  url?: string;
  title?: string;
  platformId?: string;
  targetPrice?: number | null;
  intervalMinutes?: number;
  note?: string;
}

export interface WatchPatch {
  targetPrice?: number | null;
  intervalMinutes?: number;
  enabled?: boolean;
  note?: string;
  title?: string;
}

export interface Monitor {
  id: number;
  name: string;
  instruction: string;
  plan: SearchPlan;
  intervalMinutes: number;
  enabled: boolean;
  targetPrice?: number | null;
  lastRunAt?: string | null;
  nextRunAt?: string | null;
  lastStatus: "idle" | "running" | "ok" | "error";
  lastNewCount: number;
  runCount: number;
  lastRunId?: string | null;
  bestPrice?: number | null;
  currency: string;
  createdAt: string;
}

export interface MonitorCreateInput {
  instruction: string;
  plan?: SearchPlan;
  name?: string;
  intervalMinutes: number;
  targetPrice?: number | null;
}

export interface MonitorPatch {
  name?: string;
  intervalMinutes?: number;
  enabled?: boolean;
  targetPrice?: number | null;
}

export type NoticeKind = "price_drop" | "target_hit" | "new_items" | "login" | "error" | "system";

export interface Notice {
  id: number;
  kind: NoticeKind;
  level: "info" | "success" | "warning" | "error";
  title: string;
  body: string;
  /** 应用内路由，例如 /track/watchlist?id=3 */
  link: string;
  read: boolean;
  createdAt: string;
}

/* ------------------------------------------------------------------ 洞察 */
export interface TrendPoint {
  t: string;
  min: number;
  median: number;
  max: number;
  count: number;
}

export interface TrendSeries {
  key: string;
  label: string;
  kind: "watch" | "monitor";
  currency: string;
  points: TrendPoint[];
}

export interface MarketOverview {
  days: number;
  currency: string;
  kpis: {
    tracked: number;
    drops: number;
    monitors: number;
    runs: number;
    listings: number;
    avgSpreadPct: number;
  };
  platformMedians: { platformId: string; median: number; count: number }[];
  priceBands: { label: string; count: number }[];
  platformShare: { platformId: string; count: number }[];
  topDrops: { watchId: number; title: string; platformId: string; from: number; to: number; pct: number; currency: string }[];
  daily: { day: string; runs: number; listings: number }[];
}

/* ------------------------------------------------------------------ Copilot */
export type CopilotAction =
  | { type: "search"; label: string; instruction: string }
  | { type: "watch"; label: string; listingId: string; targetPrice?: number | null }
  | { type: "open"; label: string; url: string }
  | { type: "navigate"; label: string; route: string };

export interface CopilotMessage {
  id: string;
  sessionId: string;
  role: "user" | "assistant";
  content: string;
  actions: CopilotAction[];
  createdAt: string;
}

export interface CopilotSkill {
  id: string;
  name: string;
  icon: string;
  description: string;
  starter: string;
  roles: Role[];
}

export interface CopilotSendInput {
  sessionId: string;
  text: string;
  skill?: string;
  /** 前端打包的精简上下文（当前结果/选中商品等） */
  context?: unknown;
}

/* ------------------------------------------------------------------ 杂项 */
export interface FxRates {
  base: string;
  date: string;
  source: "ecb" | "builtin";
  rates: Record<string, number>;
}

export interface Bootstrap {
  version: string;
  dataDir: string;
  settings: AppSettings;
  ai: AiStatus;
  platforms: PlatformInfo[];
  logins: LoginState[];
  unread: number;
  runningRuns: string[];
  fx: FxRates;
}

export interface AppError {
  code: "AI_REQUIRED" | "NOT_FOUND" | "BROWSER" | "LOGIN" | "AI" | "INVALID" | "INTERNAL" | "CANCELLED";
  message: string;
}

/* ------------------------------------------------------------------ 事件载荷 */
export interface RunEventPayload {
  runId: string;
  event: RunEvent;
}
export interface CopilotDelta {
  sessionId: string;
  messageId: string;
  delta: string;
}
export interface CopilotDone {
  sessionId: string;
  message: CopilotMessage;
}
export interface InsightDelta {
  reportId: string;
  delta: string;
}
export interface InsightDone {
  reportId: string;
  text: string;
  error?: string | null;
}
