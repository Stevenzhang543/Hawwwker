<script setup lang="ts">
/* 多折线图（手写 SVG）：单调平滑曲线、面积渐变、区间带、目标价线、悬停十字线与提示 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { shortDate } from "@/utils/format";

export interface ChartPoint {
  t: number;
  v: number;
  lo?: number;
  hi?: number;
}
export interface ChartSeries {
  key: string;
  label: string;
  color: string;
  points: ChartPoint[];
}

const props = withDefaults(
  defineProps<{ series: ChartSeries[]; height?: number; format?: (v: number) => string; area?: boolean; band?: boolean; target?: number | null }>(),
  { height: 240, area: true, band: false, target: null },
);

const box = ref<HTMLElement | null>(null);
const width = ref(600);
const hover = ref<{ x: number; t: number } | null>(null);
let ro: ResizeObserver | null = null;
const uid = "lc" + Math.random().toString(36).slice(2, 7);

onMounted(() => {
  if (!box.value) return;
  width.value = box.value.clientWidth || 600;
  ro = new ResizeObserver((e) => (width.value = Math.max(240, e[0]?.contentRect.width ?? width.value)));
  ro.observe(box.value);
});
onBeforeUnmount(() => ro?.disconnect());

const PAD = { l: 60, r: 16, t: 14, b: 28 };
function niceStep(raw: number): number {
  if (!Number.isFinite(raw) || raw <= 0) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const f = raw / p;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * p;
}
const fmt = (v: number) => (props.format ? props.format(v) : new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 }).format(v));

const all = computed(() => props.series.flatMap((s) => s.points));
const dom = computed(() => {
  const pts = all.value;
  if (!pts.length) return { t0: 0, t1: 1, y0: 0, y1: 1, step: 0.25 };
  let t0 = Infinity, t1 = -Infinity, y0 = Infinity, y1 = -Infinity;
  for (const p of pts) {
    t0 = Math.min(t0, p.t);
    t1 = Math.max(t1, p.t);
    y0 = Math.min(y0, props.band && p.lo != null ? p.lo : p.v);
    y1 = Math.max(y1, props.band && p.hi != null ? p.hi : p.v);
  }
  if (props.target != null) {
    y0 = Math.min(y0, props.target);
    y1 = Math.max(y1, props.target);
  }
  const pad = (y1 - y0) * 0.08 || Math.abs(y1) * 0.1 || 1;
  if (t1 === t0) t1 = t0 + 1;
  const lo = Math.max(0, y0 - pad), hi = y1 + pad;
  const step = niceStep((hi - lo) / 4);
  return { t0, t1, y0: Math.floor(lo / step) * step, y1: Math.ceil(hi / step) * step, step };
});
const iw = computed(() => width.value - PAD.l - PAD.r);
const ih = computed(() => props.height - PAD.t - PAD.b);
const X = (t: number) => PAD.l + ((t - dom.value.t0) / (dom.value.t1 - dom.value.t0)) * iw.value;
const Y = (v: number) => PAD.t + (1 - (v - dom.value.y0) / (dom.value.y1 - dom.value.y0)) * ih.value;

/** 单调三次插值路径（不会过冲） */
function monotone(xs: number[], ys: number[]): string {
  const n = xs.length;
  if (!n) return "";
  const X0 = (i: number) => xs[i]!;
  const Y0 = (i: number) => ys[i]!;
  if (n < 3) return xs.map((x, i) => `${i ? "L" : "M"}${x.toFixed(1)},${Y0(i).toFixed(1)}`).join("");
  const dx: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    dx.push(X0(i + 1) - X0(i));
    m.push((Y0(i + 1) - Y0(i)) / (dx[i]! || 1));
  }
  const t: number[] = [m[0]!];
  for (let i = 1; i < n - 1; i++) {
    const a = m[i - 1]!, b = m[i]!, da = dx[i - 1]!, db = dx[i]!;
    t.push(a * b <= 0 ? 0 : (3 * (da + db)) / ((2 * db + da) / a + (db + 2 * da) / b));
  }
  t.push(m[n - 2]!);
  let d = `M${X0(0).toFixed(1)},${Y0(0).toFixed(1)}`;
  for (let i = 0; i < n - 1; i++) {
    const h = dx[i]! / 3;
    d += `C${(X0(i) + h).toFixed(1)},${(Y0(i) + h * t[i]!).toFixed(1)} ${(X0(i + 1) - h).toFixed(1)},${(Y0(i + 1) - h * t[i + 1]!).toFixed(1)} ${X0(i + 1).toFixed(1)},${Y0(i + 1).toFixed(1)}`;
  }
  return d;
}

const paths = computed(() =>
  props.series.map((s) => {
    const pts = [...s.points].sort((a, b) => a.t - b.t);
    const xs = pts.map((p) => X(p.t));
    const ys = pts.map((p) => Y(p.v));
    const line = monotone(xs, ys);
    const area = pts.length ? `${line}L${xs[xs.length - 1]!.toFixed(1)},${(PAD.t + ih.value).toFixed(1)}L${xs[0]!.toFixed(1)},${(PAD.t + ih.value).toFixed(1)}Z` : "";
    let band = "";
    if (props.band && pts.every((p) => p.lo != null && p.hi != null)) {
      const top = pts.map((p) => `${X(p.t).toFixed(1)},${Y(p.hi!).toFixed(1)}`);
      const bot = pts.map((p) => `${X(p.t).toFixed(1)},${Y(p.lo!).toFixed(1)}`).reverse();
      band = `M${top.join("L")}L${bot.join("L")}Z`;
    }
    return { ...s, pts, line, area, band, last: pts[pts.length - 1] };
  }),
);

const yTicks = computed(() => {
  const { y0, y1, step } = dom.value;
  const out: number[] = [];
  for (let v = y0; v <= y1 + step / 2 && out.length < 9; v += step) out.push(Math.round(v * 1e6) / 1e6);
  return out;
});
const xTicks = computed(() => {
  const { t0, t1 } = dom.value;
  const n = Math.max(2, Math.min(7, Math.floor(iw.value / 110)));
  return Array.from({ length: n }, (_, i) => t0 + ((t1 - t0) * i) / (n - 1));
});

function onMove(e: PointerEvent) {
  const r = (e.currentTarget as SVGElement).getBoundingClientRect();
  const x = Math.min(Math.max(e.clientX - r.left, PAD.l), PAD.l + iw.value);
  const t = dom.value.t0 + ((x - PAD.l) / iw.value) * (dom.value.t1 - dom.value.t0);
  hover.value = { x, t };
}
const hoverRows = computed(() => {
  if (!hover.value) return [];
  return paths.value
    .map((s) => {
      let best: ChartPoint | null = null;
      for (const p of s.pts) if (!best || Math.abs(p.t - hover.value!.t) < Math.abs(best.t - hover.value!.t)) best = p;
      return best ? { key: s.key, label: s.label, color: s.color, p: best } : null;
    })
    .filter((x): x is { key: string; label: string; color: string; p: ChartPoint } => !!x);
});
const hoverX = computed(() => { const h = hoverRows.value[0]; return h ? X(h.p.t) : 0; });
const tipLeft = computed(() => (hoverX.value > width.value * 0.6 ? hoverX.value - 210 : hoverX.value + 14));
</script>

<template>
  <div ref="box" class="lc">
    <svg :width="width" :height="height" @pointermove="onMove" @pointerleave="hover = null">
      <defs>
        <linearGradient v-for="s in paths" :id="`${uid}-${s.key}`" :key="s.key" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" :stop-color="s.color" stop-opacity=".28" />
          <stop offset="1" :stop-color="s.color" stop-opacity="0" />
        </linearGradient>
      </defs>
      <g class="grid">
        <line v-for="(v, i) in yTicks" :key="'y' + i" :x1="PAD.l" :x2="width - PAD.r" :y1="Y(v)" :y2="Y(v)" />
      </g>
      <g class="axis">
        <text v-for="(v, i) in yTicks" :key="'yt' + i" :x="PAD.l - 8" :y="Y(v) + 4" text-anchor="end">{{ fmt(v) }}</text>
        <text v-for="(t, i) in xTicks" :key="'xt' + i" :x="X(t)" :y="height - 8" :text-anchor="i === 0 ? 'start' : i === xTicks.length - 1 ? 'end' : 'middle'">{{ shortDate(t) }}</text>
      </g>
      <g v-for="s in paths" :key="s.key">
        <path v-if="band && s.band" :d="s.band" :fill="s.color" opacity=".12" />
        <path v-if="area && paths.length === 1" :d="s.area" :fill="`url(#${uid}-${s.key})`" class="area" />
        <path :d="s.line" fill="none" :stroke="s.color" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" class="line" />
        <circle v-if="s.last" :cx="X(s.last.t)" :cy="Y(s.last.v)" r="4" :fill="s.color" class="end" />
      </g>
      <g v-if="target != null" class="target">
        <line :x1="PAD.l" :x2="width - PAD.r" :y1="Y(target)" :y2="Y(target)" />
        <text :x="width - PAD.r - 4" :y="Y(target) - 6" text-anchor="end">目标 {{ fmt(target) }}</text>
      </g>
      <g v-if="hover && hoverRows.length" class="hover">
        <line :x1="hoverX" :x2="hoverX" :y1="PAD.t" :y2="PAD.t + ih" />
        <circle v-for="r in hoverRows" :key="r.key" :cx="hoverX" :cy="Y(r.p.v)" r="5" :stroke="r.color" />
      </g>
    </svg>
    <div v-if="hover && hoverRows.length" class="tip glass-3" :style="{ left: tipLeft + 'px' }">
      <div class="tip-date">{{ new Date(hoverRows[0]!.p.t).toLocaleDateString("zh-CN") }}</div>
      <div v-for="r in hoverRows" :key="r.key" class="tip-row">
        <i :style="{ background: r.color }"></i>
        <span class="ellipsis">{{ r.label }}</span>
        <b class="num">{{ fmt(r.p.v) }}</b>
      </div>
      <div v-if="band && hoverRows.length === 1 && hoverRows[0]!.p.lo != null" class="tip-sub">区间 {{ fmt(hoverRows[0]!.p.lo!) }} – {{ fmt(hoverRows[0]!.p.hi!) }}</div>
    </div>
  </div>
</template>

<style scoped>
.lc {
  position: relative;
  width: 100%;
}
svg {
  display: block;
  overflow: visible;
}
.grid line {
  stroke: var(--line);
  stroke-dasharray: 3 4;
}
.axis text {
  fill: var(--text-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.line {
  stroke-dasharray: 3000;
  stroke-dashoffset: 3000;
  animation: drawline 1.2s var(--ease-out) forwards;
}
.area {
  animation: fadein 900ms 300ms both;
}
.end {
  animation: popin 420ms 900ms var(--spring) both;
  transform-box: fill-box;
  transform-origin: center;
}
@keyframes drawline {
  to {
    stroke-dashoffset: 0;
  }
}
@keyframes fadein {
  from {
    opacity: 0;
  }
}
.target line {
  stroke: var(--amber);
  stroke-width: 1.5;
  stroke-dasharray: 6 5;
}
.target text {
  fill: var(--amber-ink);
  font-size: 11px;
  font-weight: 600;
}
.hover line {
  stroke: var(--text-3);
  stroke-width: 1;
  stroke-dasharray: 2 3;
}
.hover circle {
  fill: var(--bg-solid);
  stroke-width: 2.5;
}
.tip {
  position: absolute;
  top: 8px;
  width: 196px;
  padding: 10px 12px;
  border-radius: 12px;
  pointer-events: none;
  font-size: 12px;
  z-index: 5;
}
.tip-date {
  color: var(--text-3);
  margin-bottom: 4px;
}
.tip-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}
.tip-row i {
  width: 8px;
  height: 8px;
  border-radius: 3px;
  flex: none;
}
.tip-row span {
  flex: 1;
  color: var(--text-2);
}
.tip-sub {
  margin-top: 4px;
  color: var(--text-3);
}
</style>
