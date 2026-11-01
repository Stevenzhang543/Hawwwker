<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, toAppError } from "@/api";
import { on, type Unlisten } from "@/api/events";
import type { MarketOverview } from "@/api/types";
import { usePlatformStore } from "@/stores/platforms";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { money, pct } from "@/utils/format";
import { handleMdClick, renderMarkdown } from "@/utils/markdown";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";
import CountUp from "@/components/ui/CountUp.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import Bars from "@/components/charts/Bars.vue";
import Donut from "@/components/charts/Donut.vue";
import LineChart from "@/components/charts/LineChart.vue";

const platforms = usePlatformStore();
const watchStore = useWatchStore();
const ui = useUiStore();
const app = useAppStore();
const days = ref(30);
const data = ref<MarketOverview | null>(null);
const loading = ref(true);
const scope = ref("all");
const report = ref("");
const reportId = ref<string | null>(null);
const reporting = ref(false);
const unl: Unlisten[] = [];

async function load() {
  loading.value = true;
  try {
    data.value = await api.insightsOverview(days.value);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    loading.value = false;
  }
}
onMounted(async () => {
  void load();
  if (!watchStore.monitors.length) void watchStore.loadMonitors();
  unl.push(
    await on("insight://delta", (d) => {
      if (d.reportId === reportId.value) report.value += d.delta;
    }),
    await on("insight://done", (d) => {
      if (d.reportId !== reportId.value) return;
      if (d.text) report.value = d.text;
      reporting.value = false;
      if (d.error) ui.toast("报告生成失败：" + d.error, "error");
    }),
  );
});
onBeforeUnmount(() => unl.forEach((f) => f()));
watch(days, load);

async function genReport() {
  report.value = "";
  reporting.value = true;
  try {
    const r = await api.insightsReport(scope.value, days.value);
    reportId.value = r.reportId;
  } catch (e) {
    reporting.value = false;
    ui.toast(toAppError(e).message, "error");
  }
}

const cur = computed(() => data.value?.currency ?? app.displayCurrency);
const medians = computed(() =>
  (data.value?.platformMedians ?? [])
    .slice()
    .sort((a, b) => a.median - b.median)
    .map((m) => ({ label: m.platformId, value: m.median, color: platforms.byId[m.platformId]?.color, sub: `${m.count} 件` })),
);
const bands = computed(() => (data.value?.priceBands ?? []).map((b) => ({ label: b.label, value: b.count })));
const share = computed(() => (data.value?.platformShare ?? []).map((s) => ({ label: platforms.label(s.platformId), value: s.count, color: platforms.byId[s.platformId]?.color ?? "#8a95a8" })));
const shareTotal = computed(() => share.value.reduce((s, x) => s + x.value, 0));
const daily = computed(() => [
  { key: "l", label: "记录商品", color: "#8a5cff", points: (data.value?.daily ?? []).map((d) => ({ t: new Date(d.day).getTime(), v: d.listings })) },
]);
const dayOpts = [
  { value: 7, label: "7 天" },
  { value: 30, label: "30 天" },
  { value: 90, label: "90 天" },
];
const reportHtml = computed(() => renderMarkdown(report.value));
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>市场分析</h1>
        <p>汇总搜索与追踪记录的价格数据：平台价差、价格带、平台占比与降价榜。</p>
      </div>
      <span class="spacer"></span>
      <GSegmented v-model="days" :options="dayOpts" small />
    </div>

    <div v-if="loading && !data" class="grid g4"><div v-for="i in 8" :key="i" class="skel" style="height: 110px"></div></div>
    <template v-else-if="data">
      <div class="grid g4 stagger">
        <div class="kpi glass" style="--i: 0"><span class="faint small">追踪商品</span><b><CountUp :value="data.kpis.tracked" /></b><span class="faint tiny">本期降价 {{ data.kpis.drops }} 件</span></div>
        <div class="kpi glass" style="--i: 1"><span class="faint small">平均平台价差</span><b><CountUp :value="data.kpis.avgSpreadPct" :digits="1" suffix="%" /></b><span class="faint tiny">同款最高价相对最低价</span></div>
        <div class="kpi glass" style="--i: 2"><span class="faint small">搜索 / 监控运行</span><b><CountUp :value="data.kpis.runs" /></b><span class="faint tiny">{{ data.kpis.monitors }} 个定时监控</span></div>
        <div class="kpi glass" style="--i: 3"><span class="faint small">记录商品</span><b><CountUp :value="data.kpis.listings" /></b><span class="faint tiny">近 {{ data.days }} 天</span></div>
      </div>

      <div class="grid g2">
        <section class="glass card">
          <div class="card-head"><h3>各平台中位价</h3><span class="faint tiny">已换算为 {{ cur }}</span></div>
          <Bars :data="medians" :format="(v) => money(v, cur)">
            <template #label="{ item }"><PlatformBadge :id="item.label" /></template>
          </Bars>
        </section>
        <section class="glass card">
          <div class="card-head"><h3>平台占比</h3><span class="faint tiny">按记录商品数</span></div>
          <Donut :data="share" :center="String(shareTotal)" sub="件商品" />
        </section>
      </div>

      <div class="grid g2">
        <section class="glass card">
          <div class="card-head"><h3>价格带分布</h3></div>
          <Bars :data="bands" vertical :height="190" />
        </section>
        <section class="glass card">
          <div class="card-head"><h3>每日记录商品</h3></div>
          <LineChart :series="daily" :height="200" />
        </section>
      </div>

      <section class="glass card">
        <div class="card-head"><h3>降价榜</h3><span class="faint tiny">追踪商品中相对首次记录降幅最大</span></div>
        <div v-if="!data.topDrops.length" class="faint small">暂无降价商品</div>
        <table v-else class="table">
          <tbody>
            <tr v-for="(d, i) in data.topDrops" :key="d.watchId">
              <td style="width: 28px"><b class="rank" :class="{ top: i < 3 }">{{ i + 1 }}</b></td>
              <td><PlatformBadge :id="d.platformId" /></td>
              <td style="max-width: 420px"><div class="ellipsis">{{ d.title }}</div></td>
              <td class="r strike num">{{ money(d.from, d.currency) }}</td>
              <td class="r"><span class="price amber">{{ money(d.to, d.currency) }}</span></td>
              <td class="r down num">{{ pct(d.pct) }}</td>
              <td class="r"><router-link :to="`/track/watchlist?id=${d.watchId}`" class="btn ghost sm">查看</router-link></td>
            </tr>
          </tbody>
        </table>
      </section>

      <section class="glass card report">
        <div class="card-head">
          <span class="r-ico"><Icon name="spark" :size="16" /></span>
          <h3>AI 分析报告</h3>
          <span class="spacer"></span>
          <select v-model="scope" class="select sm" style="width: 200px">
            <option value="all">全部数据</option>
            <option value="watchlist">仅追踪商品</option>
            <option v-for="m in watchStore.monitors" :key="m.id" :value="`monitor:${m.id}`">监控：{{ m.name }}</option>
          </select>
          <button v-ripple class="btn primary sm" :disabled="reporting" @click="genReport">
            <Icon :name="reporting ? 'refresh' : 'wand'" :class="{ spin: reporting }" />{{ reporting ? "生成中…" : report ? "重新生成" : "生成报告" }}
          </button>
        </div>
        <div v-if="!report && !reporting" class="faint small">基于近 {{ days }} 天的统计摘要生成（不发送任何账号或个人信息），适合周报、选品或定价决策参考。</div>
        <div v-else class="md selectable" @click="handleMdClick" v-html="reportHtml"></div>
        <span v-if="reporting" class="caret"></span>
      </section>
    </template>
  </div>
</template>

<style scoped>
.kpi {
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.kpi b {
  font-size: 28px;
  font-weight: 720;
  font-family: var(--font-num);
}
.rank {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 7px;
  background: var(--hover);
  font-size: 12px;
}
.rank.top {
  background: linear-gradient(135deg, #ffbe3d, #ff9a1f);
  color: #2b1800;
}
.r-ico {
  width: 28px;
  height: 28px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
}
.report .md {
  font-size: 14px;
  line-height: 1.75;
}
.md :deep(h2) {
  font-size: 16px;
  margin: 14px 0 6px;
}
.md :deep(h2:first-child) {
  margin-top: 0;
}
.md :deep(ul),
.md :deep(ol) {
  padding-left: 22px;
  margin: 4px 0;
}
.md :deep(strong) {
  color: var(--text);
}
.caret {
  display: inline-block;
  width: 8px;
  height: 16px;
  background: var(--brand);
  border-radius: 2px;
  animation: pulse 0.9s infinite;
}
</style>
