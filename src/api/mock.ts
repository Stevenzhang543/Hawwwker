/**
 * 浏览器预览（npm run dev）用的模拟后端：实现 api/index.ts 中的全部命令，并按真实节奏推送事件。
 * URL 参数：?onboarding=1 预览首次引导；?noai=1 预览"AI 未连接"守卫。
 */
import type {
  AiConfig, AiConfigInput, AiStatus, AiSummary, AppSettings, AppSettingsPatch, Bootstrap, CopilotAction, CopilotMessage,
  CopilotSendInput, CustomPlatformInput, ListingDetail, LoginState, MarketOverview, Monitor, MonitorCreateInput,
  MonitorPatch, Notice, PlatformInfo, PlatformPatch, PricePoint, ResultItem, RunDetail, RunEvent, RunEventBody, RunStats,
  RunSummary, SearchPlan, TrendSeries, Verdict, WatchAddInput, WatchItem, WatchPatch, PlatformStat,
} from "./types";
import {
  FX, PRESETS, SKILLS, fxConvert, hash, makeLogins, makeNotices, makePlatforms, productImage, rng, uid,
} from "./mockData";

/* ================================================================ 事件总线 */
const listeners = new Map<string, Set<(p: unknown) => void>>();
export function mockListen(event: string, cb: (p: unknown) => void): () => void {
  let set = listeners.get(event);
  if (!set) listeners.set(event, (set = new Set()));
  set.add(cb);
  return () => set?.delete(cb);
}
function emit(event: string, payload: unknown): void {
  listeners.get(event)?.forEach((cb) => {
    try {
      cb(payload);
    } catch (e) {
      console.error(e);
    }
  });
}

const jclone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const iso = (t = Date.now()) => new Date(t).toISOString();
const err = (code: string, message: string) => ({ code, message });

/* ================================================================ 状态 */
const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
const FORCE_ONBOARD = params.has("onboarding");
const NO_AI = params.has("noai") || FORCE_ONBOARD;

const settings: AppSettings = {
  role: "consumer",
  displayCurrency: "CNY",
  theme: (params.get("theme") as AppSettings["theme"]) || "system",
  motion: "full",
  windowEffect: "mica",
  sidebarCollapsed: false,
  onboarded: !FORCE_ONBOARD,
  search: {
    maxQueries: 2, perPlatform: 20, concurrency: 3, background: true, loginWaitSecs: 120, popupOnLogin: false,
    platformTimeoutSecs: 120, resultLimit: 30, defaultRegion: "all", keepalive: true,
  },
  notify: { system: true, dropPercent: 5, webhookKind: "none", webhookUrl: "" },
};

let aiConfig: AiConfig = {
  protocol: "openai", provider: "deepseek", baseUrl: "https://api.deepseek.com/v1", model: "deepseek-chat", fastModel: "",
  temperature: 0.2, timeoutSecs: 60, maxConcurrency: 3, hasKey: !NO_AI, keyHint: NO_AI ? "" : "••••7f3a",
};
let aiStatus: AiStatus = NO_AI
  ? { configured: !FORCE_ONBOARD && !NO_AI, verified: false, model: "", provider: "", lastError: FORCE_ONBOARD ? null : "API Key 已失效（HTTP 401）" }
  : { configured: true, verified: true, model: "deepseek-chat", provider: "deepseek", verifiedAt: iso(Date.now() - 3600_000), latencyMs: 842, sample: "你好，我已准备好帮你找货比价。" };
if (NO_AI && !FORCE_ONBOARD) aiStatus = { ...aiStatus, configured: true, model: "deepseek-chat", provider: "deepseek" };

const platforms: PlatformInfo[] = makePlatforms();
const logins: LoginState[] = makeLogins(platforms);
const notices: Notice[] = makeNotices();
let noticeSeq = notices.length;

const P = (id: string) => platforms.find((p) => p.id === id);

/* ================================================================ 商品与价格历史 */
interface Listing {
  item: ResultItem;
  history: PricePoint[];
}
const listings = new Map<string, Listing>();

function walk(seed: number, base: number, days: number, currency: string, step = 12): PricePoint[] {
  const r = rng(seed);
  const pts: PricePoint[] = [];
  let p = base * (1.08 + r() * 0.08);
  const now = Date.now();
  for (let h = days * 24; h >= 0; h -= step) {
    if (r() < 0.12) p *= 0.94 + r() * 0.04;
    else if (r() < 0.16) p *= 1.02 + r() * 0.05;
    else p *= 0.996 + r() * 0.008;
    p = Math.max(base * 0.78, Math.min(base * 1.25, p));
    pts.push({ t: iso(now - h * 3600_000), price: round(p, currency), currency, source: h % 48 === 0 ? "detail" : "search" });
  }
  pts[pts.length - 1]!.price = round(base, currency);
  return pts;
}
function round(v: number, currency: string): number {
  if (currency === "JPY" || currency === "KRW") return Math.round(v / 10) * 10;
  if (v > 200) return Math.round(v) - (Math.round(v) % 10 === 0 ? 1 : 0);
  return Math.round(v * 100) / 100;
}

/* ================================================================ 搜索运行 */
interface Run {
  summary: RunSummary;
  plan: SearchPlan;
  results: ResultItem[];
  aiSummary: AiSummary | null;
  stats: RunStats | null;
  events: RunEvent[];
  seq: number;
  timers: number[];
}
const runs = new Map<string, Run>();

const FILLER = /帮我|帮忙|请|找一下|找找|搜一下|搜索|在|上|和|的|找|一下|想买|看看|比较|对比|哪里|最便宜|便宜|以内|以下|左右|不要|排除|二手|全新|国行|价格|预算|多少钱|有没有|推荐|值得|入手|亚马逊|德国|美国|日本|英国|京东|淘宝|拼多多|闲鱼|小红书|抖音|eBay|ebay|Amazon|amazon|idealo|\d+\s*(欧元?|元|块|美元|刀|镑|日元|€|\$|£)/g;

function mockPlan(instruction: string): SearchPlan {
  const text = instruction.trim();
  let kw = text.replace(FILLER, " ").replace(/[，。,.!！？?、]/g, " ").replace(/\s+/g, " ").trim();
  if (!kw || kw.length < 2) kw = "Sony WH-1000XM5";
  if (kw.length > 26) kw = kw.slice(0, 26);
  const money = text.match(/(\d+(?:\.\d+)?)\s*(欧元?|€|元|块|美元|刀|\$|镑|£|日元)/);
  let currency = settings.displayCurrency;
  let priceMax: number | null = null;
  if (money) {
    priceMax = Number(money[1]);
    const u = money[2] ?? "";
    currency = /欧|€/.test(u) ? "EUR" : /美元|刀|\$/.test(u) ? "USD" : /镑|£/.test(u) ? "GBP" : /日元/.test(u) ? "JPY" : "CNY";
  }
  const wantIntl = /亚马逊|amazon|德国|美国|英国|日本|ebay|idealo|欧/i.test(text);
  const wantCn = /京东|淘宝|拼多多|闲鱼|小红书|抖音|国内|国行/.test(text);
  let ids = platforms.filter((p) => p.enabled).map((p) => p.id);
  if (/德国|德亚|\.de\b/i.test(text)) ids = ids.filter((id) => !/^(amazon|ebay)_/.test(id) || id.endsWith("_de"));
  if (/亚马逊|amazon/i.test(text) && /ebay/i.test(text)) ids = ids.filter((id) => /^(amazon|ebay)/.test(id) || P(id)?.kind === "compare" || P(id)?.kind === "c2c");
  if (wantIntl && !wantCn) ids = ids.filter((id) => P(id)?.region === "intl");
  else if (wantCn && !wantIntl) ids = ids.filter((id) => P(id)?.region === "cn");
  ids = ids.slice(0, 7);
  const exclude = /不要二手|排除二手|全新/.test(text) ? ["二手", "翻新", "gebraucht", "refurbished"] : [];
  const brand = /sony|索尼/i.test(kw) ? "Sony" : /apple|苹果|iphone|airpods/i.test(kw) ? "Apple" : /dyson|戴森/i.test(kw) ? "Dyson" : /switch|任天堂/i.test(kw) ? "Nintendo" : null;
  return {
    instruction: text || kw,
    goal: /送|礼物/.test(text) ? "gift" : /监控|盯|追踪/.test(text) ? "track" : /货源|批发|进货/.test(text) ? "source" : "compare",
    product: { category: brand ? "数码电子" : "综合", brand, model: brand ? kw : null, attributes: [], mustHave: [kw.split(" ")[0] ?? kw], mustNot: exclude.length ? ["配件", "保护壳"] : ["配件"] },
    keywords: [kw],
    exclude,
    priceMin: null,
    priceMax,
    currency,
    condition: exclude.length ? "new" : "any",
    platforms: ids,
    queries: ids.map((id) => {
      const p = P(id)!;
      const de = p.locale.startsWith("de");
      return { platformId: id, q: p.region === "cn" ? kw : de && exclude.length ? `${kw} neu` : kw, lang: p.locale.slice(0, 2) };
    }),
    sort: /最便宜|便宜|低价/.test(text) ? "price_asc" : "relevance",
    limit: settings.search.resultLimit,
    monitor: /监控|每天|每小时|提醒/.test(text) ? { intervalMinutes: 360, targetPrice: priceMax } : null,
    notes: [
      `识别目标商品「${kw}」${brand ? `（品牌 ${brand}）` : ""}`,
      priceMax ? `预算上限 ${priceMax} ${currency}，其他币种按 ECB 汇率换算` : "未设定预算，按匹配度排序",
      exclude.length ? "排除二手、翻新与配件" : "配件与周边会被标记为「不符」并折叠",
      `为 ${ids.length} 个平台生成本地化搜索词`,
    ],
    parser: "ai+rules",
  };
}

const VARIANTS: { suffix: string; suffixDe: string; verdict: Verdict; score: number; flags: string[]; reasons: string[]; mult: number; cond: "new" | "used" | "unknown" }[] = [
  { suffix: " 官方正品 全新国行", suffixDe: " – Neu & OVP", verdict: "exact", score: 96, flags: [], reasons: ["品牌与型号完全一致", "全新正品"], mult: 1, cond: "new" },
  { suffix: " 旗舰店 官方标配", suffixDe: " Kopfhörer, Schwarz", verdict: "exact", score: 92, flags: [], reasons: ["型号一致", "官方渠道"], mult: 1.04, cond: "new" },
  { suffix: " 限定配色 套装版", suffixDe: " Bundle mit Tasche", verdict: "close", score: 81, flags: ["bundle"], reasons: ["同款，附赠收纳包", "价格含套装"], mult: 1.12, cond: "new" },
  { suffix: " 午夜蓝 新色", suffixDe: " Mitternachtsblau", verdict: "close", score: 78, flags: [], reasons: ["同款不同颜色"], mult: 1.02, cond: "new" },
  { suffix: " 95新 二手 无拆修", suffixDe: " gebraucht, sehr gut", verdict: "related", score: 58, flags: ["used"], reasons: ["二手商品", "成色描述为 95 新"], mult: 0.66, cond: "used" },
  { suffix: " 官翻机 保修一年", suffixDe: " generalüberholt", verdict: "related", score: 55, flags: ["refurbished"], reasons: ["官方翻新"], mult: 0.78, cond: "used" },
  { suffix: " 上一代 清仓", suffixDe: " Vorgängermodell", verdict: "related", score: 49, flags: ["different_model"], reasons: ["上一代型号，非目标款"], mult: 0.72, cond: "new" },
  { suffix: " 专用保护壳 收纳包", suffixDe: " Hülle Tasche", verdict: "mismatch", score: 12, flags: ["accessory"], reasons: ["配件，不是商品本身"], mult: 0.06, cond: "new" },
  { suffix: " 替换耳罩 海绵套", suffixDe: " Ersatz-Ohrpolster", verdict: "mismatch", score: 8, flags: ["accessory"], reasons: ["配件：替换耳罩"], mult: 0.04, cond: "new" },
];

function makeResults(runId: string, plan: SearchPlan, okPlatforms: string[]): ResultItem[] {
  const kw = plan.keywords[0] || "商品";
  const seed = hash(kw);
  const r = rng(seed ^ hash(runId));
  const baseCny = 300 + (seed % 2600);
  const out: ResultItem[] = [];
  okPlatforms.forEach((pid, pi) => {
    const p = P(pid)!;
    const n = 2 + Math.floor(r() * 3);
    for (let k = 0; k < n; k++) {
      const v = VARIANTS[(k * 3 + pi + Math.floor(r() * 3)) % VARIANTS.length]!;
      const regional = p.region === "cn" ? 0.92 + r() * 0.12 : 1.05 + r() * 0.2;
      const priceCny = baseCny * v.mult * regional;
      const price = round(fxConvert(priceCny, "CNY", p.currency), p.currency);
      const id = `${pid}:${(seed + pi * 31 + k * 7).toString(36)}`;
      const de = p.locale.startsWith("de");
      const en = p.region === "intl" && !de;
      const title = p.region === "cn" ? `${kw}${v.suffix}` : de ? `${kw}${v.suffixDe}` : en ? `${kw} ${v.verdict === "mismatch" ? "Case / Ear Pads Accessory" : v.cond === "used" ? "Used - Very Good" : "Wireless, Black"}` : `${kw}${v.suffix}`;
      const hist = walk(hash(id), price, 60, p.currency, 24);
      const item: ResultItem = {
        id, platformId: pid, externalId: id.split(":")[1] ?? id, title, url: p.home + "item/" + (id.split(":")[1] ?? ""), image: productImage(hash(id), k),
        price, currency: p.currency, displayPrice: round(fxConvert(price, p.currency, settings.displayCurrency), settings.displayCurrency),
        originalPrice: r() < 0.35 ? round(price * 1.18, p.currency) : null,
        sales: p.kind === "social" ? null : Math.floor(r() * 9000), rating: p.kind === "social" ? null : Math.round((4 + r()) * 10) / 10,
        reviews: Math.floor(r() * 4000), shop: p.region === "cn" ? ["官方旗舰店", "数码专营店", "自营", "个人卖家"][k % 4] : ["Amazon", "MediaMarkt", "TechStore DE", "Privat"][k % 4],
        location: p.region === "cn" ? ["广东 深圳", "浙江 杭州", "上海"][k % 3] : null, condition: v.cond,
        match: { score: Math.max(3, Math.min(99, v.score + Math.round((r() - 0.5) * 8))), verdict: v.verdict, reasons: v.reasons, flags: v.flags, aiChecked: true },
        clusterId: v.verdict === "exact" ? `c-${seed}` : null, alsoOn: [], watched: false, rank: 0,
        firstSeenAt: hist[0]!.t, lastSeenAt: iso(), spark: hist.slice(-16).map((x) => x.price),
      };
      listings.set(id, { item, history: hist });
      out.push(item);
    }
  });
  // 同款聚合
  const cluster = out.filter((x) => x.clusterId);
  for (const it of cluster) {
    it.alsoOn = cluster.filter((o) => o.id !== it.id && o.platformId !== it.platformId).slice(0, 4).map((o) => ({ listingId: o.id, platformId: o.platformId, price: o.price, currency: o.currency, url: o.url }));
  }
  const order: Record<Verdict, number> = { exact: 0, close: 1, related: 2, mismatch: 3 };
  out.sort((a, b) => order[a.match.verdict] - order[b.match.verdict] || (plan.sort === "price_asc" ? (a.displayPrice ?? 0) - (b.displayPrice ?? 0) : b.match.score - a.match.score));
  out.forEach((x, i) => (x.rank = i + 1));
  return out;
}

function makeSummary(plan: SearchPlan, results: ResultItem[]): AiSummary {
  const good = results.filter((x) => x.match.verdict === "exact" || x.match.verdict === "close");
  const sorted = [...good].sort((a, b) => (a.displayPrice ?? 1e12) - (b.displayPrice ?? 1e12));
  const best = sorted[0];
  const max = sorted[sorted.length - 1];
  const cur = settings.displayCurrency;
  const fmt = (v?: number | null) => (v == null ? "—" : new Intl.NumberFormat("zh-CN", { style: "currency", currency: cur, maximumFractionDigits: 0 }).format(v));
  const kw = plan.keywords[0];
  return {
    headline: best ? `「${kw}」全新正品最低约 ${fmt(best.displayPrice)}，出现在 ${P(best.platformId)?.name}${P(best.platformId)?.region === "intl" ? " · " + P(best.platformId)?.edition : ""}` : `没有找到与「${kw}」精确匹配的商品`,
    bestPick: best ? { id: best.id, why: `型号完全一致、全新、${best.shop ?? "店铺"}发货，换算后比中位价低 ${Math.max(3, Math.round(((max?.displayPrice ?? 0) - (best.displayPrice ?? 0)) / Math.max(1, max?.displayPrice ?? 1) * 100))}%` } : null,
    bullets: [
      `共 ${results.length} 条结果，精确匹配 ${results.filter((x) => x.match.verdict === "exact").length} 条，接近 ${results.filter((x) => x.match.verdict === "close").length} 条`,
      `价格区间 ${fmt(best?.displayPrice)} – ${fmt(max?.displayPrice)}（已换算为${cur}）`,
      "国际站价格含当地增值税，跨境购买需另计运费与进口税",
    ],
    priceInsight: "近 30 天该款在多数平台呈小幅下降趋势，国内平台价格普遍低于欧洲站 8%–15%。",
    caveats: ["二手与官翻已标记为「相关」，未计入最低价", "配件 2 条已折叠到「可能不相关」"],
    nextActions: [
      ...(best ? [{ type: "watch" as const, label: "追踪最低价这款", listingId: best.id, targetPrice: best.price ? round(best.price * 0.93, best.currency) : null }] : []),
      { type: "search" as const, label: "只看国内平台", instruction: `${kw} 国内平台 全新` },
      { type: "navigate" as const, label: "看价格走势", route: "/insights/trends" },
    ],
  };
}

function startRun(instruction: string, plan: SearchPlan | null, trigger: "manual" | "monitor" = "manual"): { runId: string; plan: SearchPlan } {
  const runId = uid();
  const finalPlan = plan ?? mockPlan(instruction);
  const run: Run = {
    summary: { id: runId, instruction: finalPlan.instruction || instruction, status: "running", trigger, startedAt: iso(), finishedAt: null, fetched: 0, kept: 0, platforms: finalPlan.platforms, bestPrice: null, currency: settings.displayCurrency, error: null },
    plan: finalPlan, results: [], aiSummary: null, stats: null, events: [], seq: 0, timers: [],
  };
  runs.set(runId, run);
  const push = (body: RunEventBody) => {
    if (run.summary.status !== "running" && body.type !== "cancelled") return;
    const ev = { ...body, seq: ++run.seq, t: Date.now() } as RunEvent;
    run.events.push(ev);
    emit("run://event", { runId, event: ev });
  };
  const at = (ms: number, fn: () => void) => run.timers.push(window.setTimeout(fn, ms));
  const pids = finalPlan.platforms;
  const blocked = pids.filter((id) => {
    const st = logins.find((l) => l.platformId === id);
    return P(id)?.login === "required" && (st?.status === "logged_out" || st?.status === "expired");
  });
  const ok = pids.filter((id) => !blocked.includes(id));
  const stats: PlatformStat[] = [];
  const t0 = Date.now();

  at(60, () => push({ type: "stage", stage: "plan", label: "理解指令", progress: 0.05 }));
  at(520, () => push({ type: "plan", plan: finalPlan }));
  at(760, () => push({ type: "stage", stage: "search", label: "多平台并行搜索", progress: 0.12 }));
  pids.forEach((id, i) => at(820 + i * 90, () => push({ type: "platform", platformId: id, state: "queued" })));
  pids.forEach((id, i) => at(1100 + i * 260, () => push({ type: "platform", platformId: id, state: "searching", message: "打开搜索页…" })));
  let lastDone = 1100;
  let fetched = 0;
  pids.forEach((id, i) => {
    const doneAt = 2100 + i * 420 + (hash(id) % 600);
    lastDone = Math.max(lastDone, doneAt);
    if (blocked.includes(id)) {
      at(doneAt - 300, () => {
        push({ type: "platform", platformId: id, state: "login_required", message: `${P(id)?.name} 需要登录（只需一次）`, ms: doneAt - 1100 });
        stats.push({ platformId: id, state: "login_required", count: 0, ms: doneAt - 1100, message: "需要登录" });
      });
    } else {
      const count = 14 + (hash(id + runId) % 24);
      fetched += count;
      if (i === 1) at(doneAt - 900, () => push({ type: "platform", platformId: id, state: "searching", message: "滚动加载第 2 屏…" }));
      at(doneAt, () => {
        push({ type: "platform", platformId: id, state: "done", count, ms: doneAt - 1100 - i * 260 });
        stats.push({ platformId: id, state: "done", count, ms: doneAt - 1100, message: null });
      });
    }
  });
  const T = lastDone + 250;
  const results = makeResults(runId, finalPlan, ok);
  const visible = results.filter((x) => x.match.verdict !== "mismatch");
  const hidden = results.length - visible.length;
  at(T, () => push({ type: "stage", stage: "extract", label: "抽取商品信息", progress: 0.55 }));
  at(T + 420, () => push({ type: "filter", fetched, kept: results.length + 3, dropped: { exclude: 4, price: 6, duplicate: 3 }, duplicates: 3 }));
  at(T + 650, () => push({ type: "stage", stage: "verify", label: "AI 校验匹配度", progress: 0.68 }));
  at(T + 1500, () => push({ type: "verify", checked: results.length, matched: visible.length, hidden }));
  at(T + 1650, () => push({ type: "stage", stage: "rank", label: "同款聚合与排序", progress: 0.82 }));
  at(T + 1950, () => {
    run.results = results;
    push({ type: "results", items: results });
  });
  at(T + 2100, () => push({ type: "stage", stage: "summarize", label: "AI 总结", progress: 0.92 }));
  at(T + 2900, () => {
    run.aiSummary = makeSummary(finalPlan, results);
    push({ type: "summary", summary: run.aiSummary });
  });
  at(T + 3100, () => {
    const best = visible.filter((x) => x.match.verdict === "exact").sort((a, b) => (a.displayPrice ?? 0) - (b.displayPrice ?? 0))[0];
    run.stats = { fetched, kept: visible.length, hidden, duplicates: 3, dropped: { exclude: 4, price: 6 }, elapsedMs: Date.now() - t0, platforms: stats, aiCalls: 4 };
    push({ type: "stage", stage: "done", label: "完成", progress: 1 });
    push({ type: "done", stats: run.stats });
    Object.assign(run.summary, { status: "done", finishedAt: iso(), fetched, kept: visible.length, bestPrice: best?.displayPrice ?? null });
  });
  return { runId, plan: finalPlan };
}

function cancelRun(runId: string) {
  const run = runs.get(runId);
  if (!run || run.summary.status !== "running") return;
  run.timers.forEach((t) => clearTimeout(t));
  run.summary.status = "cancelled";
  run.summary.finishedAt = iso();
  const ev = { type: "cancelled", seq: ++run.seq, t: Date.now() } as RunEvent;
  run.events.push(ev);
  emit("run://event", { runId, event: ev });
}

/* 预置历史运行 */
function seedHistory() {
  const samples: [string, number, RunSummary["status"]][] = [
    ["Sony WH-1000XM5 在亚马逊德国和 eBay 200 欧以内，不要二手", 26, "done"],
    ["Switch 2 马力欧套装 京东 拼多多 最便宜", 30, "done"],
    ["给喜欢露营的爸爸挑 500 元以内的礼物", 50, "done"],
    ["Dyson V15 Detect idealo 最低价", 74, "interrupted"],
    ["露营灯 1688 货源 批发价", 98, "done"],
    ["iPhone 17 Pro 256G 国行 vs 日版价格", 140, "error"],
  ];
  for (const [ins, hoursAgo, status] of samples) {
    const plan = mockPlan(ins);
    const runId = uid();
    const ok = plan.platforms.filter((_, i) => i !== 1);
    const results = status === "done" ? makeResults(runId, plan, ok) : [];
    const vis = results.filter((x) => x.match.verdict !== "mismatch");
    const t = Date.now() - hoursAgo * 3600_000;
    runs.set(runId, {
      summary: { id: runId, instruction: ins, status, trigger: hoursAgo === 98 ? "monitor" : "manual", startedAt: iso(t), finishedAt: iso(t + 47_000), fetched: results.length * 3, kept: vis.length, platforms: plan.platforms, bestPrice: vis[0]?.displayPrice ?? null, currency: settings.displayCurrency, error: status === "error" ? "AI 校验失败：HTTP 429 请求过于频繁" : status === "interrupted" ? "应用在运行中被关闭" : null },
      plan, results, aiSummary: status === "done" ? makeSummary(plan, results) : null,
      stats: status === "done" ? { fetched: results.length * 3, kept: vis.length, hidden: results.length - vis.length, duplicates: 2, dropped: { price: 3 }, elapsedMs: 47_000, platforms: ok.map((id) => ({ platformId: id, state: "done", count: 12, ms: 9000, message: null })), aiCalls: 4 } : null,
      events: [], seq: 0, timers: [],
    });
  }
}

/* ================================================================ 追踪 */
const WATCH_SEED: [string, string, number, number | null, string][] = [
  ["amazon_de", "Sony WH-1000XM5 Kabellose Kopfhörer mit Noise Cancelling, Schwarz", 249, 259, ""],
  ["jd", "任天堂 Switch 2 马力欧赛车世界 同捆套装 国行", 2907, 2799, "等 618"],
  ["temu", "LED 露营灯 USB-C 充电 三档调光 防水", 13.9, null, "选品对标"],
  ["idealo", "Dyson V15 Detect Absolute kabelloser Staubsauger", 489, 459, ""],
  ["taobao", "Apple AirPods Pro 2 USB-C 官方正品", 1399, 1299, ""],
  ["joybuy", "Xiaomi Robot Vacuum S20+ Saugroboter", 399, null, ""],
  ["amazon_co_jp", "Nintendo Switch 2 マリオカート ワールド セット", 53980, 49800, "日版对比"],
  ["alibaba1688", "户外露营灯 便携式 LED 帐篷灯 厂家直销", 23.8, 20, "进货价"],
];
const watches: WatchItem[] = [];
const watchHist = new Map<number, PricePoint[]>();

function refreshWatch(w: WatchItem) {
  const h = watchHist.get(w.id) ?? [];
  const prices = h.map((x) => x.price);
  w.lastPrice = prices[prices.length - 1] ?? w.lastPrice;
  w.firstPrice = prices[0] ?? w.firstPrice;
  w.lowest = prices.length ? Math.min(...prices) : null;
  w.highest = prices.length ? Math.max(...prices) : null;
  w.changePct = w.firstPrice && w.lastPrice ? ((w.lastPrice - w.firstPrice) / w.firstPrice) * 100 : null;
  w.spark = prices.slice(-24);
}

function seedWatches() {
  WATCH_SEED.forEach(([pid, title, price, target, note], i) => {
    const p = P(pid)!;
    const id = i + 1;
    const hist = walk(hash(title), price, 90 - i * 6, p.currency, 12);
    watchHist.set(id, hist);
    const w: WatchItem = {
      id, listingId: null, platformId: pid, title, url: p.home + "dp/" + hash(title).toString(36), image: productImage(hash(title), i),
      currency: p.currency, targetPrice: target, lastPrice: price, firstPrice: null, lowest: null, highest: null, changePct: null,
      intervalMinutes: [360, 180, 720, 360, 360, 1440, 720, 1440][i] ?? 360, enabled: i !== 5, status: i === 5 ? "error" : "ok",
      lastError: i === 5 ? "连续 3 次没有在商品页找到价格" : null, lastCheckedAt: iso(Date.now() - (i + 1) * 47 * 60_000),
      nextCheckAt: iso(Date.now() + (i + 1) * 53 * 60_000), note, createdAt: hist[0]!.t, spark: [],
    };
    refreshWatch(w);
    watches.push(w);
  });
}

/* ================================================================ 监控 */
const monitors: Monitor[] = [];
function seedMonitors() {
  const rows: [string, string, number, number | null, Monitor["lastStatus"], number, number][] = [
    ["蓝牙耳机 欧洲价格带", "蓝牙降噪耳机 亚马逊德国 eBay idealo 价格分布", 720, null, "ok", 3, 42],
    ["露营灯 选品监控", "露营灯 1688 Temu AliExpress 新品与最低价", 360, 20, "ok", 7, 18],
    ["Switch 2 全网最低价", "Switch 2 马力欧套装 京东 拼多多 淘宝 亚马逊日本", 180, 2799, "running", 0, 66],
    ["iPhone 17 Pro 国行价格", "iPhone 17 Pro 256G 国行 京东 拼多多 百亿补贴", 1440, 7499, "error", 0, 9],
  ];
  rows.forEach(([name, ins, iv, target, st, n, cnt], i) => {
    monitors.push({
      id: i + 1, name, instruction: ins, plan: mockPlan(ins), intervalMinutes: iv, enabled: i !== 3, targetPrice: target,
      lastRunAt: iso(Date.now() - (i + 1) * 2.3 * 3600_000), nextRunAt: iso(Date.now() + (i + 1) * 1.7 * 3600_000), lastStatus: st,
      lastNewCount: n, runCount: cnt, lastRunId: null, bestPrice: [86, 21.5, 2907, 7599][i], currency: settings.displayCurrency,
      createdAt: iso(Date.now() - (30 + i * 9) * 86400_000),
    });
  });
}

/* ================================================================ Copilot */
const copilotSessions = new Map<string, CopilotMessage[]>();

function copilotReply(text: string, skill?: string): { content: string; actions: CopilotAction[] } {
  const s = skill || (/礼物|送/.test(text) ? "gift" : /文案|标题/.test(text) ? "copy" : /行业|品类|格局/.test(text) ? "industry" : /进货|货源|选品/.test(text) ? "sourcing" : /值得|入手|等等/.test(text) ? "timing" : "compare");
  switch (s) {
    case "gift":
      return {
        content: "根据你的描述，我给出 **3 个方向**，都控制在预算内：\n\n1. **轻量露营灯 + 挂钩套装**（¥120–260）：实用、几乎每次露营都会用到，选 USB-C 充电、带红光模式的款。\n2. **钛合金露营杯 / 咖啡套装**（¥180–420）：有仪式感，适合喜欢在营地煮咖啡的人。\n3. **便携户外电源 300Wh 级**（¥450–499 档位较少，注意促销）：给手机、灯、风扇供电，体验升级明显。\n\n> 建议优先看京东自营和天猫旗舰店，送礼开票、售后更省心。",
        actions: [
          { type: "search", label: "搜：露营灯 礼盒 500元以内", instruction: "露营灯 礼盒 500元以内 京东 淘宝" },
          { type: "search", label: "搜：钛合金露营咖啡套装", instruction: "钛合金露营咖啡杯套装 500元以内" },
          { type: "search", label: "搜：300Wh 户外电源", instruction: "户外电源 300Wh 500元左右 全新" },
        ],
      };
    case "copy":
      return {
        content: "**小红书标题**\n- 露营党狂喜｜40000mAh 户外电源，一晚灯+风扇+咖啡机都够用\n- 自驾必备！这台户外电源让我告别找插座\n\n**Amazon.de 标题（德语）**\n- Powerstation 40000mAh, tragbarer Solargenerator mit USB-C PD 100W, für Camping & Notfall\n\n**卖点要点**\n- 容量、输出功率、充电速度用具体数字表达\n- 避免「最强」「第一」等极限词（广告法 / UWG）",
        actions: [{ type: "navigate", label: "去追踪竞品价格", route: "/track/watchlist" }],
      };
    case "industry":
      return {
        content: "**欧洲蓝牙耳机价格带（近 30 天，Amazon.de / eBay.de / idealo）**\n\n| 价格带 | 占比 | 代表品牌 |\n|---|---|---|\n| €20–50 | 38% | Soundcore、JLab |\n| €50–150 | 34% | Sony、JBL、Jabra |\n| €150–300 | 22% | Sony XM5、Bose QC |\n| €300+ | 6% | Apple、Sennheiser |\n\n**判断**：中端价位竞争最激烈，降噪成为 €80 以上的标配；国内供应链产品在 €30–60 区间价格优势明显。",
        actions: [{ type: "navigate", label: "打开市场分析", route: "/insights/market" }, { type: "search", label: "搜：降噪耳机 50-150 欧", instruction: "降噪蓝牙耳机 50-150欧 亚马逊德国" }],
      };
    case "sourcing":
      return {
        content: "按当前数据估算：1688 同类露营灯进货价 **¥18–26**，Temu 售价 **€9.9–13.9**，Amazon.de 售价 **€19–29**。\n\n- 扣除头程、FBA 与平台佣金后，Amazon.de 毛利约 **35%–45%**\n- Temu 渠道价格已被压低，更适合走量\n\n建议先追踪 Amazon.de 前 10 名的价格 2 周，确认价格稳定再下单。",
        actions: [{ type: "search", label: "搜：露营灯 Amazon.de 销量榜", instruction: "LED Campinglampe wiederaufladbar 亚马逊德国 按销量" }, { type: "navigate", label: "查看露营灯监控", route: "/track/monitors" }],
      };
    case "timing":
      return {
        content: "你追踪的 **Switch 2 马力欧套装** 近 90 天价格从 ¥3,199 降到 ¥2,907（-9.1%），最近两周走平。\n\n- 历史最低 ¥2,869（出现在 1 次大促）\n- 距离你的目标价 ¥2,799 还差 3.7%\n\n**建议**：不急的话等 11.11，大概率会触达目标价；急用的话现在入手也只比历史最低高 1.3%。",
        actions: [{ type: "navigate", label: "看价格走势", route: "/insights/trends" }],
      };
    default:
      return {
        content: "我对比了同款在各平台的**到手价**（已换算为人民币）：\n\n- 京东自营 ¥1,999，次日达，开票方便\n- Amazon.de €249 ≈ ¥1,947，但需另计国际运费和进口税，实际高约 15%\n- 拼多多百亿补贴 ¥1,859 最低，注意确认是否为国行正品\n\n**结论**：在国内买更划算；在德国使用的话 Amazon.de 售后更省心。",
        actions: [{ type: "search", label: "重新比价：国内平台", instruction: `${text.slice(0, 20)} 京东 拼多多 淘宝 全新` }],
      };
  }
}

/* ================================================================ 洞察 */
function trendsFor(watchIds: number[], monitorIds: number[], days: number): TrendSeries[] {
  const out: TrendSeries[] = [];
  const since = Date.now() - days * 86400_000;
  for (const id of watchIds) {
    const w = watches.find((x) => x.id === id);
    if (!w) continue;
    const pts = (watchHist.get(id) ?? []).filter((x) => new Date(x.t).getTime() >= since);
    const byDay = new Map<string, number[]>();
    pts.forEach((x) => {
      const d = x.t.slice(0, 10);
      byDay.set(d, [...(byDay.get(d) ?? []), x.price]);
    });
    out.push({
      key: `w${id}`, label: w.title.slice(0, 22), kind: "watch", currency: w.currency,
      points: [...byDay.entries()].map(([d, xs]) => {
        const s = [...xs].sort((a, b) => a - b);
        return { t: d + "T12:00:00Z", min: s[0]!, median: s[Math.floor(s.length / 2)]!, max: s[s.length - 1]!, count: s.length };
      }),
    });
  }
  for (const id of monitorIds) {
    const m = monitors.find((x) => x.id === id);
    if (!m) continue;
    const r = rng(hash(m.name));
    const base = m.bestPrice ?? 100;
    const pts = [];
    let v = base * 1.15;
    for (let d = days; d >= 0; d--) {
      v *= 0.99 + r() * 0.018;
      pts.push({ t: iso(Date.now() - d * 86400_000), min: round(v * 0.82, m.currency), median: round(v, m.currency), max: round(v * 1.4, m.currency), count: 10 + Math.floor(r() * 30) });
    }
    out.push({ key: `m${id}`, label: m.name, kind: "monitor", currency: m.currency, points: pts });
  }
  return out;
}

function overview(days: number): MarketOverview {
  const cur = settings.displayCurrency;
  const r = rng(days * 97);
  const med: [string, number][] = [["jd", 1999], ["pinduoduo", 1859], ["taobao", 1929], ["amazon_de", 1947], ["ebay_de", 1712], ["idealo", 1890], ["temu", 1488], ["aliexpress", 1560]];
  return {
    days, currency: cur,
    kpis: { tracked: watches.length, drops: 5, monitors: monitors.length, runs: 37 + days, listings: 1284 + days * 11, avgSpreadPct: 14.6 },
    platformMedians: med.map(([platformId, m]) => ({ platformId, median: m, count: 20 + Math.floor(r() * 120) })),
    priceBands: [["< ¥100", 128], ["¥100–300", 264], ["¥300–800", 341], ["¥800–1500", 212], ["¥1500–3000", 186], ["> ¥3000", 73]].map(([label, count]) => ({ label: label as string, count: count as number })),
    platformShare: [["taobao", 312], ["jd", 268], ["amazon_de", 214], ["pinduoduo", 180], ["ebay_de", 121], ["temu", 98], ["idealo", 61], ["xianyu", 30]].map(([platformId, count]) => ({ platformId: platformId as string, count: count as number })),
    topDrops: watches.filter((w) => (w.changePct ?? 0) < 0).slice(0, 5).map((w) => ({ watchId: w.id, title: w.title, platformId: w.platformId, from: w.firstPrice ?? 0, to: w.lastPrice ?? 0, pct: w.changePct ?? 0, currency: w.currency })),
    daily: Array.from({ length: Math.min(days, 30) }, (_, i) => ({ day: iso(Date.now() - (Math.min(days, 30) - 1 - i) * 86400_000).slice(0, 10), runs: Math.floor(r() * 6), listings: 20 + Math.floor(r() * 80) })),
  };
}

const REPORT = `## 市场概览

过去 30 天共运行搜索 **67 次**，记录商品 **1,614 条**，覆盖 **8 个平台**。整体价格较上月 **下降 3.4%**，主要来自国内大促与欧洲站返校季清仓。

## 平台价差

- **国内平台**中位价普遍低于欧洲站 **8%–15%**（已按 ECB 汇率换算）
- **Temu / AliExpress** 在低价带（< ¥300）占比最高，但精确匹配率只有 61%，配件与相似款较多
- **idealo** 汇总的商家报价离散度最大，适合找最低价，需留意商家评分

## 价格带与机会

1. ¥300–800 价格带商品最多（27%），竞争激烈
2. ¥1500–3000 价格带降价最明显（-6.8%），适合消费者入手
3. 露营灯类目进货价稳定在 ¥18–26，零售端 Amazon.de 仍有 35% 以上毛利空间

## 建议

- 消费者：Switch 2、Dyson V15 已接近历史低位，可在目标价附近入手
- 商户：关注 Temu 的低价冲击，Amazon.de 中端价位保持差异化
- 平台分析：建议把监控频率提高到 6 小时以捕捉周末促销
`;

/* ================================================================ 初始化 */
seedHistory();
seedWatches();
seedMonitors();
const sessionMain: CopilotMessage[] = [];
copilotSessions.set("main", sessionMain);

let lastUsage = { calls: 23, promptTokens: 48210, completionTokens: 9214 };

function patchSettings(patch: AppSettingsPatch) {
  const { search, notify, ...rest } = patch;
  Object.assign(settings, rest);
  if (search) Object.assign(settings.search, search);
  if (notify) Object.assign(settings.notify, notify);
}

function setLogin(id: string, upd: Partial<LoginState>) {
  const st = logins.find((l) => l.platformId === id);
  if (!st) return null;
  Object.assign(st, upd);
  emit("login://state", { ...st });
  return { ...st };
}
const loginTimers = new Map<string, number[]>();
function clearLoginTimers(id: string) {
  (loginTimers.get(id) ?? []).forEach((t) => clearTimeout(t));
  loginTimers.set(id, []);
}
function loginSuccess(id: string) {
  const p = P(id);
  const now = iso();
  const group = platforms.filter((o) => o.accountGroup === p?.accountGroup).map((o) => o.id);
  group.forEach((gid) => setLogin(gid, { status: "logged_in", flow: gid === id ? "success" : "idle", verifiedAt: now, lastWallAt: null, cookieExpires: iso(Date.now() + 30 * 86400_000), message: gid === id ? "已登录并保存会话" : null }));
  addNotice({ kind: "login", level: "success", title: `${p?.name}${p?.region === "intl" ? " · " + p.edition : ""} 已登录`, body: "登录会话只保存在本机，之后搜索会自动使用。", link: "/platforms/accounts" });
}

function addNotice(n: Omit<Notice, "id" | "read" | "createdAt">) {
  const notice: Notice = { ...n, id: ++noticeSeq, read: false, createdAt: iso() };
  notices.unshift(notice);
  emit("notice://new", notice);
}

/* ================================================================ 命令 */
type Handler = (a: Record<string, unknown>) => unknown | Promise<unknown>;

const H: Record<string, Handler> = {
  app_bootstrap: (): Bootstrap => ({
    version: "26.01", dataDir: "C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app", settings: jclone(settings),
    ai: { ...aiStatus }, platforms: jclone(platforms), logins: jclone(logins), unread: notices.filter((n) => !n.read).length,
    runningRuns: [...runs.values()].filter((r) => r.summary.status === "running").map((r) => r.summary.id), fx: FX,
  }),
  settings_update: ({ patch }) => {
    patchSettings(patch as AppSettingsPatch);
    return jclone(settings);
  },
  open_data_dir: () => "C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app",
  data_export: ({ kind }) => `C:\\Users\\steven\\Downloads\\hawwwker-${kind}-${new Date().toISOString().slice(0, 10)}.csv`,
  fx_rates: () => FX,

  ai_get_config: () => ({ ...aiConfig }),
  ai_save_config: ({ input }) => {
    const i = input as AiConfigInput;
    const hasKey = i.apiKey === undefined ? aiConfig.hasKey : i.apiKey !== "";
    aiConfig = {
      ...aiConfig, protocol: i.protocol, provider: i.provider, baseUrl: i.baseUrl, model: i.model, fastModel: i.fastModel ?? "",
      temperature: i.temperature ?? aiConfig.temperature, timeoutSecs: i.timeoutSecs ?? aiConfig.timeoutSecs,
      maxConcurrency: i.maxConcurrency ?? aiConfig.maxConcurrency, hasKey,
      keyHint: i.apiKey ? "••••" + i.apiKey.slice(-4) : hasKey ? aiConfig.keyHint : "",
    };
    aiStatus = { ...aiStatus, configured: !!(aiConfig.baseUrl && aiConfig.model && (aiConfig.hasKey || aiConfig.provider === "ollama")), verified: false, model: aiConfig.model, provider: aiConfig.provider };
    return { ...aiConfig };
  },
  ai_test: async () => {
    await sleep(900 + Math.random() * 600);
    if (!aiConfig.baseUrl || !aiConfig.model) {
      aiStatus = { ...aiStatus, verified: false, lastError: "请填写 Base URL 与模型名称" };
    } else if (!aiConfig.hasKey && aiConfig.provider !== "ollama") {
      aiStatus = { ...aiStatus, verified: false, lastError: "缺少 API Key（HTTP 401 Unauthorized）" };
    } else {
      aiStatus = { configured: true, verified: true, model: aiConfig.model, provider: aiConfig.provider, verifiedAt: iso(), latencyMs: 600 + Math.floor(Math.random() * 700), lastError: null, sample: "你好！我是你的购物搜索助手，已准备好帮你找货、比价和盯价。" };
    }
    return { ...aiStatus };
  },
  ai_presets: () => PRESETS,
  ai_usage: () => ({
    today: lastUsage, total: { calls: 1288, promptTokens: 2_641_200, completionTokens: 512_840 },
    days: Array.from({ length: 14 }, (_, i) => ({ day: iso(Date.now() - (13 - i) * 86400_000).slice(0, 10), calls: 20 + ((i * 37) % 60), tokens: 30000 + ((i * 7919) % 50000) })),
  }),

  platforms_list: () => jclone(platforms),
  platform_update: ({ id, patch }) => {
    const p = P(id as string);
    if (!p) throw err("NOT_FOUND", "平台不存在");
    const pa = patch as PlatformPatch;
    if (pa.enabled !== undefined) p.enabled = pa.enabled;
    if (pa.searchUrl !== undefined) p.searchUrl = pa.searchUrl === "" ? p.defaultSearchUrl : pa.searchUrl;
    return { ...p };
  },
  platform_add_custom: ({ input }) => {
    const i = input as CustomPlatformInput;
    if (!i.searchUrl.includes("{q}")) throw err("INVALID", "搜索地址必须包含 {q}");
    const id = "custom_" + hash(i.name).toString(36);
    const p: PlatformInfo = {
      id, family: id, familyName: i.name, name: i.name, edition: "自定义", region: i.region, country: i.region === "cn" ? "CN" : "GL",
      kind: i.kind ?? "retail", currency: i.currency, locale: i.locale, color: "#5b6cff", glyph: i.name.slice(0, 1).toUpperCase(),
      home: i.home || new URL(i.searchUrl.replace("{q}", "x")).origin, searchUrl: i.searchUrl, defaultSearchUrl: i.searchUrl,
      accountGroup: id, login: "optional", beta: true, enabled: true, custom: true, note: "自定义平台",
    };
    platforms.push(p);
    logins.push({ platformId: id, status: "not_needed", flow: "idle", sharedWith: [] });
    return p;
  },
  platform_remove_custom: ({ id }) => {
    const i = platforms.findIndex((p) => p.id === id && p.custom);
    if (i >= 0) platforms.splice(i, 1);
  },
  login_states: async ({ refresh }) => {
    if (refresh) await sleep(700);
    return jclone(logins);
  },
  login_start: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    const st = setLogin(id, { flow: "opening", message: "正在打开官网登录页…" });
    const ts = loginTimers.get(id)!;
    ts.push(window.setTimeout(() => setLogin(id, { flow: "waiting", message: "请在弹出的浏览器窗口中登录，完成后会自动识别" }), 700));
    ts.push(window.setTimeout(() => setLogin(id, { flow: "verifying", message: "检测到登录 Cookie，正在验证…" }), 5200));
    ts.push(window.setTimeout(() => loginSuccess(id), 6400));
    return st;
  },
  login_confirm: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    const st = setLogin(id, { flow: "verifying", message: "正在验证登录状态…" });
    loginTimers.get(id)!.push(window.setTimeout(() => loginSuccess(id), 1100));
    return st;
  },
  login_cancel: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    return setLogin(id, { flow: "cancelled", message: "已取消登录（窗口已关闭）" });
  },
  logout: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    const p = P(id);
    platforms.filter((o) => o.accountGroup === p?.accountGroup && o.id !== id).forEach((o) => setLogin(o.id, { status: "logged_out", flow: "idle", verifiedAt: null, message: null }));
    return setLogin(id, { status: "logged_out", flow: "idle", verifiedAt: null, cookieExpires: null, lastWallAt: null, message: "已退出，本机会话已删除" });
  },
  browser_info: () => ({ executable: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", running: true, profileDir: "C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app\\browser-profile" }),
  browser_show: () => undefined,
  browser_wipe: async () => {
    await sleep(600);
    logins.forEach((l) => {
      if (l.status !== "not_needed") setLogin(l.platformId, { status: "logged_out", flow: "idle", verifiedAt: null, cookieExpires: null, lastWallAt: null, message: null });
    });
  },

  search_plan: async ({ instruction }) => {
    await sleep(1100);
    return mockPlan(String(instruction ?? ""));
  },
  search_start: async ({ instruction, plan }) => {
    if (!aiStatus.verified) throw err("AI_REQUIRED", "请先连接并测试 AI 模型");
    await sleep(120);
    return startRun(String(instruction ?? ""), (plan as SearchPlan | null) ?? null);
  },
  search_cancel: ({ runId }) => cancelRun(runId as string),
  search_events: ({ runId, since }) => (runs.get(runId as string)?.events ?? []).filter((e) => e.seq > Number(since ?? 0)),
  search_runs: ({ limit }) => [...runs.values()].map((r) => r.summary).sort((a, b) => b.startedAt.localeCompare(a.startedAt)).slice(0, Number(limit ?? 50)),
  search_run: ({ runId }): RunDetail => {
    const r = runs.get(runId as string);
    if (!r) throw err("NOT_FOUND", "运行不存在或已删除");
    return { summary: { ...r.summary }, plan: r.plan, results: r.results, aiSummary: r.aiSummary, stats: r.stats };
  },
  search_delete_run: ({ runId }) => {
    runs.delete(runId as string);
  },
  listing_get: ({ id }): ListingDetail => {
    const l = listings.get(id as string);
    if (!l) throw err("NOT_FOUND", "商品不存在");
    const w = watches.find((x) => x.listingId === id);
    return { item: { ...l.item, watched: !!w }, history: l.history, alsoOn: l.item.alsoOn, watchId: w?.id ?? null };
  },

  watch_list: () => jclone(watches),
  watch_add: ({ input }) => {
    const i = input as WatchAddInput;
    const l = i.listingId ? listings.get(i.listingId) : undefined;
    if (i.listingId && watches.some((w) => w.listingId === i.listingId)) throw err("INVALID", "这件商品已经在追踪中");
    let pid = i.platformId ?? l?.item.platformId;
    if (!pid && i.url) pid = platforms.find((p) => i.url!.includes(new URL(p.home).hostname.replace("www.", "")))?.id;
    if (!l && !i.url) throw err("INVALID", "请提供商品链接");
    const p = (P(pid ?? "amazon_de") ?? platforms[0])!;
    const id = Math.max(0, ...watches.map((w) => w.id)) + 1;
    const price = l?.item.price ?? round(50 + (hash(i.url ?? "") % 900), p.currency);
    const hist = l?.history ?? [{ t: iso(), price, currency: p.currency, source: "detail" as const }];
    watchHist.set(id, [...hist]);
    const w: WatchItem = {
      id, listingId: i.listingId ?? null, platformId: p.id, title: i.title ?? l?.item.title ?? "新商品（首次读价中…）", url: i.url ?? l?.item.url ?? p.home,
      image: l?.item.image ?? productImage(id * 13, id), currency: l?.item.currency ?? p.currency, targetPrice: i.targetPrice ?? null, lastPrice: price,
      firstPrice: null, lowest: null, highest: null, changePct: null, intervalMinutes: i.intervalMinutes ?? 360, enabled: true, status: l ? "ok" : "new",
      lastError: null, lastCheckedAt: iso(), nextCheckAt: iso(Date.now() + (i.intervalMinutes ?? 360) * 60_000), note: i.note ?? "", createdAt: iso(), spark: [],
    };
    refreshWatch(w);
    watches.unshift(w);
    if (l) l.item.watched = true;
    return { ...w };
  },
  watch_update: ({ id, patch }) => {
    const w = watches.find((x) => x.id === id);
    if (!w) throw err("NOT_FOUND", "追踪项不存在");
    Object.assign(w, patch as WatchPatch);
    return { ...w };
  },
  watch_remove: ({ id }) => {
    const i = watches.findIndex((x) => x.id === id);
    if (i >= 0) {
      const w = watches[i]!;
      if (w.listingId) {
        const l = listings.get(w.listingId);
        if (l) l.item.watched = false;
      }
      watches.splice(i, 1);
    }
  },
  watch_check: ({ id }) => {
    const w = watches.find((x) => x.id === id);
    if (!w) throw err("NOT_FOUND", "追踪项不存在");
    w.status = "checking";
    window.setTimeout(() => {
      const h = watchHist.get(w.id) ?? [];
      const last = w.lastPrice ?? 100;
      const next = round(last * (0.95 + Math.random() * 0.06), w.currency);
      h.push({ t: iso(), price: next, currency: w.currency, source: "detail" });
      watchHist.set(w.id, h);
      refreshWatch(w);
      w.status = "ok";
      w.lastError = null;
      w.lastCheckedAt = iso();
      w.nextCheckAt = iso(Date.now() + w.intervalMinutes * 60_000);
      if (title(w) && next < last) {
        addNotice({ kind: w.targetPrice && next <= w.targetPrice ? "target_hit" : "price_drop", level: "success", title: `${title(w)} 降价 ${(((last - next) / last) * 100).toFixed(1)}%`, body: `${P(w.platformId)?.name} 当前 ${next} ${w.currency}`, link: `/track/watchlist?id=${w.id}` });
      }
      emit("watch://updated", { ...w });
    }, 1600);
    return { ...w };
  },
  watch_history: ({ id, days }) => {
    const since = Date.now() - Number(days ?? 90) * 86400_000;
    return (watchHist.get(Number(id)) ?? []).filter((x) => new Date(x.t).getTime() >= since);
  },

  monitor_list: () => jclone(monitors),
  monitor_create: ({ input }) => {
    const i = input as MonitorCreateInput;
    const m: Monitor = {
      id: Math.max(0, ...monitors.map((x) => x.id)) + 1, name: i.name || i.instruction.slice(0, 16), instruction: i.instruction,
      plan: i.plan ?? mockPlan(i.instruction), intervalMinutes: i.intervalMinutes, enabled: true, targetPrice: i.targetPrice ?? null,
      lastRunAt: null, nextRunAt: iso(Date.now() + i.intervalMinutes * 60_000), lastStatus: "idle", lastNewCount: 0, runCount: 0,
      lastRunId: null, bestPrice: null, currency: settings.displayCurrency, createdAt: iso(),
    };
    monitors.unshift(m);
    return m;
  },
  monitor_update: ({ id, patch }) => {
    const m = monitors.find((x) => x.id === id);
    if (!m) throw err("NOT_FOUND", "监控不存在");
    Object.assign(m, patch as MonitorPatch);
    return { ...m };
  },
  monitor_delete: ({ id }) => {
    const i = monitors.findIndex((x) => x.id === id);
    if (i >= 0) monitors.splice(i, 1);
  },
  monitor_run: ({ id }) => {
    const m = monitors.find((x) => x.id === id);
    if (!m) throw err("NOT_FOUND", "监控不存在");
    const { runId } = startRun(m.instruction, m.plan, "monitor");
    Object.assign(m, { lastStatus: "running", lastRunId: runId, lastRunAt: iso() });
    window.setTimeout(() => {
      Object.assign(m, { lastStatus: "ok", runCount: m.runCount + 1, lastNewCount: 2 + (m.id % 4) });
    }, 9000);
    return { runId };
  },

  notice_list: ({ limit }) => notices.slice(0, Number(limit ?? 100)),
  notice_read: ({ id }) => {
    const n = notices.find((x) => x.id === id);
    if (n) n.read = true;
  },
  notice_read_all: () => notices.forEach((n) => (n.read = true)),

  insights_overview: ({ days }) => overview(Number(days ?? 30)),
  insights_trends: ({ watchIds, monitorIds, days }) => trendsFor((watchIds as number[]) ?? [], (monitorIds as number[]) ?? [], Number(days ?? 30)),
  insights_report: () => {
    const reportId = uid();
    let i = 0;
    const tick = () => {
      if (i >= REPORT.length) {
        emit("insight://done", { reportId, text: REPORT, error: null });
        return;
      }
      const n = 4 + Math.floor(Math.random() * 10);
      emit("insight://delta", { reportId, delta: REPORT.slice(i, i + n) });
      i += n;
      window.setTimeout(tick, 28);
    };
    window.setTimeout(tick, 700);
    return { reportId };
  },

  copilot_skills: () => SKILLS,
  copilot_history: ({ sessionId }) => [...(copilotSessions.get(sessionId as string) ?? [])],
  copilot_send: ({ input }) => {
    if (!aiStatus.verified) throw err("AI_REQUIRED", "请先连接 AI 模型");
    const i = input as CopilotSendInput;
    const list = copilotSessions.get(i.sessionId) ?? [];
    copilotSessions.set(i.sessionId, list);
    list.push({ id: uid(), sessionId: i.sessionId, role: "user", content: i.text, actions: [], createdAt: iso() });
    const messageId = uid();
    const { content, actions } = copilotReply(i.text, i.skill);
    const raw = content + "\n\n```actions\n" + JSON.stringify(actions) + "\n```";
    let k = 0;
    const tick = () => {
      if (k >= raw.length) {
        const msg: CopilotMessage = { id: messageId, sessionId: i.sessionId, role: "assistant", content, actions, createdAt: iso() };
        list.push(msg);
        lastUsage = { ...lastUsage, calls: lastUsage.calls + 1 };
        emit("copilot://done", { sessionId: i.sessionId, message: msg });
        return;
      }
      const n = 2 + Math.floor(Math.random() * 6);
      emit("copilot://delta", { sessionId: i.sessionId, messageId, delta: raw.slice(k, k + n) });
      k += n;
      window.setTimeout(tick, 22);
    };
    window.setTimeout(tick, 600);
    return { messageId };
  },
  copilot_reset: ({ sessionId }) => {
    copilotSessions.set(sessionId as string, []);
  },
};

function title(w: WatchItem): string {
  return w.title.length > 18 ? w.title.slice(0, 18) + "…" : w.title;
}

export async function mockInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  const h = H[cmd];
  if (!h) throw err("INTERNAL", `mock 未实现命令：${cmd}`);
  await sleep(60 + Math.random() * 90);
  // 与真实 IPC 一致：参数与返回值都经过 JSON 序列化（也避免把 Vue 响应式代理存进模拟状态）
  const out = await h(JSON.parse(JSON.stringify(args ?? {})));
  return out === undefined ? undefined : JSON.parse(JSON.stringify(out));
}
