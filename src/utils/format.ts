import type { FxRates } from "@/api/types";

const NO_DECIMALS = new Set(["JPY", "KRW", "HUF", "CLP", "ISK", "IDR", "VND"]);
const fmtCache = new Map<string, Intl.NumberFormat>();

function moneyFmt(currency: string, compact = false): Intl.NumberFormat {
  const key = `${currency}|${compact}`;
  let f = fmtCache.get(key);
  if (!f) {
    const int = currency.endsWith("|int");
    const code = int ? currency.slice(0, -4) : currency;
    try {
      f = new Intl.NumberFormat("zh-CN", {
        style: "currency",
        currency: code,
        currencyDisplay: "narrowSymbol",
        maximumFractionDigits: int || NO_DECIMALS.has(code) ? 0 : compact ? 0 : 2,
        minimumFractionDigits: 0,
        notation: compact ? "compact" : "standard",
      });
    } catch {
      f = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 });
    }
    fmtCache.set(key, f);
  }
  return f;
}

/** 价格格式化：¥1,299 / €1.299,00 风格按货币符号显示 */
export function money(value: number | null | undefined, currency = "CNY", compact = false): string {
  if (value == null || !Number.isFinite(value)) return "价格待定";
  // 大额去掉小数，避免换算价出现 ¥1,947.18 这类噪音
  const s = (Math.abs(value) >= 1000 && !compact ? moneyFmt(currency + "|int", false) : moneyFmt(currency, compact)).format(value);
  // 人民币与日元都用 ¥，日元加注避免混淆
  if (currency === "JPY") return s.replace("¥", "JP¥");
  return s;
}

export function num(value: number | null | undefined, digits = 0): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: digits }).format(value);
}

export function compactNum(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) >= 10000) return (value / 10000).toFixed(value >= 100000 ? 0 : 1).replace(/\.0$/, "") + " 万";
  return num(value);
}

export function pct(value: number | null | undefined, digits = 1, signed = true): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const s = value.toFixed(digits).replace(/\.0+$/, "");
  return (signed && value > 0 ? "+" : "") + s + "%";
}

export function ago(iso?: string | null): string {
  if (!iso) return "—";
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return "—";
  const diff = Date.now() - t;
  const future = diff < 0;
  const s = Math.abs(diff) / 1000;
  let out: string;
  if (s < 45) out = future ? "即将" : "刚刚";
  else if (s < 3600) out = `${Math.round(s / 60)} 分钟`;
  else if (s < 86400) out = `${Math.round(s / 3600)} 小时`;
  else if (s < 86400 * 30) out = `${Math.round(s / 86400)} 天`;
  else return new Date(t).toLocaleDateString("zh-CN");
  if (out === "刚刚" || out === "即将") return out;
  return future ? `${out}后` : `${out}前`;
}

export function dateTime(iso?: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return "—";
  return d.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

export function shortDate(t: number | string): string {
  const d = new Date(t);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

export function duration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60000);
  return `${m}m ${Math.round((ms % 60000) / 1000)}s`;
}

export function interval(minutes: number): string {
  if (minutes < 60) return `${minutes} 分钟`;
  if (minutes % 1440 === 0) return `${minutes / 1440} 天`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时`;
  return `${(minutes / 60).toFixed(1)} 小时`;
}

/** 汇率换算：rates 以 base 为 1 */
export function convert(amount: number | null | undefined, from: string, to: string, fx?: FxRates | null): number | null {
  if (amount == null || !Number.isFinite(amount)) return null;
  if (from === to) return amount;
  if (!fx) return null;
  const rf = from === fx.base ? 1 : fx.rates[from];
  const rt = to === fx.base ? 1 : fx.rates[to];
  if (!rf || !rt) return null;
  return (amount / rf) * rt;
}

export const CURRENCIES: { code: string; name: string }[] = [
  { code: "CNY", name: "人民币" },
  { code: "EUR", name: "欧元" },
  { code: "USD", name: "美元" },
  { code: "GBP", name: "英镑" },
  { code: "JPY", name: "日元" },
  { code: "HKD", name: "港币" },
  { code: "KRW", name: "韩元" },
  { code: "CHF", name: "瑞士法郎" },
  { code: "AUD", name: "澳元" },
  { code: "CAD", name: "加元" },
  { code: "SGD", name: "新加坡元" },
];

export function median(xs: number[]): number {
  if (!xs.length) return 0;
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m]! : (s[m - 1]! + s[m]!) / 2;
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
