/**
 * 浏览器预览用：为模拟结果生成 v26.03 的店铺 / 标签 / 风险 / 评论 / 综合分，规则尽量贴近后端的输出形态。
 */
import type { ItemTag, ResultItem, ReviewInsight, RiskFactor, RiskInfo, ShopInfo, ShopKind } from "./types";

type Profile = [ShopKind, string];

/** 各平台常见的店铺类型（按商品序号轮换） */
const PROFILES: Record<string, Profile[]> = {
  taobao: [["flagship", "{brand}官方旗舰店"], ["distributor", "优品数码专营店"], ["individual", "小李的数码店（个人）"], ["studio", "声学小工作室"], ["overseas", "海外直邮 全球购"]],
  jd: [["self_operated", "京东自营"], ["flagship", "{brand}京东自营旗舰店"], ["authorized", "{brand}授权专卖店"], ["distributor", "数码配件专营店"]],
  pinduoduo: [["official", "{brand}品牌官方店"], ["enterprise", "鑫源电子企业店"], ["individual", "个人店铺·好物分享"], ["distributor", "百亿补贴 数码专营"]],
  xianyu: [["individual", "闲鱼用户·阿杰"], ["secondhand", "回收商家·极客二手"], ["individual", "闲鱼用户·Momo"]],
  douyin: [["official", "{brand}官方旗舰店"], ["individual", "好物推荐官"], ["enterprise", "源头工厂企业店"]],
  xiaohongshu: [["official", "{brand}官方店"], ["studio", "手作小工作室"], ["individual", "博主自营小店"]],
  alibaba1688: [["enterprise", "深圳市声科电子有限公司"], ["enterprise", "源头工厂·东莞"], ["distributor", "义乌小商品批发"]],
  amazon_de: [["self_operated", "Amazon (Verkauf und Versand durch Amazon)"], ["marketplace", "TechStore DE"], ["overseas", "Global Seller CN"], ["official", "{brand} Store"]],
  amazon_com: [["self_operated", "Ships from and sold by Amazon"], ["marketplace", "BestAudio LLC"], ["overseas", "Global Seller CN"], ["official", "{brand} Store"]],
  ebay_de: [["marketplace", "Top-Rated Händler · audio-world"], ["individual", "Privat"], ["official", "{brand} Offizieller Shop"]],
  idealo: [["marketplace", "MediaMarkt"], ["marketplace", "Cyberport"], ["marketplace", "notebooksbilliger.de"]],
  kleinanzeigen: [["individual", "Privat · Berlin"], ["individual", "Privat · München"]],
  temu: [["overseas", "Temu 跨境卖家"], ["marketplace", "Local Warehouse"]],
  aliexpress: [["overseas", "Shenzhen Audio Store"], ["official", "{brand} Official Store"]],
};

const KIND_LABEL: Record<ShopKind, string> = {
  flagship: "旗舰店", official: "官方店", self_operated: "平台自营", authorized: "专卖店", distributor: "专营店", enterprise: "企业店",
  individual: "个人卖家", studio: "小工作室", overseas: "海外代购", secondhand: "二手商家", marketplace: "第三方卖家", unknown: "普通店铺",
};
const BASE_CRED: Record<ShopKind, number> = {
  self_operated: 94, flagship: 92, official: 90, authorized: 84, distributor: 70, enterprise: 66, marketplace: 68,
  studio: 58, individual: 50, secondhand: 52, overseas: 46, unknown: 55,
};
const BASE_RISK: Record<ShopKind, number> = {
  self_operated: 6, flagship: 8, official: 10, authorized: 14, distributor: 26, enterprise: 28, marketplace: 24,
  studio: 30, individual: 38, secondhand: 40, overseas: 42, unknown: 30,
};
const GOOD: ShopKind[] = ["flagship", "official", "self_operated", "authorized"];

export function mockShop(platformId: string, k: number, brand: string | null, seed: () => number): ShopInfo {
  const list = PROFILES[platformId] ?? [["marketplace", "第三方卖家"], ["individual", "个人卖家"]];
  const [kind, tpl] = list[k % list.length]!;
  const name = tpl.replace("{brand}", brand ?? "品牌");
  const cn = /[一-龥]/.test(name) && !platformId.startsWith("amazon") && !platformId.startsWith("ebay");
  const scores: Record<string, number> = {};
  if (["taobao", "pinduoduo", "douyin"].includes(platformId)) {
    scores["描述"] = round1(4.5 + seed() * 0.45);
    scores["服务"] = round1(4.4 + seed() * 0.5);
    scores["物流"] = round1(4.5 + seed() * 0.45);
  } else if (platformId === "jd") {
    scores["好评率"] = Math.round(95 + seed() * 4.5);
  } else if (platformId.startsWith("ebay") || platformId.startsWith("amazon")) {
    scores["Positive feedback %"] = Math.round(92 + seed() * 7.5);
  }
  const years = kind === "individual" || kind === "secondhand" ? null : Math.round((0.5 + seed() * 11) * 10) / 10;
  const credibility = clamp(Math.round(BASE_CRED[kind] + (seed() - 0.5) * 8 - (years != null && years < 1 ? 10 : 0)), 20, 99);
  const notes: string[] = [];
  if (GOOD.includes(kind)) notes.push(kind === "self_operated" ? "平台自营发货，售后由平台负责" : "品牌官方或授权渠道，正品保障更有把握");
  if (kind === "individual") notes.push(cn ? "个人卖家，没有店铺资质认证，注意核对成色与发票" : "Privatverkauf：keine Gewährleistung / Rückgabe");
  if (kind === "studio") notes.push("小工作室手作或改装，品控差异较大，适合看重个性的买家");
  if (kind === "overseas") notes.push("海外 / 跨境发货，物流慢且国内保修可能不适用");
  if (kind === "secondhand") notes.push("二手回收商家，通常提供验机报告，注意保修期");
  if (years != null) notes.push(years >= 5 ? `开店 ${Math.round(years)} 年，经营稳定` : years < 1 ? "开店不足 1 年" : `开店 ${Math.round(years)} 年`);
  return {
    name, kind, label: KIND_LABEL[kind], rating: Object.keys(scores).length ? null : round1(4.2 + seed() * 0.75), scores, years,
    followers: kind === "individual" ? null : Math.round(2000 + seed() * 1_800_000), credibility, notes,
  };
}

/** 风险：店铺基线 + 价格异常 + 仿品关键词 + 二手 / 配件 */
export function mockRisk(it: ResultItem, shop: ShopInfo, median: number | null): RiskInfo {
  const f: RiskFactor[] = [];
  let score = BASE_RISK[shop.kind];
  f.push({ kind: "shop", label: `${shop.label}（店铺可信度 ${shop.credibility}）`, weight: score - 20 });
  if (/原单|高仿|1:1|复刻|同款|A货|超A/i.test(it.title)) {
    score += 38;
    f.push({ kind: "counterfeit", label: "标题含「原单 / 同款」等仿品常用词", weight: 38 });
  }
  const p = it.displayLanded ?? it.displayPrice;
  if (median && p && p < median * 0.55 && it.condition !== "used" && it.match.verdict !== "mismatch") {
    score += 24;
    f.push({ kind: "price_anomaly", label: `到手价比同款中位价低 ${Math.round((1 - p / median) * 100)}%，明显偏低`, weight: 24 });
  }
  if (it.condition === "used") {
    score += 14;
    f.push({ kind: "used", label: "二手 / 翻新商品，成色与保修需单独确认", weight: 14 });
  }
  if (it.match.flags.includes("accessory")) {
    score += 10;
    f.push({ kind: "accessory", label: "配件而非商品本身", weight: 10 });
  }
  if ((it.sales ?? 0) < 30 && shop.kind !== "self_operated") {
    score += 8;
    f.push({ kind: "no_reviews", label: "销量很低，参考信息少", weight: 8 });
  }
  if (GOOD.includes(shop.kind)) f.push({ kind: "shop", label: "官方 / 自营渠道，正品有保障", weight: -12 });
  score = clamp(score + (GOOD.includes(shop.kind) ? -6 : 0), 2, 98);
  const level: RiskInfo["level"] = score >= 60 ? "high" : score >= 30 ? "medium" : "low";
  const summary =
    level === "high"
      ? f.some((x) => x.kind === "counterfeit") ? "疑似仿品：标题用词与价格都不像正品，不建议购买" : "风险较高：价格或店铺信息异常，下单前请仔细核实"
      : level === "medium" ? "风险一般：渠道不是官方，注意核对型号、成色与售后" : "风险低：正规渠道，信息完整";
  return { score, level, factors: f.filter((x) => x.weight !== 0), summary, aiChecked: true };
}

const PROS_CN = ["降噪效果明显", "佩戴舒适，久戴不夹头", "续航很长，出差够用", "音质通透，低频有力", "做工扎实"];
const CONS_CN = ["耳罩夏天有点闷", "APP 偶尔断连", "包装简陋", "价格偏高"];
const SNIP_CN = [
  "降噪真的强，地铁里基本听不到报站，戴了一周很满意。",
  "和专柜对比过，序列号官网可查，正品放心。",
  "物流很快，第二天就到了，客服回复也及时。",
  "音质不错，就是夏天戴久了有点热。",
];
const SNIP_DE = [
  "Hervorragende Geräuschunterdrückung, im Zug absolut ruhig.",
  "Sitzt bequem, Akku hält ewig. Klare Kaufempfehlung.",
  "Verpackung war beschädigt, Kopfhörer aber einwandfrei.",
];

/** 评论：raw = 只读到原始数据（等待 AI 分析），否则附带 AI 分析结果 */
export function mockReviews(it: ResultItem, shop: ShopInfo, seed: () => number, raw: boolean): ReviewInsight {
  const now = new Date().toISOString();
  const locked = it.platformId === "pinduoduo" && shop.kind !== "official";
  if (locked || it.platformId === "xiaohongshu") {
    return {
      sampled: 0, pros: [], cons: [], redFlags: [], snippets: [], source: "none", aiChecked: !raw, fetchedAt: now,
      unavailableReason: "需要登录才能查看评论：到「平台 · 账号与登录」登录后，下次搜索会自动读取", score: null, rating: null, count: null, goodRate: null,
    };
  }
  const fake = /原单|高仿|1:1|复刻|同款/.test(it.title);
  const de = /^(amazon_de|ebay_de|idealo|kleinanzeigen)$/.test(it.platformId);
  const rating = round1(fake ? 3.6 + seed() * 0.5 : 4.4 + seed() * 0.55);
  const count = Math.round(fake ? 40 + seed() * 300 : 300 + seed() * 48000);
  const goodRate = Math.round(fake ? 72 + seed() * 10 : 93 + seed() * 6.5);
  const sampled = Math.round(12 + seed() * 18);
  const snippets = (de ? SNIP_DE : SNIP_CN).slice(0, 3);
  const base: ReviewInsight = {
    rating, count, goodRate, sampled, pros: [], cons: [], redFlags: [], matchNote: null, score: null,
    source: shop.kind === "individual" ? "product" : "product+shop", snippets: fake ? ["和正品放在一起对比，logo 位置不一样，做工粗糙。", "降噪几乎没有效果，和描述不符。", ...snippets.slice(0, 1)] : snippets,
    shopNote: shop.kind === "individual" ? null : `店铺近 90 天评价 ${goodRate}% 好评，${goodRate > 95 ? "售后响应快" : "有少量发货慢的反馈"}`,
    unavailableReason: null, aiChecked: false, fetchedAt: now,
  };
  if (raw) return base;
  const pick = (arr: string[], n: number) => arr.slice(Math.floor(seed() * 2), Math.floor(seed() * 2) + n);
  const score = clamp(Math.round((rating - 3) * 30 + (goodRate - 85) * 1.6 + (count > 5000 ? 8 : 0) - (fake ? 25 : 0)), 8, 98);
  return {
    ...base,
    aiChecked: true,
    score,
    pros: fake ? ["价格便宜"] : de ? ["Starke Geräuschunterdrückung", "Bequemer Sitz", "Lange Akkulaufzeit"] : pick(PROS_CN, 3),
    cons: fake ? ["做工粗糙", "降噪效果差"] : de ? ["Verpackung teils beschädigt"] : pick(CONS_CN, 2),
    redFlags: fake ? ["多条评论称与正品做工不同", "有买家反映序列号查询不到"] : [],
    matchNote: fake ? "评论与「正品、降噪」的需求明显不符" : `${Math.round(sampled * 0.6)} 条评论提到降噪与佩戴，与你的需求一致`,
  };
}

/** 标签：店铺类型在前，另加包邮 / 好价 / 高销量 / 好评 / 高风险 */
export function mockTags(it: ResultItem, shop: ShopInfo, risk: RiskInfo | null, reviews: ReviewInsight | null): ItemTag[] {
  const tone = GOOD.includes(shop.kind) ? "good" : ["individual", "studio", "overseas", "secondhand"].includes(shop.kind) ? "warn" : "neutral";
  const tags: ItemTag[] = [{ kind: shop.kind, label: shop.label, tone }];
  if (it.shipping === 0) tags.push({ kind: "free_shipping", label: "包邮", tone: "good" });
  if (it.deal?.level === "lowest" || it.deal?.level === "low") tags.push({ kind: "good_deal", label: "好价", tone: "good" });
  if ((it.sales ?? 0) >= 5000) tags.push({ kind: "high_sales", label: "高销量", tone: "neutral" });
  if (reviews?.score != null && reviews.score >= 85) tags.push({ kind: "well_reviewed", label: "口碑好", tone: "good" });
  if (risk?.level === "high") tags.push({ kind: "risk", label: "谨慎购买", tone: "bad" });
  return tags;
}

const DEAL: Record<string, number> = { lowest: 96, low: 82, normal: 62, high: 35 };

export function mockFinal(it: ResultItem): number {
  const parts: [number, number][] = [[it.match.score, 0.4]];
  if (it.risk) parts.push([100 - it.risk.score, 0.2]);
  if (it.shopInfo) parts.push([it.shopInfo.credibility, 0.15]);
  if (it.reviewInsight?.score != null) parts.push([it.reviewInsight.score, 0.15]);
  if (it.deal && it.deal.level !== "new") parts.push([DEAL[it.deal.level] ?? 60, 0.1]);
  const w = parts.reduce((a, [, x]) => a + x, 0);
  return Math.round(parts.reduce((a, [v, x]) => a + v * x, 0) / w);
}

function round1(v: number) {
  return Math.round(v * 10) / 10;
}
function clamp(v: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(hi, v));
}
