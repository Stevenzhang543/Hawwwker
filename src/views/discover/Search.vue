<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { api, openExternal, toAppError } from "@/api";
import type { Region, ResultItem, SearchPlan, Verdict } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { useSearchStore } from "@/stores/search";
import { usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { useWatchStore } from "@/stores/watch";
import { useCopilotStore } from "@/stores/copilot";
import { useActions } from "@/composables/actions";
import { EXAMPLES, VERDICT, RUN_STATUS, CONDITION } from "@/utils/labels";
import { ago, money } from "@/utils/format";
import CommandBox from "@/components/search/CommandBox.vue";
import PlatformPicker from "@/components/search/PlatformPicker.vue";
import LiveActivity from "@/components/search/LiveActivity.vue";
import PlanCard from "@/components/search/PlanCard.vue";
import SummaryCard from "@/components/search/SummaryCard.vue";
import ResultCard from "@/components/search/ResultCard.vue";
import ResultDetail from "@/components/search/ResultDetail.vue";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import PriceTag from "@/components/ui/PriceTag.vue";

const app = useAppStore();
const search = useSearchStore();
const platforms = usePlatformStore();
const ui = useUiStore();
const watchStore = useWatchStore();
const copilot = useCopilotStore();
const route = useRoute();
const router = useRouter();
const { runAction, watchItem, copyLink } = useActions();

const text = ref("");
const region = ref<"all" | Region>(app.settings?.search.defaultRegion ?? "all");
const selected = ref<string[]>([]);
const busy = ref(false);
const view = ref<"card" | "list" | "table">("card");
const fPlatform = ref<string>("all");
const fVerdict = ref<"good" | "all" | "exact">("good");
const fCond = ref<"any" | "new" | "used">("any");
const fMin = ref("");
const fMax = ref("");
const sort = ref<"rank" | "price_asc" | "price_desc" | "score">("rank");
const showHidden = ref(false);
const detailId = ref<string | null>(null);
const cmdBox = ref<InstanceType<typeof CommandBox> | null>(null);

const run = computed(() => search.current);
const hasRun = computed(() => !!run.value);
const running = computed(() => run.value?.status === "running");

onMounted(async () => {
  if (!search.history.length) void search.loadHistory();
  const q = route.query;
  if (typeof q.run === "string" && q.run !== search.currentId) {
    try {
      await search.loadRun(q.run);
    } catch (e) {
      ui.toast(toAppError(e).message, "error");
    }
  }
  if (typeof q.q === "string") text.value = q.q;
  if (run.value && !text.value) text.value = run.value.instruction;
  if (!run.value) setTimeout(() => cmdBox.value?.focus(), 300);
});

watch(
  () => search.currentId,
  () => {
    showHidden.value = false;
    fPlatform.value = "all";
    if (run.value?.instruction) text.value = run.value.instruction;
  },
);
watch(region, () => (selected.value = selected.value.filter((id) => region.value === "all" || platforms.byId[id]?.region === region.value)));

async function go(instruction?: string, plan?: SearchPlan) {
  const t = (instruction ?? text.value).trim();
  if (!t && !plan) {
    ui.toast("先说说你想找什么", "warn");
    cmdBox.value?.focus();
    return;
  }
  if (!app.ai.verified) {
    ui.toast("AI 未连接，无法搜索", "error", 3500, "请先在「设置 · AI 模型」完成连接测试");
    void router.push("/settings/ai");
    return;
  }
  busy.value = true;
  try {
    if (plan) {
      await search.start(t || plan.instruction, plan);
    } else if (selected.value.length || region.value !== "all") {
      const ids = selected.value.length ? selected.value : platforms.enabled.filter((p) => p.region === region.value).map((p) => p.id);
      if (!ids.length) throw { code: "INVALID", message: "该地区没有启用的平台，请到「平台目录」启用" };
      ui.toast("AI 正在理解你的需求…", "ai", 2000);
      await search.planThenStart(t, ids, (id) => (platforms.byId[id]?.locale ?? "zh").slice(0, 2));
    } else {
      await search.start(t);
    }
    if (route.query.run) void router.replace({ query: {} });
  } catch (e) {
    const err = toAppError(e);
    ui.toast(err.message, "error", 4200);
    if (err.code === "AI_REQUIRED") void router.push("/settings/ai");
  } finally {
    busy.value = false;
  }
}

function newSearch() {
  search.clearCurrent();
  text.value = "";
  setTimeout(() => cmdBox.value?.focus(), 50);
}

async function cancel() {
  if (!run.value) return;
  try {
    await search.cancel(run.value.runId);
    ui.toast("已取消本次搜索", "info");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function showBrowser() {
  try {
    await api.browserShow();
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function saveMonitor(plan: SearchPlan) {
  try {
    const m = await watchStore.createMonitor({
      instruction: plan.instruction,
      plan,
      intervalMinutes: plan.monitor?.intervalMinutes ?? 360,
      targetPrice: plan.monitor?.targetPrice ?? plan.priceMax ?? null,
      name: plan.keywords[0] ? `${plan.keywords[0]} 监控` : undefined,
    });
    ui.toast(`已保存为定时监控「${m.name}」`, "success", 3000, "每 " + Math.round(m.intervalMinutes / 60) + " 小时自动重跑");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}

/* ---------------- 结果筛选 */
const results = computed(() => run.value?.results ?? []);
const resultPlatforms = computed(() => [...new Set(results.value.map((r) => r.platformId))]);
const price = (r: ResultItem) => r.displayPrice ?? app.toDisplay(r.price, r.currency);
const filtered = computed(() => {
  const okV: Record<"good" | "all" | "exact", Verdict[]> = { good: ["exact", "close", "related"], all: ["exact", "close", "related"], exact: ["exact"] };
  const min = fMin.value ? Number(fMin.value) : null;
  const max = fMax.value ? Number(fMax.value) : null;
  let out = results.value.filter((r) => r.match.verdict !== "mismatch" && okV[fVerdict.value].includes(r.match.verdict));
  if (fVerdict.value === "good") out = out.filter((r) => r.match.verdict !== "related" || r.match.score >= 50);
  if (fPlatform.value !== "all") out = out.filter((r) => r.platformId === fPlatform.value);
  if (fCond.value !== "any") out = out.filter((r) => r.condition === fCond.value);
  if (min != null) out = out.filter((r) => (price(r) ?? 0) >= min);
  if (max != null) out = out.filter((r) => (price(r) ?? Infinity) <= max);
  const s = [...out];
  if (sort.value === "price_asc") s.sort((a, b) => (price(a) ?? 1e15) - (price(b) ?? 1e15));
  else if (sort.value === "price_desc") s.sort((a, b) => (price(b) ?? -1) - (price(a) ?? -1));
  else if (sort.value === "score") s.sort((a, b) => b.match.score - a.match.score);
  else s.sort((a, b) => a.rank - b.rank);
  return s;
});
const hidden = computed(() => results.value.filter((r) => r.match.verdict === "mismatch"));
const bestId = computed(() => run.value?.summary?.bestPick?.id ?? null);
const showSkeleton = computed(() => running.value && !results.value.length && ["extract", "verify", "rank", "search"].includes(run.value?.stage ?? ""));
const examples = computed(() => EXAMPLES[app.role] ?? EXAMPLES.consumer);
const recent = computed(() => search.history.slice(0, 4));

function openDetail(r: ResultItem) {
  detailId.value = r.id;
}
function ask(r: ResultItem) {
  copilot.openWith(r);
}

const regionOpts = [
  { value: "all" as const, label: "全部" },
  { value: "cn" as const, label: "国内版" },
  { value: "intl" as const, label: "国际版" },
];
const viewOpts = [
  { value: "card" as const, label: "卡片", icon: "grid" },
  { value: "list" as const, label: "列表", icon: "list" },
  { value: "table" as const, label: "比价表", icon: "table" },
];
const verdictOpts = [
  { value: "good" as const, label: "推荐" },
  { value: "exact" as const, label: "仅精确" },
  { value: "all" as const, label: "全部相关" },
];
</script>

<template>
  <div class="page search-page">
    <section class="hero" :class="{ compact: hasRun }">
      <div v-if="!hasRun" class="hero-text">
        <p class="eyebrow">AI 多平台购物搜索</p>
        <h1>今天想<span class="grad">找点什么</span>？</h1>
        <p class="muted">说出品牌型号、预算和平台，Hawwwker 会在你登录好的各平台上替你搜索、校验和比价。</p>
      </div>
      <div v-else class="hero-row-top">
        <button class="btn ghost sm" @click="newSearch"><Icon name="plus" />新搜索</button>
        <span class="faint tiny">修改指令后回车即可重新搜索</span>
      </div>
      <CommandBox ref="cmdBox" v-model="text" :busy="busy || search.pendingPlan || running" :compact="hasRun" @run="go()" />
      <div class="hero-row">
        <GSegmented v-model="region" :options="regionOpts" small />
        <PlatformPicker v-model="selected" :region="region" />
        <span class="spacer"></span>
        <span class="faint tiny"><Icon name="shield" :size="13" /> 在你的登录会话中运行 · 全程可见、可取消</span>
      </div>
    </section>

    <Transition name="blur" mode="out-in">
      <div v-if="!hasRun" key="idle" class="stack">
        <div class="grid g4 stagger">
          <button v-for="(e, i) in examples" :key="e.title" v-tilt="3" class="ex glass" :style="{ '--i': i }" @click="text = e.text; go(e.text)">
            <span class="ex-ico"><Icon :name="e.icon" :size="18" /></span>
            <b>{{ e.title }}</b>
            <span class="muted small">{{ e.text }}</span>
            <span class="ex-go"><Icon name="arrow" :size="15" /></span>
          </button>
        </div>
        <section v-if="recent.length" class="glass card">
          <div class="card-head"><h3>最近搜索</h3><span class="spacer"></span><router-link to="/discover/history" class="tiny">全部记录</router-link></div>
          <div class="recent">
            <button v-for="h in recent" :key="h.id" class="rec" @click="search.loadRun(h.id)">
              <Icon name="history" :size="15" class="faint" />
              <span class="ellipsis">{{ h.instruction }}</span>
              <span class="chip" :class="RUN_STATUS[h.status].tone">{{ RUN_STATUS[h.status].label }}</span>
              <span class="faint tiny">{{ ago(h.startedAt) }}</span>
            </button>
          </div>
        </section>
      </div>

      <div v-else key="run" class="stack">
        <LiveActivity v-if="run" :run="run" @cancel="cancel" @show-browser="showBrowser" />
        <Transition name="blur">
          <SummaryCard v-if="run?.summary" :summary="run.summary" :results="results" @action="runAction" @open="openDetail" />
        </Transition>
        <PlanCard v-if="run?.plan" :plan="run.plan" :running="running" :collapsed="!running && !!run.summary" @rerun="(p) => go(p.instruction, p)" @monitor="saveMonitor" />

        <template v-if="results.length || showSkeleton">
          <div class="toolbar glass">
            <GSegmented v-model="fVerdict" :options="verdictOpts" small />
            <select v-model="fPlatform" class="select sm" style="width: 150px">
              <option value="all">全部平台（{{ resultPlatforms.length }}）</option>
              <option v-for="id in resultPlatforms" :key="id" :value="id">{{ platforms.label(id) }}</option>
            </select>
            <select v-model="fCond" class="select sm" style="width: 104px">
              <option value="any">不限成色</option>
              <option value="new">全新</option>
              <option value="used">二手</option>
            </select>
            <div class="range">
              <input v-model="fMin" class="input sm" type="number" placeholder="最低" />
              <span class="faint">–</span>
              <input v-model="fMax" class="input sm" type="number" placeholder="最高" />
              <span class="faint tiny">{{ app.displayCurrency }}</span>
            </div>
            <select v-model="sort" class="select sm" style="width: 128px">
              <option value="rank">综合排序</option>
              <option value="price_asc">价格从低到高</option>
              <option value="price_desc">价格从高到低</option>
              <option value="score">匹配度</option>
            </select>
            <span class="spacer"></span>
            <span class="faint tiny">{{ filtered.length }} 条</span>
            <GSegmented v-model="view" :options="viewOpts" small>
              <template #option="{ option }"><span class="vt" :title="option.label"><Icon :name="option.icon!" :size="15" /></span></template>
            </GSegmented>
          </div>

          <div v-if="showSkeleton" class="grid g4">
            <div v-for="i in 8" :key="i" class="skel" style="height: 300px; border-radius: 20px"></div>
          </div>

          <div v-else-if="view === 'card'" class="grid g4 stagger results">
            <ResultCard
              v-for="(r, i) in filtered"
              :key="r.id"
              :item="r"
              :best="r.id === bestId"
              :style="{ '--i': Math.min(i, 12) }"
              @open="openDetail(r)"
              @watch="watchItem(r)"
              @copilot="ask(r)"
              @external="openExternal(r.url)"
              @copy="copyLink(r.url)"
            />
          </div>

          <div v-else-if="view === 'list'" class="glass list stagger">
            <div v-for="(r, i) in filtered" :key="r.id" class="li" :style="{ '--i': Math.min(i, 12) }" @click="openDetail(r)">
              <img v-if="r.image" :src="r.image" alt="" />
              <div class="li-main">
                <div class="row" style="gap: 8px">
                  <PlatformBadge :id="r.platformId" />
                  <span class="chip" :class="VERDICT[r.match.verdict].tone">{{ VERDICT[r.match.verdict].label }} {{ r.match.score }}</span>
                  <span v-if="r.id === bestId" class="chip amber">AI 推荐</span>
                </div>
                <p class="ellipsis li-title">{{ r.title }}</p>
                <p class="faint tiny ellipsis">{{ r.match.reasons.join(" · ") }}</p>
              </div>
              <PriceTag :price="r.price" :currency="r.currency" :display="r.displayPrice" />
              <div class="row" @click.stop>
                <button class="btn sm" @click="openExternal(r.url)"><Icon name="external" /></button>
                <button class="btn sm" :disabled="r.watched" @click="watchItem(r)"><Icon :name="r.watched ? 'check' : 'tag'" /></button>
              </div>
            </div>
          </div>

          <div v-else class="glass table-wrap">
            <table class="table">
              <thead>
                <tr><th>商品</th><th>平台</th><th class="r">原币价格</th><th class="r">换算（{{ app.displayCurrency }}）</th><th>匹配</th><th>成色</th><th>店铺</th><th></th></tr>
              </thead>
              <tbody>
                <tr v-for="r in [...filtered].sort((a, b) => (price(a) ?? 1e15) - (price(b) ?? 1e15))" :key="r.id" @click="openDetail(r)" style="cursor: pointer">
                  <td style="max-width: 320px"><div class="ellipsis" :title="r.title">{{ r.title }}</div></td>
                  <td><PlatformBadge :id="r.platformId" /></td>
                  <td class="r"><span class="price">{{ money(r.price, r.currency) }}</span></td>
                  <td class="r"><span class="price" :class="{ amber: r.id === bestId }">{{ money(price(r), app.displayCurrency) }}</span></td>
                  <td><span class="chip" :class="VERDICT[r.match.verdict].tone">{{ VERDICT[r.match.verdict].label }}</span></td>
                  <td class="faint">{{ CONDITION[r.condition] }}</td>
                  <td class="faint ellipsis" style="max-width: 120px">{{ r.shop ?? "—" }}</td>
                  <td class="r" @click.stop><button class="btn ghost sm" @click="openExternal(r.url)"><Icon name="external" />打开</button></td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="!showSkeleton && !filtered.length" class="empty glass">
            <div class="empty-icon"><Icon name="filter" /></div>
            <h3>没有符合筛选条件的结果</h3>
            <p>试试放宽价格区间、切换到「全部相关」或选择全部平台。</p>
          </div>

          <section v-if="hidden.length" class="hidden-box glass">
            <button class="hb-head" @click="showHidden = !showHidden">
              <Icon name="eye" :size="16" />
              <b>可能不相关 {{ hidden.length }} 条</b>
              <span class="faint small">AI 判断为配件、不同产品或广告，默认折叠以免干扰</span>
              <span class="spacer"></span>
              <Icon name="down" :size="15" :style="{ transform: showHidden ? 'rotate(180deg)' : '', transition: 'transform .3s' }" />
            </button>
            <Transition name="blur">
              <div v-if="showHidden" class="grid g4" style="padding: 0 14px 14px">
                <ResultCard
                  v-for="r in hidden"
                  :key="r.id"
                  :item="r"
                  @open="openDetail(r)"
                  @watch="watchItem(r)"
                  @copilot="ask(r)"
                  @external="openExternal(r.url)"
                  @copy="copyLink(r.url)"
                />
              </div>
            </Transition>
          </section>
        </template>
      </div>
    </Transition>

    <ResultDetail :id="detailId" @close="detailId = null" />
  </div>
</template>

<style scoped>
.hero {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 26px 0 8px;
  max-width: 940px;
  margin: 0 auto;
  width: 100%;
  transition: padding 420ms var(--spring-soft);
}
.hero.compact {
  padding: 0;
  max-width: none;
}
.hero-text {
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 8px;
}
.hero-text h1 {
  font-size: 34px;
  font-weight: 760;
  letter-spacing: -0.02em;
}
.grad {
  background: linear-gradient(120deg, #3d7bff, #8a5cff 55%, #ff9f1a);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.hero-row,
.hero-row-top {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.hero-row .faint {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.ex {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 16px;
  border-radius: 20px;
  text-align: left;
  cursor: pointer;
  min-height: 150px;
}
.ex-ico {
  width: 36px;
  height: 36px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  background: var(--info-bg);
  color: var(--brand-ink);
  margin-bottom: 4px;
}
.ex b {
  font-size: 14.5px;
}
.ex-go {
  position: absolute;
  right: 14px;
  top: 16px;
  color: var(--text-3);
  transition: transform var(--t-mid) var(--spring), color var(--t-fast);
}
.ex:hover .ex-go {
  transform: translateX(4px);
  color: var(--brand);
}
.recent {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 16px;
}
.rec {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: none;
  background: transparent;
  border-radius: 10px;
  cursor: pointer;
  text-align: left;
  min-width: 0;
}
.rec:hover {
  background: var(--hover);
}
.rec .ellipsis {
  flex: 1;
  font-size: 13px;
}
.toolbar {
  position: sticky;
  top: 64px;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 16px;
  flex-wrap: wrap;
}
.range {
  display: flex;
  align-items: center;
  gap: 4px;
}
.range .input {
  width: 64px;
}
.vt {
  display: inline-grid;
  place-items: center;
}
.results {
  align-items: stretch;
}
.list {
  padding: 4px;
}
.li {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px 12px;
  border-radius: 14px;
  cursor: pointer;
  transition: background var(--t-fast);
}
.li:hover {
  background: var(--hover);
}
.li img {
  width: 56px;
  height: 56px;
  border-radius: 12px;
  object-fit: cover;
}
.li-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.li-title {
  font-weight: 560;
}
.table-wrap {
  padding: 6px 8px;
  overflow-x: auto;
}
.hidden-box {
  padding: 0;
  overflow: hidden;
}
.hb-head {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
}
@media (max-width: 1280px) {
  .results.g4,
  .grid.g4 {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}
@media (max-width: 1100px) {
  .results.g4,
  .grid.g4 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
