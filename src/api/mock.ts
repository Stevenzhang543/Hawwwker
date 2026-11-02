/**
 * 浏览器预览（npm run dev）用的模拟后端：实现 api/index.ts 中的全部命令，并按真实节奏推送事件。
 * URL 参数：?onboarding=1 预览首次引导；?noai=1 预览"AI 未连接"守卫。
 */
import type {
  AiConfig, AiConfigInput, AiStatus, AiSummary, AppSettings, AppSettingsPatch, Bootstrap, CopilotAction, CopilotMessage,
  CopilotSendInput, CustomPlatformInput, ListingDetail, LoginState, MarketOverview, Monitor, MonitorCreateInput,
  MonitorPatch, Notice, PlatformInfo, PlatformPatch, PricePoint, ResultItem, RunDetail, RunEvent, RunEventBody, RunStats,
  RunSummary, SearchPlan, TrendSeries, Verdict, WatchAddInput, WatchItem, WatchPatch, PlatformStat,
  DealInfo, LinkInfo, LocalAi, Recommendation, SelfTestResult,
} from "./types";
import {
  FX, PRESETS, SKILLS, fxConvert, hash, makeLogins, makeNotices, makePlatforms, productImage, rng, uid,
} from "./mockData";
import { mockFinal, mockReviews, mockRisk, mockShop, mockTags } from "./mockInsight";

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
    depth: "standard", reviewTop: 6, hideHighRisk: true,
  },
  notify: { system: true, dropPercent: 5, webhookKind: "none", webhookUrl: "" },
};

let aiConfig: AiConfig = {
  protocol: "openai", provider: "deepseek", baseUrl: "https://api.deepseek.com", model: params.has("oldmodel") ? "deepseek-chat" : "deepseek-flash", fastModel: "",
  temperature: 0.2, timeoutSecs: 60, maxConcurrency: 3, hasKey: !NO_AI, keyHint: NO_AI ? "" : "••••7f3a",
};
let aiStatus: AiStatus = NO_AI
  ? { configured: !FORCE_ONBOARD && !NO_AI, verified: false, model: "", provider: "", lastError: FORCE_ONBOARD ? null : "API Key 已失效（HTTP 401）" }
  : { configured: true, verified: true, model: params.has("oldmodel") ? "deepseek-chat" : "deepseek-flash", provider: "deepseek", verifiedAt: iso(Date.now() - 3600_000), latencyMs: 842, sample: "你好，我已准备好帮你找货比价。" };
if (NO_AI && !FORCE_ONBOARD) aiStatus = { ...aiStatus, configured: true, model: "deepseek-flash", provider: "deepseek" };

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
  // v26.03：指令里点名的平台 = 指定平台（只搜这些，且需要先登录）
  const de = /德国|德亚|\.de\b/i.test(text);
  const NAMED: [RegExp, string][] = [
    [/京东/, "jd"], [/淘宝|天猫/, "taobao"], [/拼多多/, "pinduoduo"], [/闲鱼/, "xianyu"], [/小红书/, "xiaohongshu"], [/抖音/, "douyin"],
    [/1688/, "alibaba1688"], [/亚马逊|amazon/i, de ? "amazon_de" : "amazon_com"], [/ebay/i, "ebay_de"], [/idealo/i, "idealo"], [/temu/i, "temu"],
  ];
  const named = NAMED.filter(([re]) => re.test(text)).map(([, id]) => id).filter((id) => P(id));
  if (named.length) ids = [...new Set(named)];
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
    platformsExplicit: named.length > 0,
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

/** 与 Rust deal::verdict 相同的规则：按本机历史价格判断当前价位置。 */
function mockDeal(hist: PricePoint[], current: number | null | undefined, currency: string): DealInfo | null {
  if (current == null) return null;
  const pts = hist.filter((h) => h.currency === currency && Date.now() - new Date(h.t).getTime() <= 180 * 86400_000);
  const days = pts.length ? Math.round((Date.now() - new Date(pts[0]!.t).getTime()) / 86400_000) : 0;
  const prices = pts.map((h) => h.price).sort((a, b) => a - b);
  const q = (f: number) => prices[Math.min(prices.length - 1, Math.floor(f * (prices.length - 1)))]!;
  if (prices.length < 3 || days < 2) {
    return { level: "new", label: "数据积累中", basis: `已记录 ${prices.length} 次价格，再观察几天就能判断`, low: prices[0] ?? null, median: null, high: prices[prices.length - 1] ?? null, points: prices.length, days };
  }
  const low = prices[0]!, median = q(0.5), high = prices[prices.length - 1]!, p25 = q(0.25);
  const basis = `${days} 天内 ${prices.length} 次观测，最低 ${low}，中位 ${median}`;
  const [level, label] = current <= low * 1.005 ? ["lowest", days >= 30 ? "历史最低" : "近期最低"] : current <= p25 ? ["low", "近期低位"] : current <= median * 1.05 ? ["normal", "价格正常"] : ["high", "偏高"];
  return { level: level as DealInfo["level"], label, basis, low, median, high, points: prices.length, days };
}

/** 模拟运费 / 券后价，与真实解析器输出同样的字段。 */
function mockLanded(price: number, currency: string, cn: boolean, k: number, r: () => number) {
  const shipping = cn ? (k % 4 === 3 ? 8 : 0) : k % 3 === 0 ? 0 : currency === "JPY" ? 550 : 4.99;
  const coupon = cn && r() < 0.45 ? round(price * (0.9 + r() * 0.06), currency) : !cn && r() < 0.2 ? round(price * 0.9, currency) : null;
  const landed = round((coupon ?? price) + shipping, currency);
  return { shipping, couponPrice: coupon, landedPrice: landed, displayLanded: round(fxConvert(landed, currency, settings.displayCurrency), settings.displayCurrency) };
}

const feedbackMap = new Map<string, string>();

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
      // v26.03 演示：淘宝 / 拼多多最后一条是"原单 / 同款"仿品（价格异常低）
      const fake = (pid === "taobao" || pid === "pinduoduo") && k === n - 1;
      const v = fake ? VARIANTS[3]! : VARIANTS[(k * 3 + pi + Math.floor(r() * 3)) % VARIANTS.length]!;
      const regional = p.region === "cn" ? 0.92 + r() * 0.12 : 1.05 + r() * 0.2;
      const priceCny = baseCny * v.mult * regional * (fake ? 0.38 : 1);
      const price = round(fxConvert(priceCny, "CNY", p.currency), p.currency);
      const id = `${pid}:${(seed + pi * 31 + k * 7).toString(36)}`;
      const de = p.locale.startsWith("de");
      const en = p.region === "intl" && !de;
      const title = fake ? `${kw} 原单 高品质 同款 现货速发` : p.region === "cn" ? `${kw}${v.suffix}` : de ? `${kw}${v.suffixDe}` : en ? `${kw} ${v.verdict === "mismatch" ? "Case / Ear Pads Accessory" : v.cond === "used" ? "Used - Very Good" : "Wireless, Black"}` : `${kw}${v.suffix}`;
      const hist = walk(hash(id), price, 60, p.currency, 24);
      const landed = mockLanded(price, p.currency, p.region === "cn", k, r);
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
        ...landed, deal: mockDeal(hist, landed.couponPrice ?? price, p.currency), provisional: false, feedback: feedbackMap.get(id) ?? null,
      };
      if (item.feedback === "irrelevant") item.match = { ...item.match, verdict: "mismatch", reasons: ["你之前标记为不相关", ...item.match.reasons] };
      // v26.03 店铺（仿品固定为个人卖家）
      item.shopInfo = mockShop(pid, fake ? 2 : k, plan.product.brand ?? null, r);
      item.shop = item.shopInfo.name ?? item.shop;
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
  out.sort((a, b) => order[a.match.verdict] - order[b.match.verdict] || (plan.sort === "price_asc" ? (a.displayLanded ?? a.displayPrice ?? 0) - (b.displayLanded ?? b.displayPrice ?? 0) : b.match.score - a.match.score));
  // v26.03：风险 → 评论（排名靠前的 N 件）→ 标签 → 综合分，按综合分重排
  const good = out.filter((x) => x.match.verdict === "exact" || x.match.verdict === "close").map((x) => x.displayLanded ?? x.displayPrice ?? 0).filter((x) => x > 0).sort((a, b) => a - b);
  const median = good.length ? good[Math.floor(good.length / 2)]! : null;
  for (const x of out) x.risk = mockRisk(x, x.shopInfo!, median);
  const depth = settings.search.depth;
  const topN = depth === "quick" ? 0 : depth === "deep" ? Math.max(12, settings.search.reviewTop) : settings.search.reviewTop;
  out.filter((x) => x.match.verdict !== "mismatch").slice(0, topN).forEach((x) => (x.reviewInsight = mockReviews(x, x.shopInfo!, r, false)));
  for (const x of out) {
    x.tags = mockTags(x, x.shopInfo!, x.risk ?? null, x.reviewInsight ?? null);
    x.finalScore = x.match.verdict === "mismatch" ? null : mockFinal(x);
  }
  out.sort((a, b) => order[a.match.verdict] - order[b.match.verdict] || (b.finalScore ?? 0) - (a.finalScore ?? 0));
  out.forEach((x, i) => (x.rank = i + 1));
  return out;
}

/** v26.03 分阶段推送：只保留到 upto 阶段为止已经得出的信息 */
type EnrichStage = "screen" | "risk" | "shop" | "reviews" | "review_ai" | "final";
function staged(full: ResultItem[], upto: EnrichStage): ResultItem[] {
  const rank: Record<EnrichStage, number> = { screen: 0, risk: 1, shop: 2, reviews: 3, review_ai: 4, final: 5 };
  const n = rank[upto];
  const list = full.map((x): ResultItem => ({
    ...x,
    risk: n >= 1 ? x.risk : null,
    shopInfo: n >= 2 ? x.shopInfo : null,
    shop: x.shop,
    tags: n >= 2 ? (x.tags ?? []).filter((t) => t.kind !== "well_reviewed" || n >= 4) : [],
    // 评论检索阶段只有原始数据（评分 / 条数 / 原文），AI 分析后才有优缺点、风险信号与评论分
    reviewInsight: n >= 4 ? x.reviewInsight : n === 3 && x.reviewInsight ? { ...x.reviewInsight, pros: [], cons: [], redFlags: [], matchNote: null, score: null, aiChecked: false } : null,
    finalScore: n >= 5 ? x.finalScore : null,
  }));
  // 细筛之前按匹配度排序，细筛后用综合分排名
  if (n < 5) {
    const order: Record<Verdict, number> = { exact: 0, close: 1, related: 2, mismatch: 3 };
    list.sort((a, b) => order[a.match.verdict] - order[b.match.verdict] || b.match.score - a.match.score);
    list.forEach((x, i) => (x.rank = i + 1));
  }
  return list;
}

function makeSummary(plan: SearchPlan, results: ResultItem[]): AiSummary {
  const good = results.filter((x) => (x.match.verdict === "exact" || x.match.verdict === "close") && x.risk?.level !== "high");
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
  const needLogin = (id: string) => {
    const st = logins.find((l) => l.platformId === id);
    return P(id)?.login === "required" && st?.status !== "logged_in";
  };
  // v26.03 平台判定：AI 选择的平台里需要登录但没登录的直接跳过并说明（指定平台在 search_start 已被拦截）
  const blocked = pids.filter(needLogin);
  const ok = pids.filter((id) => !blocked.includes(id));
  const skipped = blocked.map((id) => ({ platformId: id, reason: "login_required", message: `${P(id)?.name} 未登录或登录已过期，本次跳过；登录后可重新搜索` }));
  if (!finalPlan.platformsExplicit && !pids.includes("xiaohongshu") && P("xiaohongshu")?.enabled && needLogin("xiaohongshu")) {
    skipped.push({ platformId: "xiaohongshu", reason: "login_required", message: "适合看真实口碑，但小红书未登录，本次跳过" });
  }
  const disabledSkip = platforms.filter((p) => !p.enabled && p.region === "cn" && p.kind === "c2c").slice(0, 1).map((p) => ({ platformId: p.id, reason: "disabled", message: "在平台目录中未启用" }));
  const stats: PlatformStat[] = [];
  const t0 = Date.now();
  const results = makeResults(runId, finalPlan, ok);
  const depth = settings.search.depth;
  const reviewed = results.filter((x) => x.reviewInsight).length;
  const reviewedOk = results.filter((x) => x.reviewInsight && x.reviewInsight.source !== "none").length;

  at(60, () => push({ type: "stage", stage: "plan", label: "AI 理解需求", progress: 0.04 }));
  at(520, () => push({ type: "plan", plan: finalPlan }));
  at(700, () => push({ type: "stage", stage: "platforms", label: "平台判定", progress: 0.08 }));
  at(980, () =>
    push({
      type: "platform_plan",
      chosen: ok,
      skipped: [...skipped, ...disabledSkip],
      reason: finalPlan.platformsExplicit ? "按你指定的平台搜索" : `按「${finalPlan.product.category || "综合"}」品类与地区选择 ${ok.length} 个平台：综合电商比价 + 二手参考`,
    }),
  );
  at(1150, () => push({ type: "stage", stage: "search", label: "多平台并行搜索", progress: 0.12 }));
  ok.forEach((id, i) => at(1200 + i * 90, () => push({ type: "platform", platformId: id, state: "queued" })));
  ok.forEach((id, i) => at(1400 + i * 260, () => push({ type: "platform", platformId: id, state: "searching", message: "打开搜索页…" })));
  let lastDone = 1400;
  let fetched = 0;
  ok.forEach((id, i) => {
    const doneAt = 2300 + i * 420 + (hash(id) % 600);
    lastDone = Math.max(lastDone, doneAt);
    const count = 14 + (hash(id + runId) % 24);
    fetched += count;
    if (i === 1) at(doneAt - 900, () => push({ type: "platform", platformId: id, state: "searching", message: "滚动加载第 2 屏…" }));
    at(doneAt, () => {
      push({ type: "platform", platformId: id, state: "done", count, ms: doneAt - 1400 - i * 260 });
      const prov = staged(results, "screen").filter((x) => x.platformId === id).map((x): ResultItem => {
        const rule = Math.max(20, Math.min(95, x.match.score + ((hash(x.id) % 15) - 7)));
        return { ...x, id: `p:${x.platformId}:${x.externalId}`, provisional: true, deal: null, alsoOn: [], clusterId: null,
          match: { score: rule, verdict: rule >= 85 ? "exact" : rule >= 65 ? "close" : rule >= 35 ? "related" : "mismatch", reasons: ["标题包含关键词（规则初判）"], flags: x.match.flags, aiChecked: false } };
      });
      if (prov.length) push({ type: "partial", platformId: id, items: prov });
      stats.push({ platformId: id, state: "done", count, ms: doneAt - 1400, message: null });
    });
  });
  for (const sk of skipped) stats.push({ platformId: sk.platformId, state: "login_required", count: 0, ms: 0, message: "需要登录" });

  const visible = results.filter((x) => x.match.verdict !== "mismatch");
  const hidden = results.length - visible.length;
  const highRisk = visible.filter((x) => x.risk?.level === "high").length;
  let T = lastDone + 300;
  at(T, () => push({ type: "stage", stage: "screen", label: "粗筛：AI 核对需求匹配度", progress: 0.5 }));
  at(T + 300, () => push({ type: "filter", fetched, kept: results.length + 3, dropped: { exclude: 4, price: 6, duplicate: 3 }, duplicates: 3 }));
  at(T + 1100, () => {
    push({ type: "verify", checked: results.length, matched: visible.length, hidden });
    push({ type: "results", items: staged(results, "screen") });
    push({ type: "funnel", stage: "screen", label: "粗筛", input: fetched, kept: visible.length, note: `去重 3 条、不符合预算 / 排除词 10 条，AI 折叠不相关 ${hidden} 条` });
  });
  T += 1300;
  at(T, () => push({ type: "stage", stage: "risk", label: "商品风险分析（正品 / 功能 / 二手 / 价格）", progress: 0.6 }));
  at(T + 1000, () => {
    push({ type: "results", items: staged(results, "risk") });
    push({ type: "funnel", stage: "risk", label: "风险", input: visible.length, kept: visible.length - highRisk, note: `${highRisk} 条高风险（疑似仿品或价格异常）` });
  });
  T += 1200;
  at(T, () => push({ type: "stage", stage: "shop", label: "店铺可信度匹配", progress: 0.68 }));
  at(T + 800, () => push({ type: "results", items: staged(results, "shop") }));
  T += 1000;
  if (depth !== "quick" && reviewed) {
    at(T, () => push({ type: "stage", stage: "reviews", label: `评论检索：读取前 ${reviewed} 件商品的评论与店铺评价`, progress: 0.74 }));
    // 一件一件读：每读完一件推送一次
    const order = results.filter((x) => x.reviewInsight);
    order.forEach((_, i) =>
      at(T + 700 + i * 450, () => {
        const done = new Set(order.slice(0, i + 1).map((x) => x.id));
        const list = staged(results, "reviews").map((x) => (done.has(x.id) ? x : { ...x, reviewInsight: null }));
        push({ type: "results", items: list });
      }),
    );
    T += 900 + order.length * 450;
    at(T - 100, () =>
      push({ type: "funnel", stage: "reviews", label: "评论", input: reviewed, kept: reviewedOk, note: reviewed > reviewedOk ? `${reviewed - reviewedOk} 件需要登录才能看评论` : "全部读取成功" }),
    );
    at(T, () => push({ type: "stage", stage: "review_ai", label: "AI 分析评论与需求的匹配", progress: 0.84 }));
    at(T + 1200, () => push({ type: "results", items: staged(results, "review_ai") }));
    T += 1400;
  }
  at(T, () => push({ type: "stage", stage: "final", label: "最终细筛：综合匹配、风险、店铺、评论与价格", progress: 0.9 }));
  at(T + 700, () => {
    run.results = results;
    push({ type: "results", items: staged(results, "final") });
    push({ type: "funnel", stage: "final", label: "最终", input: visible.length, kept: settings.search.hideHighRisk ? visible.length - highRisk : visible.length, note: settings.search.hideHighRisk && highRisk ? `${highRisk} 条高风险已折叠` : null });
  });
  T += 900;
  at(T, () => push({ type: "stage", stage: "summarize", label: "AI 总结与推荐", progress: 0.95 }));
  at(T + 800, () => {
    run.aiSummary = makeSummary(finalPlan, results);
    push({ type: "summary", summary: run.aiSummary });
  });
  at(T + 1000, () => {
    const best = visible.filter((x) => x.match.verdict === "exact" && x.risk?.level !== "high").sort((a, b) => (a.displayPrice ?? 0) - (b.displayPrice ?? 0))[0];
    run.stats = { fetched, kept: visible.length, hidden, duplicates: 3, dropped: { exclude: 4, price: 6 }, elapsedMs: Date.now() - t0, platforms: stats, aiCalls: depth === "quick" ? 4 : 6 };
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
    let results = status === "done" ? makeResults(runId, plan, ok) : [];
    // 98 小时前那条模拟 v26.02 的旧记录：没有标签 / 风险 / 店铺 / 评论 / 综合分
    if (hoursAgo === 98) results = results.map((x) => ({ ...x, tags: undefined, shopInfo: undefined, risk: undefined, reviewInsight: undefined, finalScore: undefined }));
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
  w.deal = mockDeal(h, w.lastPrice, w.currency);
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
      myPrice: pid === "temu" ? 15.9 : pid === "alibaba1688" ? 39 : null, deal: null,
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

/** 搜索完成后的自动点评（mock）：按综合分 / 到手价挑推荐，并点出风险 */
function followupReply(ctx: unknown): { content: string; actions: CopilotAction[] } {
  type C = { id: string; title: string; platform: string; shop?: string | null; landed?: number | null; currency: string; verdict: string; risk?: { level: string; why?: string } | null; reviews?: { score?: number | null } | null; finalScore?: number | null };
  const items = ((ctx as { results?: C[] })?.results ?? []).filter((x) => x.verdict === "exact" || x.verdict === "close");
  if (!items.length) return { content: "这次没有找到和需求完全一致的商品，可以放宽预算或换个关键词再试。", actions: [] };
  const safe = items.filter((x) => x.risk?.level !== "high");
  const pick = [...(safe.length ? safe : items)].sort((a, b) => (b.finalScore ?? 0) - (a.finalScore ?? 0) || (a.landed ?? 1e12) - (b.landed ?? 1e12))[0]!;
  const cheapest = [...items].sort((a, b) => (a.landed ?? 1e12) - (b.landed ?? 1e12))[0]!;
  const cur = settings.displayCurrency;
  const money = (v?: number | null) => (v == null ? "—" : new Intl.NumberFormat("zh-CN", { style: "currency", currency: cur, maximumFractionDigits: 0 }).format(v));
  const risky = items.filter((x) => x.risk?.level === "high").slice(0, 2);
  const lines = [
    `**推荐：${pick.title.slice(0, 28)}**（${pick.platform}${pick.shop ? " · " + pick.shop : ""}，到手 ${money(pick.landed)}）——匹配度高、店铺可信${pick.reviews?.score ? `，评论分 ${pick.reviews.score}` : ""}。`,
    cheapest.id !== pick.id ? `- 最便宜的是 ${cheapest.platform} 的 ${money(cheapest.landed)}，${cheapest.risk?.level === "high" ? "但风险偏高，不建议" : "可以作为备选"}。` : "- 它同时也是到手价最低的一款。",
    ...risky.map((x) => `- ⚠️ ${x.platform}「${x.title.slice(0, 16)}…」风险高：${x.risk?.why || "价格异常或店铺可信度低"}。`),
    "- 不急的话可以追踪推荐款，降到目标价再买。",
  ];
  return { content: lines.join("\n"), actions: [{ type: "watch", label: "追踪推荐款", listingId: pick.id, targetPrice: null, auto: false }] };
}

/** 用户明确要求 App 去执行时才产生 auto 动作（与 Rust copilot_prompts::wants_app_action 一致的简化版） */
function wantsAppAction(text: string): boolean {
  const t = text.toLowerCase().replace(/\s/g, "");
  return /用app|用软件|在app|app里|帮我搜|帮我查|去搜|搜一下|搜搜|查一下|去(京东|淘宝|拼多多|闲鱼|亚马逊|amazon|ebay)|帮我追踪|加入追踪|帮我盯|盯一下|每天帮我|每周帮我|定时|帮我监控|打开|切换|改成|导出|复制清单|帮我登录|去登录|取消搜索|按到手价排序/.test(t) || /^(搜|search)/.test(t);
}
const PLATFORM_WORDS: [RegExp, string][] = [[/京东/, "jd"], [/淘宝|天猫/, "taobao"], [/拼多多/, "pinduoduo"], [/闲鱼/, "xianyu"], [/亚马逊德国|amazon\.?de/i, "amazon_de"], [/ebay/i, "ebay_de"]];

/** App 操控类指令（mock）：搜索 / 追踪页 / 深色 / 导出 / 监控 / 登录 */
function appControlReply(text: string): { content: string; actions: CopilotAction[] } | null {
  if (!wantsAppAction(text)) return null;
  const t = text.replace(/^(请)?(帮我)?(用\s*app|用软件|在\s*app\s*里)?/i, "");
  if (/深色|暗色|dark/i.test(text)) return { content: "好的，已为你切换到深色模式。", actions: [{ type: "setting", label: "切换深色", key: "theme", value: "dark", auto: true }] };
  if (/浅色|亮色|light/i.test(text)) return { content: "好的，已切换到浅色模式。", actions: [{ type: "setting", label: "切换浅色", key: "theme", value: "light", auto: true }] };
  if (/导出/.test(text)) return { content: "好的，正在把当前搜索结果导出为 CSV。", actions: [{ type: "results", label: "导出 CSV", op: "export_csv", auto: true }] };
  if (/复制清单/.test(text)) return { content: "好的，比价清单复制好了，可以直接粘贴给朋友。", actions: [{ type: "results", label: "复制比价清单", op: "copy_list", auto: true }] };
  if (/打开.*追踪/.test(text)) return { content: "已为你打开价格追踪页。", actions: [{ type: "navigate", label: "打开价格追踪", route: "/track/watchlist", auto: true }] };
  if (/登录/.test(text)) {
    const id = PLATFORM_WORDS.find(([re]) => re.test(text))?.[1] ?? "taobao";
    return { content: `好的，已打开 ${id === "jd" ? "京东" : id === "taobao" ? "淘宝" : id} 的登录窗口，请在窗口里完成登录（账号密码和验证码只由你本人输入）。`, actions: [{ type: "login", label: "打开登录窗口", platformId: id, auto: true }] };
  }
  if (/每天|每周|定时|监控|盯/.test(text)) {
    const what = t.replace(/每天|每周|帮我|盯一下|盯|监控一下|监控|定时|的价格|价格/g, "").trim() || "Switch 2 马力欧套装";
    return { content: `好的，我已经创建了「${what}」的定时监控（每 ${/每周/.test(text) ? "7 天" : "天"}一次），降价或出现新低会提醒你。`, actions: [{ type: "monitor", label: `监控：${what}`, instruction: what, intervalMinutes: /每周/.test(text) ? 10080 : 1440, targetPrice: null, auto: true }] };
  }
  const platforms = PLATFORM_WORDS.filter(([re]) => re.test(text)).map(([, id]) => id).filter((id) => P(id));
  const q = t.replace(/搜索一下|搜一下|搜索|去搜|搜搜|查一下|帮我查|^搜/g, "").replace(/^(在|去)/, "").trim() || text;
  return {
    content: `好的，我正在${platforms.length ? platforms.map((id) => P(id)?.name).join("、") : "多个平台"}搜索「${q}」，结果出来后我会帮你点评。`,
    actions: [{ type: "search", label: `搜索：${q.slice(0, 18)}`, instruction: q, platforms: platforms.length ? platforms : null, auto: true }],
  };
}

function copilotReply(text: string, skill?: string): { content: string; actions: CopilotAction[] } {
  const ctl = appControlReply(text);
  if (ctl) return ctl;
  const s = skill || (/礼物|送/.test(text) ? "gift" : /文案|标题/.test(text) ? "copy" : /行业|品类|格局/.test(text) ? "industry" : /进货|货源|选品/.test(text) ? "sourcing" : /值得|入手|等等/.test(text) ? "timing" : "compare");
  switch (s) {
    case "gift":
      if (/茶/.test(text)) {
        return {
          content:
            "送爱喝茶的朋友，**建议送好茶具，不建议直接送茶叶**：爱喝茶的人往往只认自己常喝的那几种茶、产区甚至茶商，不知道他的口味时随手买的茶叶很可能不合心意，反而显得外行。茶具则人人用得上，也更体面。\n\n**500 元内的 4 个稳妥方向**（按到手价算）：\n1. **旅行茶具套装**（¥180–380）：快客杯 / 便携盖碗 + 收纳包，出差露营都能泡，实用度最高。\n2. **白瓷或青瓷盖碗 + 公道杯 + 品茗杯组合**（¥200–450）：不挑茶类，绿茶、乌龙、红茶都适合。\n3. **建盏品茗杯**（¥150–400）：有收藏感，选正规窑口、带证书的店铺。\n4. **随手泡 / 煮茶器**（¥250–499）：控温烧水壶对泡茶体验提升明显。\n\n**避雷**：不送紫砂壶（壶型和泥料讲究多，容易踩坑）；包装避免纯白纯黑；选支持七天无理由的旗舰店，方便对方换货。\n\n> 如果你知道他平时只喝某一种茶（比如武夷岩茶、普洱），告诉我，我再帮你挑对应的好茶。",
          actions: [
            { type: "search", label: "搜：旅行茶具套装 500 元内", instruction: "旅行茶具套装 便携 500元以内 礼盒", auto: false },
            { type: "search", label: "搜：盖碗公道杯套装", instruction: "白瓷盖碗 公道杯 品茗杯 套装 500元以内 礼盒", auto: false },
            { type: "search", label: "搜：建盏品茗杯", instruction: "建盏 品茗杯 正品 带证书 400元以内", auto: false },
          ],
        };
      }
      return {
        content: "根据你的描述，我给出 **3 个稳妥方向**，都控制在预算内（按到手价）：\n\n1. **轻量露营灯 + 挂钩套装**（¥120–260）：实用、几乎每次露营都会用到，选 USB-C 充电、带红光模式的款。\n2. **钛合金露营杯 / 咖啡器具套装**（¥180–420）：有仪式感；送器具而不是咖啡豆，避免口味不合。\n3. **便携户外电源 300Wh 级**（¥450–499 档位较少，注意促销）：给手机、灯、风扇供电，体验升级明显。\n\n> 送长辈避开钟、伞、鞋；建议优先看京东自营和天猫旗舰店，开票、售后和换货更省心。",
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
/** v26.03：只更新这一个平台（平台之间不再联动） */
function loginSuccess(id: string) {
  const p = P(id);
  const now = iso();
  setLogin(id, { status: "logged_in", flow: "success", verifiedAt: now, checkedAt: now, vaulted: true, lastWallAt: null, cookieExpires: iso(Date.now() + 30 * 86400_000), message: `${p?.name ?? id} 登录成功，3 秒后自动关闭窗口` });
  addNotice({ kind: "login", level: "success", title: `${p?.name}${p?.region === "intl" ? " · " + p.edition : ""} 已登录`, body: "登录会话保存在本机（含加密备份），之后搜索会自动使用。", link: "/platforms/accounts" });
  loginTimers.get(id)?.push(window.setTimeout(() => setLogin(id, { flow: "idle", message: null }), 8000));
}

function addNotice(n: Omit<Notice, "id" | "read" | "createdAt" | "url"> & { url?: string }) {
  const notice: Notice = { ...n, url: n.url ?? "", id: ++noticeSeq, read: false, createdAt: iso() };
  notices.unshift(notice);
  emit("notice://new", notice);
}

/* ================================================================ 命令 */
type Handler = (a: Record<string, unknown>) => unknown | Promise<unknown>;

const H: Record<string, Handler> = {
  app_bootstrap: (): Bootstrap => ({
    version: "26.03", dataDir: "C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app", settings: jclone(settings),
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
    logins.push({ platformId: id, status: "not_needed", flow: "idle", checkedAt: null, vaulted: false });
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
    const name = P(id)?.name ?? id;
    const st = setLogin(id, { flow: "opening", message: `正在打开 ${name} 官网…` });
    const ts = loginTimers.get(id)!;
    ts.push(window.setTimeout(() => setLogin(id, { flow: "waiting", message: `请在弹出的浏览器窗口中登录 ${name}，登录成功后会自动识别（会打开账号页确认），请勿手动关闭窗口` }), 700));
    ts.push(window.setTimeout(() => setLogin(id, { flow: "waiting", message: "正在登录 / 身份验证中…完成后会自动识别（请勿关闭窗口）" }), 3600));
    ts.push(window.setTimeout(() => setLogin(id, { flow: "verifying", message: "检测到登录信息，正在确认…（请勿关闭窗口）" }), 6200));
    ts.push(window.setTimeout(() => setLogin(id, { flow: "verifying", message: "正在打开账号页确认登录…（请勿关闭窗口）" }), 8200));
    ts.push(window.setTimeout(() => loginSuccess(id), 9600));
    return st;
  },
  login_confirm: async ({ platformId }) => {
    const id = platformId as string;
    const cur = logins.find((l) => l.platformId === id);
    const name = P(id)?.name ?? id;
    if (cur && ["opening", "waiting", "verifying"].includes(cur.flow)) {
      clearLoginTimers(id);
      const st = setLogin(id, { flow: "verifying", message: "正在打开账号页确认登录…" });
      loginTimers.get(id)!.push(window.setTimeout(() => loginSuccess(id), 1400));
      return st;
    }
    // 没有登录流程：主动验证一次（打开账号页）
    await sleep(1100);
    if (cur?.status === "logged_in") {
      const st = setLogin(id, { checkedAt: iso(), verifiedAt: iso(), vaulted: true })!;
      return { ...st, message: `${name} 登录有效（已打开账号页确认）` };
    }
    if (cur?.status === "not_needed") return { ...cur, message: `${name} 无需登录即可搜索` };
    const st = setLogin(id, { status: cur?.status === "expired" ? "expired" : "logged_out", checkedAt: iso() })!;
    return { ...st, message: `${name} 需要重新登录：账号页跳转到了登录 / 验证页面` };
  },
  login_cancel: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    return setLogin(id, { flow: "cancelled", message: "已取消登录（窗口已关闭）" });
  },
  logout: ({ platformId }) => {
    const id = platformId as string;
    clearLoginTimers(id);
    setLogin(id, { status: "logged_out", flow: "idle", verifiedAt: null, checkedAt: null, vaulted: false, cookieExpires: null, lastWallAt: null, message: null });
    return { ...logins.find((l) => l.platformId === id)!, message: "已退出，本机会话与备份已删除，其他平台不受影响" };
  },
  browser_info: () => ({ executable: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", running: true, profileDir: "C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app\\browser-profile", transport: "pipe" }),
  browser_show: () => undefined,
  browser_wipe: async () => {
    await sleep(600);
    logins.forEach((l) => {
      if (l.status !== "not_needed") setLogin(l.platformId, { status: "logged_out", flow: "idle", verifiedAt: null, checkedAt: null, vaulted: false, cookieExpires: null, lastWallAt: null, message: null });
    });
  },

  search_plan: async ({ instruction }) => {
    await sleep(1100);
    return mockPlan(String(instruction ?? ""));
  },
  search_start: async ({ instruction, plan }) => {
    if (!aiStatus.verified) throw err("AI_REQUIRED", "请先连接并测试 AI 模型");
    await sleep(120);
    // v26.03：指定平台（选择器 / 指令点名）里需要登录却没登录的，不能搜索
    const p = (plan as SearchPlan | null) ?? mockPlan(String(instruction ?? ""));
    if (p.platformsExplicit) {
      const need = p.platforms.filter((id) => P(id)?.login === "required" && logins.find((l) => l.platformId === id)?.status !== "logged_in");
      if (need.length) throw { code: "LOGIN_REQUIRED", message: `请先登录：${need.map((id) => P(id)?.name ?? id).join("、")}（登录后再搜索）`, platforms: need };
    }
    return startRun(String(instruction ?? ""), p);
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
    return { item: { ...l.item, watched: !!w }, history: l.history, alsoOn: l.item.alsoOn, watchId: w?.id ?? null, deal: l.item.deal ?? null };
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
      myPrice: null, deal: null,
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
    refreshWatch(w);
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
        addNotice({ kind: w.targetPrice && next <= w.targetPrice ? "target_hit" : "price_drop", level: "success", title: `${title(w)} 降价 ${(((last - next) / last) * 100).toFixed(1)}%`, body: `${P(w.platformId)?.name} 当前 ${next} ${w.currency}`, link: `/track/watchlist?id=${w.id}`, url: w.url });
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

  /* ---------------------------------------------------------- v26.02 */
  link_inspect: async ({ url }): Promise<LinkInfo> => {
    const u = String(url ?? "").trim();
    if (!/^https?:\/\//i.test(u)) throw err("INVALID", "请粘贴以 http(s):// 开头的商品链接");
    await sleep(900);
    let host = "";
    try { host = new URL(u).hostname.replace(/^www\./, ""); } catch { throw err("INVALID", "链接格式不正确"); }
    const p = platforms.find((x) => { try { const h = new URL(x.home).hostname.replace(/^www\./, ""); return host === h || host.endsWith("." + h) || h.endsWith("." + host); } catch { return false; } });
    const w = watches.find((x) => x.url === u);
    const title = p?.region === "cn" ? "Apple AirPods Pro 2 USB-C 主动降噪 官方正品" : "Apple AirPods Pro 2 mit USB-C, Aktive Geräuschunterdrückung";
    const price = p ? round(fxConvert(1599, "CNY", p.currency), p.currency) : null;
    return { url: u, platformId: p?.id ?? null, platformName: p ? `${p.name}${p.region === "intl" ? " · " + p.edition : ""}` : null, title: p ? title : null, price, currency: p?.currency ?? null,
      image: p ? productImage(hash(u), 2) : null, instruction: `全网找同款比价：${p ? title : u.slice(0, 60)}`, watchId: w?.id ?? null,
      message: p ? null : "没有识别出平台，仍可直接搜索或追踪" };
  },
  feedback_set: ({ listingId, runId, kind }) => {
    const id = String(listingId);
    const real = id.startsWith("p:") ? id.slice(2) : id;
    if (kind === "clear") feedbackMap.delete(real); else feedbackMap.set(real, String(kind));
    const run = runs.get(String(runId));
    run?.results.forEach((x) => { if (x.id === real) x.feedback = kind === "clear" ? null : String(kind); });
    const l = listings.get(real);
    if (l) l.item.feedback = kind === "clear" ? null : String(kind);
  },
  run_export: ({ runId, format }) => {
    const r = runs.get(String(runId));
    if (!r) throw err("NOT_FOUND", "运行不存在或已删除");
    if (format === "csv") return `C:\\Users\\steven\\AppData\\Roaming\\com.hawwwker.app\\exports\\search-${new Date().toISOString().slice(0, 19).replace(/[-:T]/g, "")}.csv`;
    const cur = settings.displayCurrency;
    const lines = r.results.filter((x) => x.match.verdict !== "mismatch" && !x.feedback).slice(0, 15).map((x, i) => {
      const p = P(x.platformId);
      return `${i + 1}. ${p?.name ?? x.platformId} ${p?.edition ?? ""} | ${x.title.slice(0, 50)} | 到手 ${cur} ${(x.displayLanded ?? x.displayPrice ?? 0).toFixed(2)} | 标价 ${x.currency} ${x.price?.toFixed(2) ?? "-"} | ${x.deal?.label ?? ""}\n   ${x.url}`;
    });
    return `【Hawwwker 比价】${r.summary.instruction}\n${r.aiSummary ? "结论：" + r.aiSummary.headline + "\n" : ""}（到手价已换算为 ${cur}，含运费与券后价）\n\n${lines.join("\n")}\n`;
  },
  ai_detect_local: async (): Promise<LocalAi> => {
    await sleep(500);
    return params.has("ollama") ? { found: true, baseUrl: "http://localhost:11434/v1", models: ["qwen3:8b", "llama3.2:3b"] } : { found: false, baseUrl: "", models: [] };
  },
  platforms_recommend: ({ timezone, language }): Recommendation => {
    const tz = String(timezone ?? "").toLowerCase();
    const zh = String(language ?? "").toLowerCase().startsWith("zh");
    const cn = ["taobao", "jd", "pinduoduo", "xianyu"];
    let rec: Recommendation;
    if (tz.startsWith("asia/shanghai")) rec = { region: "cn", ids: ["taobao", "jd", "pinduoduo", "xianyu", "xiaohongshu", "douyin"], reason: "你在中国大陆：推荐国内主流电商与种草平台" };
    else if (/europe\/(berlin|vienna|zurich)/.test(tz)) rec = { region: "all", ids: ["amazon_de", "ebay_de", "idealo", "kleinanzeigen"], reason: "你在德语区：推荐 Amazon.de、eBay.de、idealo 比价与 Kleinanzeigen 二手，并保留国内平台" };
    else if (tz.startsWith("america/")) rec = { region: "all", ids: ["amazon_com", "ebay_com", "walmart", "bestbuy"], reason: "你在美洲：推荐 Amazon.com、eBay、Walmart、Best Buy，并保留国内平台" };
    else rec = { region: "all", ids: ["amazon_com", "aliexpress", "temu"], reason: "推荐通用国际站点，并保留国内平台" };
    if (rec.region !== "cn" && zh) rec.ids.push(...cn);
    rec.ids = rec.ids.filter((id) => P(id));
    return rec;
  },
  platform_selftest: async ({ ids }): Promise<SelfTestResult[]> => {
    const list = ((ids as string[]) ?? []).slice(0, 12);
    await sleep(1500 + list.length * 400);
    return list.map((id) => {
      const p = P(id);
      const st = logins.find((l) => l.platformId === id);
      const ms = 3000 + (hash(id) % 9000);
      if (p?.login === "required" && st?.status !== "logged_in") return { platformId: id, ok: false, count: 0, withPrice: 0, samples: [], ms, loginRequired: true, message: `${p.name} 需要登录：到「账号与登录」登录一次即可` };
      const n = 8 + (hash(id) % 12);
      const price = round(fxConvert(899, "CNY", p?.currency ?? "CNY"), p?.currency ?? "CNY");
      return { platformId: id, ok: true, count: n, withPrice: n - (hash(id) % 3), ms, loginRequired: false, message: null,
        samples: [{ title: p?.region === "cn" ? "蓝牙耳机 主动降噪 长续航" : "Bluetooth Headphones Noise Cancelling", price, currency: p?.currency ?? "CNY", landed: price, url: (p?.home ?? "") + "item/1" }] };
    });
  },

  /* ---------------------------------------------------------- v26.03（各分支在此基础上完善） */
  ai_list_models: async ({ input }) => {
    await sleep(600);
    const i = (input as AiConfigInput | null) ?? { provider: aiConfig.provider, baseUrl: aiConfig.baseUrl };
    const p = PRESETS.find((x) => x.id === i.provider) ?? PRESETS.find((x) => x.baseUrl && i.baseUrl?.startsWith(x.baseUrl));
    if (!p || !p.models.length) return { models: [], message: "该服务不支持列出模型，可手动填写模型名" };
    const extra: Record<string, string[]> = { deepseek: [], openai: ["gpt-6-luna"], qwen: ["qwen3.8-max-0902"], gemini: ["gemini-flash-latest"] };
    return { models: [...p.models, ...(p.fastModel && !p.models.includes(p.fastModel) ? [p.fastModel] : []), ...(extra[p.id] ?? [])].sort(), message: null };
  },
  login_preflight: async ({ platformIds, verify }) => {
    const ids = (platformIds as string[]) ?? [];
    const stale = (t?: string | null) => !t || Date.now() - new Date(t).getTime() > 20 * 60_000;
    const todo = verify ? ids.filter((id) => P(id)?.login !== "optional" && logins.find((l) => l.platformId === id)?.status === "logged_in" && stale(logins.find((l) => l.platformId === id)?.checkedAt)) : [];
    if (todo.length) await sleep(500 + todo.length * 250);
    todo.forEach((id) => setLogin(id, { checkedAt: iso(), verifiedAt: iso() }));
    return ids.map((id) => {
      const p = P(id);
      const st = logins.find((l) => l.platformId === id);
      const ok = st?.status === "logged_in" || st?.status === "not_needed" || p?.login === "optional";
      const message = ok ? null : st?.status === "expired" ? `${p?.name} 的登录已过期，请重新登录` : p?.login === "recommended" ? `建议先登录 ${p?.name}（不登录也能搜，结果可能较少）` : `${p?.name} 需要先登录才能搜索`;
      return { platformId: id, need: p?.login ?? "optional", status: st?.status ?? "unknown", ok, verifiedNow: todo.includes(id), message };
    });
  },
  login_check_all: async () => {
    await sleep(1500);
    platforms.filter((p) => p.enabled && p.login !== "optional").forEach((p) => {
      const st = logins.find((l) => l.platformId === p.id);
      if (st?.status === "logged_in") setLogin(p.id, { checkedAt: iso(), verifiedAt: iso(), vaulted: true });
    });
    return jclone(logins);
  },

  copilot_skills: () => SKILLS,
  copilot_history: ({ sessionId }) => [...(copilotSessions.get(sessionId as string) ?? [])],
  copilot_send: ({ input }) => {
    if (!aiStatus.verified) throw err("AI_REQUIRED", "请先连接 AI 模型");
    const i = input as CopilotSendInput;
    const list = copilotSessions.get(i.sessionId) ?? [];
    copilotSessions.set(i.sessionId, list);
    const followup = i.kind === "run_followup";
    list.push({ id: uid(), sessionId: i.sessionId, role: followup ? "event" : "user", content: i.text, actions: [], createdAt: iso() });
    const messageId = uid();
    const { content, actions } = followup ? followupReply(i.context) : copilotReply(i.text, i.skill);
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
