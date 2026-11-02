/**
 * v26.03 结果洞察的展示辅助：店铺类型标签、风险等级、评论摘要、综合分构成、搜索阶段。
 * 只做展示层的整理，数据全部来自后端（ResultItem.tags / shopInfo / risk / reviewInsight / finalScore）。
 */
import type { ItemTag, ResultItem, ReviewInsight, RiskInfo, ShopKind, Stage, TagTone } from "@/api/types";
import { compactNum } from "@/utils/format";

export const SHOP_KIND_LABEL: Record<ShopKind, string> = {
  flagship: "旗舰店",
  official: "官方店",
  self_operated: "平台自营",
  authorized: "专卖店",
  distributor: "专营店",
  enterprise: "企业店",
  individual: "个人卖家",
  studio: "小工作室",
  overseas: "海外代购",
  secondhand: "二手商家",
  marketplace: "第三方卖家",
  unknown: "普通店铺",
};

const SHOP_KINDS = new Set<string>(Object.keys(SHOP_KIND_LABEL));
/** 筛选里"官方渠道"一组 */
export const OFFICIAL_KINDS: ShopKind[] = ["flagship", "official", "self_operated", "authorized"];

export function shopTone(kind: string): TagTone {
  if (OFFICIAL_KINDS.includes(kind as ShopKind)) return "good";
  if (kind === "individual" || kind === "studio" || kind === "overseas" || kind === "secondhand") return "warn";
  return "neutral";
}

/** 标签色 → chip 类名 */
export const TONE_CHIP: Record<string, string> = { good: "green", neutral: "", warn: "orange", bad: "red" };

export function isShopTag(t: ItemTag): boolean {
  return SHOP_KINDS.has(t.kind);
}

/** 结果标签：店铺类型在最前（缺少时由 shopInfo 补一个），其余按后端顺序 */
export function itemTags(r: ResultItem): ItemTag[] {
  const tags = [...(r.tags ?? [])];
  const shop = tags.filter(isShopTag);
  const rest = tags.filter((t) => !isShopTag(t));
  if (!shop.length && r.shopInfo && r.shopInfo.kind !== "unknown") {
    shop.push({ kind: r.shopInfo.kind, label: r.shopInfo.label || SHOP_KIND_LABEL[r.shopInfo.kind], tone: shopTone(r.shopInfo.kind) });
  }
  return [...shop, ...rest];
}

/** 卡片上的标签：去掉与"包邮"小标签、价格判断徽章重复的项 */
export function cardTags(r: ResultItem): ItemTag[] {
  const dealShown = r.deal?.level === "lowest" || r.deal?.level === "low";
  return itemTags(r).filter((t) => !(t.kind === "free_shipping" && r.shipping === 0) && !(t.kind === "good_deal" && dealShown));
}

/** 店铺类型（筛选用）：优先 shopInfo，其次店铺标签 */
export function shopKindOf(r: ResultItem): ShopKind | null {
  if (r.shopInfo?.kind) return r.shopInfo.kind;
  const t = (r.tags ?? []).find(isShopTag);
  return (t?.kind as ShopKind | undefined) ?? null;
}

export const RISK_LEVEL: Record<RiskInfo["level"], { label: string; tone: string; icon: string }> = {
  low: { label: "低风险", tone: "green", icon: "shield" },
  medium: { label: "中风险", tone: "orange", icon: "warn" },
  high: { label: "高风险", tone: "red", icon: "warn" },
};

export function riskTitle(risk: RiskInfo): string {
  const lines = [`风险分 ${risk.score}（越高越需要谨慎）${risk.aiChecked ? " · AI 已复核" : " · 规则判断"}`];
  if (risk.summary) lines.push(risk.summary);
  for (const f of risk.factors) lines.push(`${f.weight > 0 ? "▲" : "▼"} ${f.label}${f.weight ? `（${f.weight > 0 ? "+" : ""}${f.weight}）` : ""}`);
  return lines.join("\n");
}

export function isHighRisk(r: ResultItem): boolean {
  return r.risk?.level === "high";
}

/** 结果卡上的评论小标签 */
export function reviewChip(ri: ReviewInsight | null | undefined): { text: string; title: string; tone: string } | null {
  if (!ri) return null;
  if (ri.source === "none" || (ri.unavailableReason && ri.rating == null && ri.count == null && ri.score == null)) {
    return { text: "评论不可用", title: ri.unavailableReason ?? "没有读取到评论", tone: "outline" };
  }
  const parts: string[] = [];
  if (ri.score != null) parts.push(`评论 ${ri.score}`);
  if (ri.rating != null) parts.push(`★${ri.rating.toFixed(1)}`);
  if (ri.count != null) parts.push(`${compactNum(ri.count)} 条`);
  if (ri.goodRate != null && parts.length < 3) parts.push(`好评 ${Math.round(ri.goodRate)}%`);
  const title = [
    ri.matchNote,
    ri.pros.length ? `优点：${ri.pros.slice(0, 3).join("、")}` : "",
    ri.cons.length ? `缺点：${ri.cons.slice(0, 3).join("、")}` : "",
    ri.redFlags.length ? `风险信号：${ri.redFlags.join("、")}` : "",
    ri.goodRate != null ? `好评率 ${Math.round(ri.goodRate)}%` : "",
    `读取 ${ri.sampled} 条评论${ri.aiChecked ? " · AI 已分析" : " · 等待 AI 分析"}`,
  ]
    .filter(Boolean)
    .join("\n");
  const tone = ri.redFlags.length ? "orange" : ri.score != null && ri.score >= 80 ? "green" : "blue";
  return { text: parts.join(" · ") || "已读评论", title, tone };
}

export const REVIEW_SOURCE: Record<string, string> = {
  product: "商品评论",
  "product+shop": "商品评论 + 店铺评价",
  shop: "店铺评价",
  none: "未读取",
};

/** 价格判断 → 0–100（只用于综合分构成展示） */
const DEAL_SCORE: Record<string, number> = { lowest: 96, low: 82, normal: 62, high: 35 };

/** 综合分构成（缺失的项为 null） */
export function scoreParts(r: ResultItem): { key: string; label: string; value: number | null; hint: string }[] {
  return [
    { key: "match", label: "需求匹配", value: r.provisional ? null : r.match.score, hint: "AI 逐条核对与需求的匹配程度" },
    { key: "risk", label: "风险（越低越好）", value: r.risk ? 100 - r.risk.score : null, hint: "正品、功能、二手、价格异常等风险的反向分" },
    { key: "shop", label: "店铺可信度", value: r.shopInfo ? r.shopInfo.credibility : null, hint: "店铺类型、评分、开店年限等" },
    { key: "reviews", label: "评论", value: r.reviewInsight?.score ?? null, hint: "评分、好评率与评论内容分析" },
    { key: "price", label: "价格", value: r.deal && r.deal.level !== "new" ? DEAL_SCORE[r.deal.level] ?? null : null, hint: "到手价在本机历史价格中的位置" },
  ];
}

export function scoreTone(v: number | null | undefined): string {
  if (v == null) return "";
  return v >= 80 ? "green" : v >= 60 ? "blue" : v >= 40 ? "orange" : "red";
}

/* ---------------------------------------------------------------- 搜索阶段 */
export interface PipelineStage {
  key: Stage;
  label: string;
}

/** v26.03 搜索管线（顺序即显示顺序） */
export const PIPELINE: PipelineStage[] = [
  { key: "plan", label: "理解需求" },
  { key: "platforms", label: "平台判定" },
  { key: "search", label: "搜索" },
  { key: "screen", label: "粗筛" },
  { key: "risk", label: "风险" },
  { key: "shop", label: "店铺" },
  { key: "reviews", label: "评论检索" },
  { key: "review_ai", label: "评论分析" },
  { key: "final", label: "细筛" },
  { key: "summarize", label: "总结" },
];

/** 旧版（v26.02 及以前）阶段名映射到新管线，回放旧记录时使用 */
const LEGACY: Partial<Record<Stage, Stage>> = { extract: "screen", verify: "screen", rank: "final" };

export function stageIndex(stage: Stage): number {
  if (stage === "done") return PIPELINE.length;
  const k = LEGACY[stage] ?? stage;
  const i = PIPELINE.findIndex((s) => s.key === k);
  return i < 0 ? 0 : i;
}

export function normStage(stage: Stage): Stage {
  return LEGACY[stage] ?? stage;
}
