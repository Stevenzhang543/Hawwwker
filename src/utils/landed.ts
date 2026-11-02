import type { ResultItem } from "@/api/types";
import { money } from "./format";

/** 有运费或券信息时才把价格称为"到手价" */
export function hasLandedInfo(r: Pick<ResultItem, "shipping" | "couponPrice">): boolean {
  return r.shipping != null || r.couponPrice != null;
}

/** 到手价构成的小标签：包邮 / 含运费 / 券后 */
export function landedChips(r: ResultItem): { text: string; tone: string; title: string }[] {
  const out: { text: string; tone: string; title: string }[] = [];
  if (r.couponPrice != null && r.price != null && r.couponPrice < r.price) {
    out.push({ text: "券后", tone: "purple", title: `标价 ${money(r.price, r.currency)}，券后 / 满减后 ${money(r.couponPrice, r.currency)}` });
  }
  if (r.shipping === 0) out.push({ text: "包邮", tone: "green", title: "商品页标注包邮 / 免运费" });
  else if (r.shipping != null && r.shipping > 0) out.push({ text: `含运费 ${money(r.shipping, r.currency)}`, tone: "", title: "到手价已加上运费" });
  return out;
}

/** 按显示币种比较用的到手价（缺失时退回标价换算） */
export function landedDisplay(r: ResultItem, toDisplay: (v: number | null | undefined, cur: string) => number | null): number | null {
  return r.displayLanded ?? (r.landedPrice != null ? toDisplay(r.landedPrice, r.currency) : null) ?? r.displayPrice ?? toDisplay(r.price, r.currency);
}

export const isUrl = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());
