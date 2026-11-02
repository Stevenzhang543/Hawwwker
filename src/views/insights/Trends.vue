<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, toAppError } from "@/api";
import type { TrendSeries } from "@/api/types";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { money, pct } from "@/utils/format";
import GSegmented from "@/components/ui/GSegmented.vue";
import GSwitch from "@/components/ui/GSwitch.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import LineChart from "@/components/charts/LineChart.vue";

const store = useWatchStore();
const ui = useUiStore();
const days = ref(30);
const watchIds = ref<number[]>([]);
const monitorIds = ref<number[]>([]);
const series = ref<TrendSeries[]>([]);
const loading = ref(false);
const normalize = ref(false);
const COLORS = ["#3d7bff", "#ff9f1a", "#2fd27a", "#8a5cff", "#ff5a5f", "#13c2c2", "#e056fd", "#7f8c8d"];

onMounted(async () => {
  await Promise.all([store.loaded ? null : store.load(), store.monitors.length ? null : store.loadMonitors()]);
  autoPick();
  void load();
});

/** 没有选择时：自动选历史最多的 3 件追踪商品（没有追踪就选监控），保证图表不空。 */
function autoPick() {
  const ws = [...store.items].filter((w) => w.spark.length > 0).sort((a, b) => b.spark.length - a.spark.length).slice(0, 3);
  watchIds.value = ws.map((w) => w.id);
  if (!ws.length) monitorIds.value = store.monitors.filter((m) => m.runCount > 0).slice(0, 3).map((m) => m.id);
}

async function load() {
  if (!watchIds.value.length && !monitorIds.value.length) {
    series.value = [];
    return;
  }
  loading.value = true;
  try {
    series.value = await api.insightsTrends(watchIds.value, monitorIds.value, days.value);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    loading.value = false;
  }
}
watch([days, watchIds, monitorIds], load, { deep: true });

function toggle(kind: "w" | "m", id: number) {
  const list = kind === "w" ? watchIds : monitorIds;
  const s = new Set(list.value);
  if (s.has(id)) s.delete(id);
  else if (s.size < 8) s.add(id);
  else ui.toast("最多同时对比 8 条曲线", "warn");
  list.value = [...s];
}

/** 数据较少：每条曲线都不足 3 个点 */
const sparse = computed(() => series.value.length > 0 && series.value.every((s) => s.points.length < 3));
const currencies = computed(() => [...new Set(series.value.map((s) => s.currency))]);
const mixed = computed(() => currencies.value.length > 1);
const useIndex = computed(() => normalize.value || mixed.value);
const chart = computed(() =>
  series.value.map((s, i) => {
    const base = s.points[0]?.median || 1;
    return {
      key: s.key,
      label: s.label,
      color: COLORS[i % COLORS.length]!,
      points: s.points.map((p) => ({
        t: new Date(p.t).getTime(),
        v: useIndex.value ? (p.median / base) * 100 : p.median,
        lo: useIndex.value ? (p.min / base) * 100 : p.min,
        hi: useIndex.value ? (p.max / base) * 100 : p.max,
      })),
    };
  }),
);
const fmt = (v: number) => (useIndex.value ? v.toFixed(0) : money(v, currencies.value[0] ?? "CNY"));
const summary = computed(() =>
  series.value.map((s, i) => {
    const f = s.points[0]?.median ?? 0;
    const l = s.points[s.points.length - 1]?.median ?? 0;
    const lo = Math.min(...s.points.map((p) => p.min));
    return { s, color: COLORS[i % COLORS.length]!, first: f, last: l, change: f ? ((l - f) / f) * 100 : 0, lo };
  }),
);
const dayOpts = [
  { value: 7, label: "7 天" },
  { value: 30, label: "30 天" },
  { value: 90, label: "90 天" },
];
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>价格走势</h1>
        <p>选择追踪商品或定时监控，对比它们的价格曲线（监控取每天的中位价，阴影为最低–最高区间）。</p>
      </div>
      <span class="spacer"></span>
      <GSegmented v-model="days" :options="dayOpts" small />
    </div>

    <div class="layout">
      <aside class="picker glass">
        <p class="eyebrow">追踪商品</p>
        <button v-for="w in store.items" :key="w.id" class="pk" :class="{ on: watchIds.includes(w.id) }" @click="toggle('w', w.id)">
          <span class="ck"><Icon name="check" :size="11" :stroke="3" /></span>
          <PlatformBadge :id="w.platformId" :show-name="false" />
          <span class="ellipsis">{{ w.title }}</span>
        </button>
        <p class="eyebrow" style="margin-top: 12px">定时监控</p>
        <button v-for="m in store.monitors" :key="m.id" class="pk" :class="{ on: monitorIds.includes(m.id) }" @click="toggle('m', m.id)">
          <span class="ck"><Icon name="check" :size="11" :stroke="3" /></span>
          <Icon name="clock" :size="15" class="faint" />
          <span class="ellipsis">{{ m.name }}</span>
        </button>
      </aside>

      <div class="stack" style="min-width: 0; flex: 1">
        <section class="glass card">
          <div class="card-head">
            <h3>价格曲线</h3>
            <span v-if="mixed" class="chip orange">多币种，已切换为指数（首日 = 100）</span>
            <span v-if="sparse" class="chip" title="每条曲线不足 3 个价格点；追踪商品会按检查间隔持续记录，几天后曲线会更完整">数据较少 · 继续追踪几天会更完整</span>
            <span class="spacer"></span>
            <span class="faint tiny">指数化</span>
            <GSwitch v-model="normalize" small :disabled="mixed" />
          </div>
          <div v-if="loading && !series.length" class="skel" style="height: 320px"></div>
          <div v-else-if="!series.length" class="empty">
            <div class="empty-icon"><Icon name="trend" /></div>
            <template v-if="store.items.length || store.monitors.length">
              <p>在左侧选择要对比的商品或监控</p>
              <button v-ripple class="btn sm" @click="autoPick"><Icon name="wand" />自动选择历史最多的 3 条</button>
            </template>
            <template v-else>
              <p>还没有价格数据：先在搜索结果上点「追踪」，或创建一个定时监控。</p>
              <router-link to="/track/watchlist?add=1" class="btn sm primary"><Icon name="plus" />追踪一件商品</router-link>
            </template>
          </div>
          <LineChart v-else :series="chart" :height="320" :format="fmt" :area="false" :band="chart.length === 1" />
        </section>

        <section v-if="summary.length" class="glass card">
          <table class="table">
            <thead><tr><th>曲线</th><th class="r">期初</th><th class="r">最新</th><th class="r">变化</th><th class="r">区间最低</th></tr></thead>
            <tbody>
              <tr v-for="r in summary" :key="r.s.key">
                <td><span class="row" style="gap: 8px"><i class="sw" :style="{ background: r.color }"></i><span class="ellipsis" style="max-width: 360px">{{ r.s.label }}</span></span></td>
                <td class="r num">{{ money(r.first, r.s.currency) }}</td>
                <td class="r"><span class="price">{{ money(r.last, r.s.currency) }}</span></td>
                <td class="r num" :class="r.change < 0 ? 'down' : r.change > 0 ? 'up' : ''">{{ pct(r.change) }}</td>
                <td class="r num">{{ money(r.lo, r.s.currency) }}</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
.picker {
  width: 280px;
  flex: none;
  padding: 14px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: calc(100vh - 200px);
  overflow-y: auto;
  position: sticky;
  top: 0;
}
.picker .eyebrow {
  padding: 0 8px 6px;
}
.pk {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: none;
  border-radius: 10px;
  background: transparent;
  cursor: pointer;
  text-align: left;
  font-size: 12.5px;
  color: var(--text-2);
  min-width: 0;
}
.pk:hover {
  background: var(--hover);
}
.pk.on {
  color: var(--text);
  background: var(--info-bg);
}
.ck {
  width: 17px;
  height: 17px;
  flex: none;
  border-radius: 5px;
  border: 1.5px solid var(--line-2);
  display: grid;
  place-items: center;
  color: transparent;
  transition: all 260ms var(--spring);
}
.on .ck {
  background: var(--brand-grad);
  border-color: transparent;
  color: #fff;
}
.sw {
  width: 10px;
  height: 10px;
  border-radius: 3px;
  flex: none;
}
</style>
