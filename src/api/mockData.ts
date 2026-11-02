/* 浏览器预览用的模拟数据（仅 npm run dev 时使用，Tauri 中不会加载） */
import type {
  AiPreset, CopilotSkill, FxRates, LoginState, LoginStatus, Notice, PlatformInfo, PlatformKind, LoginNeed, Region,
} from "./types";

type PDef = [
  id: string, family: string, familyName: string, name: string, edition: string, region: Region, country: string,
  kind: PlatformKind, currency: string, locale: string, color: string, glyph: string, home: string, search: string,
  group: string, login: LoginNeed, beta: boolean, note: string,
];

const DEFS: PDef[] = [
  ["xiaohongshu", "xiaohongshu", "小红书", "小红书", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#ff2442", "红", "https://www.xiaohongshu.com/explore", "https://www.xiaohongshu.com/search_result?keyword={q}", "xiaohongshu", "required", false, "笔记与种草，适合看口碑与真实体验"],
  ["douyin", "douyin", "抖音", "抖音", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#fe2c55", "抖", "https://www.douyin.com/", "https://www.douyin.com/search/{q}?type=general", "douyin", "required", false, "短视频与直播带货"],
  ["pinduoduo", "pinduoduo", "拼多多", "拼多多", "国内版", "cn", "CN", "marketplace", "CNY", "zh-CN", "#e02e24", "拼", "https://mobile.yangkeduo.com/", "https://mobile.yangkeduo.com/search_result.html?search_key={q}", "pinduoduo", "required", false, "百亿补贴与拼团价"],
  ["taobao", "taobao", "淘宝", "淘宝/天猫", "国内版", "cn", "CN", "marketplace", "CNY", "zh-CN", "#ff5000", "淘", "https://www.taobao.com/", "https://s.taobao.com/search?q={q}", "alibaba_cn", "required", false, "与闲鱼、1688 共享阿里账号"],
  ["jd", "jd", "京东", "京东", "国内版", "cn", "CN", "retail", "CNY", "zh-CN", "#e1251b", "京", "https://www.jd.com/", "https://search.jd.com/Search?keyword={q}&enc=utf-8", "jd", "recommended", false, "自营与官方旗舰店"],
  ["alibaba1688", "1688", "1688", "1688", "国内版", "cn", "CN", "wholesale", "CNY", "zh-CN", "#ff6a00", "批", "https://www.1688.com/", "https://s.1688.com/selloffer/offer_search.htm?keywords={q}", "alibaba_cn", "required", false, "源头工厂与批发价"],
  ["xianyu", "xianyu", "闲鱼", "闲鱼", "国内版", "cn", "CN", "c2c", "CNY", "zh-CN", "#f5b800", "闲", "https://www.goofish.com/", "https://www.goofish.com/search?q={q}", "alibaba_cn", "required", false, "二手与闲置转卖"],
  ["weibo", "weibo", "微博", "微博", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#e6162d", "博", "https://weibo.com/", "https://s.weibo.com/weibo?q={q}", "weibo", "required", false, "公开讨论与求推荐"],
  ["rednote", "xiaohongshu", "小红书", "RedNote", "国际版", "intl", "GL", "social", "USD", "en", "#ff2442", "R", "https://www.rednote.com/", "https://www.rednote.com/search_result?keyword={q}", "rednote", "recommended", true, "2026 年起国际用户使用 rednote.com"],
  ["tiktok", "douyin", "抖音", "TikTok", "国际版", "intl", "GL", "social", "USD", "en", "#13c2c2", "T", "https://www.tiktok.com/", "https://www.tiktok.com/search?q={q}", "tiktok", "recommended", false, "TikTok Shop 与短视频"],
  ["temu", "pinduoduo", "拼多多", "Temu", "国际版", "intl", "GL", "marketplace", "EUR", "de-DE", "#fb7701", "T", "https://www.temu.com/", "https://www.temu.com/search_result.html?search_key={q}", "temu", "optional", false, "按所在国家自动跳转站点"],
  ["aliexpress", "taobao", "淘宝", "AliExpress", "国际版", "intl", "GL", "marketplace", "EUR", "en", "#ff4747", "A", "https://www.aliexpress.com/", "https://www.aliexpress.com/w/wholesale-{q}.html", "aliexpress", "optional", false, "阿里巴巴国际零售"],
  ["joybuy", "jd", "京东", "Joybuy", "国际版·欧洲", "intl", "DE", "retail", "EUR", "de-DE", "#e1251b", "J", "https://www.joybuy.com/", "https://www.joybuy.com/search?keyword={q}", "joybuy", "optional", true, "京东欧洲站，地址以平台目录为准"],
  ["alibaba", "1688", "1688", "Alibaba.com", "国际版", "intl", "GL", "wholesale", "USD", "en", "#ff6a00", "A", "https://www.alibaba.com/", "https://www.alibaba.com/trade/search?SearchText={q}", "alibaba_intl", "optional", false, "国际批发与 OEM"],
  ["amazon_com", "amazon", "Amazon", "Amazon", "美国站", "intl", "US", "retail", "USD", "en-US", "#ff9900", "a", "https://www.amazon.com/", "https://www.amazon.com/s?k={q}", "amazon_com", "recommended", false, "各站点账号独立"],
  ["amazon_de", "amazon", "Amazon", "Amazon", "德国站", "intl", "DE", "retail", "EUR", "de-DE", "#ff9900", "a", "https://www.amazon.de/", "https://www.amazon.de/s?k={q}", "amazon_de", "recommended", false, "价格含 19% 增值税"],
  ["amazon_co_uk", "amazon", "Amazon", "Amazon", "英国站", "intl", "GB", "retail", "GBP", "en-GB", "#ff9900", "a", "https://www.amazon.co.uk/", "https://www.amazon.co.uk/s?k={q}", "amazon_co_uk", "recommended", false, ""],
  ["amazon_co_jp", "amazon", "Amazon", "Amazon", "日本站", "intl", "JP", "retail", "JPY", "ja-JP", "#ff9900", "a", "https://www.amazon.co.jp/", "https://www.amazon.co.jp/s?k={q}", "amazon_co_jp", "recommended", false, "日元价格，注意 ¥ 与人民币区分"],
  ["amazon_fr", "amazon", "Amazon", "Amazon", "法国站", "intl", "FR", "retail", "EUR", "fr-FR", "#ff9900", "a", "https://www.amazon.fr/", "https://www.amazon.fr/s?k={q}", "amazon_fr", "recommended", false, ""],
  ["ebay_com", "ebay", "eBay", "eBay", "美国站", "intl", "US", "marketplace", "USD", "en-US", "#0064d2", "e", "https://www.ebay.com/", "https://www.ebay.com/sch/i.html?_nkw={q}", "ebay_com", "optional", false, "拍卖与一口价"],
  ["ebay_de", "ebay", "eBay", "eBay", "德国站", "intl", "DE", "marketplace", "EUR", "de-DE", "#0064d2", "e", "https://www.ebay.de/", "https://www.ebay.de/sch/i.html?_nkw={q}", "ebay_de", "optional", false, ""],
  ["ebay_co_uk", "ebay", "eBay", "eBay", "英国站", "intl", "GB", "marketplace", "GBP", "en-GB", "#0064d2", "e", "https://www.ebay.co.uk/", "https://www.ebay.co.uk/sch/i.html?_nkw={q}", "ebay_co_uk", "optional", false, ""],
  ["kleinanzeigen", "kleinanzeigen", "Kleinanzeigen", "Kleinanzeigen", "德国二手", "intl", "DE", "c2c", "EUR", "de-DE", "#7ab51d", "K", "https://www.kleinanzeigen.de/", "https://www.kleinanzeigen.de/s-{q}/k0", "kleinanzeigen", "optional", true, "德国本地二手，相当于闲鱼"],
  ["idealo", "idealo", "idealo", "idealo", "德国比价", "intl", "DE", "compare", "EUR", "de-DE", "#f2731d", "i", "https://www.idealo.de/", "https://www.idealo.de/preisvergleich/MainSearchProductCategory.html?q={q}", "idealo", "optional", true, "比价网站，汇总多家商家报价"],
  ["etsy", "etsy", "Etsy", "Etsy", "国际", "intl", "GL", "marketplace", "USD", "en", "#f1641e", "E", "https://www.etsy.com/", "https://www.etsy.com/search?q={q}", "etsy", "optional", true, "手作与原创设计"],
  ["walmart", "walmart", "Walmart", "Walmart", "美国", "intl", "US", "retail", "USD", "en-US", "#0071dc", "W", "https://www.walmart.com/", "https://www.walmart.com/search?q={q}", "walmart", "optional", true, ""],
  ["bestbuy", "bestbuy", "Best Buy", "Best Buy", "美国", "intl", "US", "retail", "USD", "en-US", "#0046be", "B", "https://www.bestbuy.com/", "https://www.bestbuy.com/site/searchpage.jsp?st={q}", "bestbuy", "optional", true, "数码家电"],
  ["shein", "shein", "SHEIN", "SHEIN", "国际", "intl", "GL", "retail", "EUR", "en", "#e0457b", "S", "https://www.shein.com/", "https://www.shein.com/pdsearch/{q}/", "shein", "optional", true, "服饰快时尚"],
  ["otto", "otto", "OTTO", "OTTO", "德国", "intl", "DE", "retail", "EUR", "de-DE", "#d52b1e", "O", "https://www.otto.de/", "https://www.otto.de/suche/{q}/", "otto", "optional", true, ""],
  ["mercari_jp", "mercari", "Mercari", "Mercari", "日本二手", "intl", "JP", "c2c", "JPY", "ja-JP", "#ff0211", "M", "https://jp.mercari.com/", "https://jp.mercari.com/search?keyword={q}", "mercari_jp", "optional", true, ""],
  ["rakuten", "rakuten", "楽天", "Rakuten", "日本", "intl", "JP", "marketplace", "JPY", "ja-JP", "#bf0000", "R", "https://www.rakuten.co.jp/", "https://search.rakuten.co.jp/search/mall/{q}/", "rakuten", "optional", true, ""],
  ["reddit", "reddit", "Reddit", "Reddit", "国际社区", "intl", "GL", "social", "USD", "en", "#ff4500", "r", "https://www.reddit.com/", "https://www.reddit.com/search/?q={q}", "reddit", "optional", false, "海外真实讨论与避坑"],
];

const ENABLED = new Set([
  "xiaohongshu", "douyin", "pinduoduo", "taobao", "jd", "alibaba1688", "xianyu",
  "rednote", "temu", "aliexpress", "amazon_com", "amazon_de", "ebay_de", "kleinanzeigen", "idealo",
]);

export function makePlatforms(): PlatformInfo[] {
  return DEFS.map((d) => ({
    id: d[0], family: d[1], familyName: d[2], name: d[3], edition: d[4], region: d[5], country: d[6], kind: d[7],
    currency: d[8], locale: d[9], color: d[10], glyph: d[11], home: d[12], searchUrl: d[13], defaultSearchUrl: d[13],
    accountGroup: d[14], login: d[15], beta: d[16], enabled: ENABLED.has(d[0]), custom: false, note: d[17],
  }));
}

const LOGIN_INIT: Record<string, LoginStatus> = {
  xiaohongshu: "expired", douyin: "logged_out", pinduoduo: "logged_in", taobao: "logged_out", jd: "logged_in",
  alibaba1688: "logged_in", xianyu: "logged_in", weibo: "logged_out", rednote: "logged_out", tiktok: "logged_out",
  amazon_com: "logged_out", amazon_de: "logged_in", amazon_co_uk: "unknown", amazon_co_jp: "unknown", amazon_fr: "unknown",
};

export function makeLogins(platforms: PlatformInfo[]): LoginState[] {
  const now = Date.now();
  return platforms.map((p) => {
    const status: LoginStatus = LOGIN_INIT[p.id] ?? (p.login === "optional" ? "not_needed" : "unknown");
    return {
      platformId: p.id,
      status,
      flow: "idle",
      verifiedAt: status === "logged_in" ? new Date(now - (1 + (p.id.length % 7)) * 3600_000).toISOString() : status === "expired" ? new Date(now - 6 * 86400_000).toISOString() : null,
      lastWallAt: status === "expired" ? new Date(now - 2 * 3600_000).toISOString() : null,
      cookieExpires: status === "logged_in" ? new Date(now + 26 * 86400_000).toISOString() : null,
      message: status === "expired" ? "最近一次搜索遇到登录页，需要重新登录" : null,
      checkedAt: status === "logged_in" ? new Date(now - (2 + (p.id.length % 5)) * 3600_000).toISOString() : null,
      vaulted: status === "logged_in" && p.login !== "optional",
    };
  });
}

export const FX: FxRates = {
  base: "EUR",
  date: new Date().toISOString().slice(0, 10),
  source: "ecb",
  rates: { EUR: 1, CNY: 7.82, USD: 1.08, GBP: 0.852, JPY: 163.4, HKD: 8.43, KRW: 1478, CHF: 0.948, AUD: 1.64, CAD: 1.49, SGD: 1.45 },
};

export function fxConvert(amount: number, from: string, to: string): number {
  const rf = FX.rates[from] ?? 1;
  const rt = FX.rates[to] ?? 1;
  return (amount / rf) * rt;
}

export const PRESETS: AiPreset[] = [
  { id: "deepseek", name: "DeepSeek", protocol: "openai", baseUrl: "https://api.deepseek.com", models: ["deepseek-flash", "deepseek-v4-pro"], fastModel: "deepseek-flash", keyUrl: "https://platform.deepseek.com/api_keys", note: "deepseek-flash 始终指向最新 Flash；结构化任务自动关闭思考模式；推荐作为默认", region: "cn", retired: ["deepseek-chat", "deepseek-reasoner", "deepseek-v4-flash"] },
  { id: "qwen", name: "通义千问（阿里云百炼）", protocol: "openai", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", models: ["qwen3.8-max", "qwen3.7-plus", "qwen3.8-flash"], fastModel: "qwen3.8-flash", keyUrl: "https://bailian.console.aliyun.com/", note: "OpenAI 兼容模式；qwen-plus / qwen-max 为旧别名", region: "cn", retired: ["qwen-turbo", "qwen-plus", "qwen-max", "qwen-flash"] },
  { id: "kimi", name: "Kimi（月之暗面）", protocol: "openai", baseUrl: "https://api.moonshot.cn/v1", models: ["kimi-k3.1-pro", "kimi-k3", "kimi-k3.1-pro-highspeed"], fastModel: "kimi-k3", keyUrl: "https://platform.moonshot.cn/console/api-keys", note: "国际账号用 https://api.moonshot.ai/v1；K2 系列年底下线", region: "cn", retired: ["kimi-latest", "moonshot-v1-8k", "moonshot-v1-32k", "moonshot-v1-128k", "kimi-k2-turbo-preview", "kimi-k2.5", "kimi-k2.6"] },
  { id: "zhipu", name: "智谱 GLM", protocol: "openai", baseUrl: "https://open.bigmodel.cn/api/paas/v4", models: ["glm-5.3", "glm-5.2", "glm-4.7-flash"], fastModel: "glm-4.7-flash", keyUrl: "https://open.bigmodel.cn/usercenter/apikeys", note: "glm-4.7-flash 免费，可作快速模型", region: "cn", retired: ["glm-4-flash", "glm-4-plus", "glm-4-air"] },
  { id: "openai", name: "OpenAI", protocol: "openai", baseUrl: "https://api.openai.com/v1", models: ["gpt-6.1-sol", "gpt-6-astra", "gpt-6.1-terra"], fastModel: "gpt-6-luna", keyUrl: "https://platform.openai.com/api-keys", note: "需要可访问 OpenAI 的网络；gpt-5 系列 2026-12-11 停用", region: "intl", retired: ["gpt-5", "gpt-5-mini", "gpt-5-nano", "gpt-4.1-mini", "gpt-4o", "gpt-4o-mini"] },
  { id: "anthropic", name: "Anthropic Claude", protocol: "anthropic", baseUrl: "https://api.anthropic.com", models: ["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"], fastModel: "claude-haiku-5-5", keyUrl: "https://console.anthropic.com/settings/keys", note: "原生 Messages 协议；模型 ID 为固定版本，新模型发布后需手动更换", region: "intl", retired: ["claude-sonnet-4-5", "claude-haiku-4-5"] },
  { id: "gemini", name: "Google Gemini", protocol: "openai", baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", models: ["gemini-3.8-flash", "gemini-4-argon", "gemini-3.5-flash-lite"], fastModel: "gemini-3.5-flash-lite", keyUrl: "https://aistudio.google.com/apikey", note: "OpenAI 兼容端点；2.5 系列仅老账号可用", region: "intl", retired: ["gemini-2.0-flash", "gemini-2.0-flash-lite", "gemini-2.5-flash", "gemini-2.5-pro", "gemini-3-pro-preview", "gemini-3-flash-preview"] },
  { id: "ollama", name: "Ollama（本机）", protocol: "openai", baseUrl: "http://localhost:11434/v1", models: ["qwen3.5:9b", "qwen3.8:27b", "qwen3.5:4b"], keyUrl: "https://ollama.com/library", note: "本机运行，无需 API Key；小模型的校验准确度较低", region: "local", retired: ["qwen2.5:14b", "qwen2.5:7b", "llama3.1:8b"] },
  { id: "custom", name: "自定义（OpenAI 兼容）", protocol: "openai", baseUrl: "", models: [], keyUrl: "", note: "任何兼容 /chat/completions 的服务", region: "intl", retired: [] },
];

export const SKILLS: CopilotSkill[] = [
  { id: "gift", name: "礼物构思", icon: "gift", description: "懂人情世故：按关系、场合、预算给出稳妥的礼物方向，避开雷区", starter: "给我选 500 元内送爱喝茶的朋友的礼物", roles: ["consumer"] },
  { id: "compare", name: "比价顾问", icon: "scale", description: "对比同款在不同平台/站点的到手价与风险", starter: "Sony WH-1000XM5 在亚马逊德国和京东哪个更划算？", roles: ["consumer", "merchant"] },
  { id: "timing", name: "入手时机", icon: "timer", description: "结合价格走势判断现在买还是再等等", starter: "我追踪的 Switch 2 现在值得入手吗？", roles: ["consumer"] },
  { id: "industry", name: "行业判断", icon: "compass", description: "品类趋势、价格带、竞争格局的快速判断", starter: "蓝牙耳机品类在欧洲的价格带和主要玩家是什么样？", roles: ["merchant", "analyst"] },
  { id: "sourcing", name: "选品分析", icon: "box", description: "从 1688 / Alibaba 到零售平台的价差与机会", starter: "露营灯从 1688 进货到亚马逊德国卖，价差空间有多大？", roles: ["merchant", "analyst"] },
  { id: "copy", name: "商品文案", icon: "pen", description: "根据商品卖点生成多平台标题与描述", starter: "帮我为一款 40000mAh 户外电源写小红书和亚马逊标题", roles: ["merchant"] },
];

/* ---------------------------------------------------------------- 图片占位 */
const PALETTES = [
  ["#5b8cff", "#9b7bff"], ["#ff8a5b", "#ffc15b"], ["#2fd2a5", "#4aa8ff"], ["#ff5b8c", "#ff9bd2"],
  ["#7b61ff", "#3dd6ff"], ["#ffb020", "#ff6a3d"], ["#28c76f", "#a4f05b"], ["#8a95a8", "#c7cfdd"],
];

export function productImage(seed: number, shape = 0): string {
  const [a, b] = PALETTES[Math.abs(seed) % PALETTES.length]!;
  const s = Math.abs(shape + seed) % 4;
  const obj = [
    `<rect x="70" y="58" width="100" height="124" rx="26" fill="white" fill-opacity=".92"/><circle cx="120" cy="104" r="22" fill="${a}" fill-opacity=".55"/><rect x="92" y="140" width="56" height="10" rx="5" fill="${b}" fill-opacity=".5"/>`,
    `<path d="M70 150c0-40 22-80 50-80s50 40 50 80" stroke="white" stroke-width="16" fill="none" stroke-linecap="round"/><rect x="56" y="140" width="34" height="48" rx="14" fill="white"/><rect x="150" y="140" width="34" height="48" rx="14" fill="white"/>`,
    `<rect x="62" y="80" width="116" height="86" rx="18" fill="white" fill-opacity=".92"/><circle cx="120" cy="123" r="28" fill="${b}" fill-opacity=".6"/><circle cx="120" cy="123" r="14" fill="white"/><rect x="132" y="66" width="30" height="16" rx="6" fill="white"/>`,
    `<path d="M84 72h72l10 112H74z" fill="white" fill-opacity=".92"/><path d="M98 72c0-16 10-24 22-24s22 8 22 24" stroke="white" stroke-width="9" fill="none"/><circle cx="120" cy="120" r="12" fill="${a}" fill-opacity=".5"/>`,
  ][s];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="240" height="240" fill="url(#g)"/><circle cx="200" cy="40" r="70" fill="white" fill-opacity=".14"/>${obj}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/* ---------------------------------------------------------------- 随机 */
export function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

export function rng(seed: number) {
  let x = seed || 1;
  return () => {
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    return ((x >>> 0) % 100000) / 100000;
  };
}

export function uid(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return Math.random().toString(16).slice(2) + Date.now().toString(16);
  }
}

export function makeNotices(): Notice[] {
  const now = Date.now();
  const rows: (Omit<Notice, "id" | "url"> & { url?: string })[] = [
    { kind: "target_hit", level: "success", title: "Sony WH-1000XM5 达到目标价", body: "Amazon.de 当前 €249.00，低于你设定的 €259（-12.4%）", link: "/track/watchlist?id=1", url: "https://www.amazon.de/dp/B09XS7JWHH", read: false, createdAt: new Date(now - 18 * 60_000).toISOString() },
    { kind: "price_drop", level: "info", title: "Switch 2 马力欧套装降价 6.2%", body: "京东自营 ¥3,099 → ¥2,907", link: "/track/watchlist?id=2", url: "https://item.jd.com/100012043978.html", read: false, createdAt: new Date(now - 2.5 * 3600_000).toISOString() },
    { kind: "new_items", level: "info", title: "「露营灯 选品监控」发现 7 件新品", body: "最低 ¥23.8（1688），匹配度精确 5 件", link: "/track/monitors", read: false, createdAt: new Date(now - 5 * 3600_000).toISOString() },
    { kind: "login", level: "warning", title: "小红书登录已过期", body: "最近一次搜索遇到登录页，请到「账号与登录」重新登录", link: "/platforms/accounts", read: false, createdAt: new Date(now - 7 * 3600_000).toISOString() },
    { kind: "error", level: "error", title: "Joybuy 读价失败", body: "连续 3 次没有在商品页找到价格，可能是页面结构变化", link: "/track/watchlist?id=6", read: true, createdAt: new Date(now - 26 * 3600_000).toISOString() },
    { kind: "price_drop", level: "info", title: "Dyson V15 在 idealo 最低价刷新", body: "€529 → €489（-7.6%）", link: "/track/watchlist?id=4", url: "https://www.idealo.de/preisvergleich/OffersOfProduct/201846385.html", read: true, createdAt: new Date(now - 2 * 86400_000).toISOString() },
    { kind: "system", level: "info", title: "汇率已更新", body: "ECB 参考汇率 · 1 EUR = 7.82 CNY", link: "/settings/data", read: true, createdAt: new Date(now - 3 * 86400_000).toISOString() },
    { kind: "new_items", level: "info", title: "「蓝牙耳机 欧洲价格带」完成第 42 次运行", body: "中位价 €89，较上周 -3.1%", link: "/insights/market", read: true, createdAt: new Date(now - 4 * 86400_000).toISOString() },
  ];
  return rows.map((r, i) => ({ ...r, url: r.url ?? "", id: i + 1 }));
}
