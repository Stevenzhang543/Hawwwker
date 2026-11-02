<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { api, openExternal, toAppError } from "@/api";
import type { LinkInfo, Region, ResultItem, SearchPlan, SearchPrefs, ShopKind, Verdict } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { itemKey, useSearchStore, visibleItems } from "@/stores/search";
import { usePlatformStore } from "@/stores/platforms";
import { useUiStore } from "@/stores/ui";
import { useWatchStore } from "@/stores/watch";
import { useCopilotStore } from "@/stores/copilot";
import { useActions, RESULTS_OP_EVENT } from "@/composables/actions";
import { EXAMPLES, VERDICT, RUN_STATUS, CONDITION } from "@/utils/labels";
import { ago, copyText, money } from "@/utils/format";
import { hasLandedInfo, isUrl, landedChips, landedDisplay } from "@/utils/landed";
import { OFFICIAL_KINDS, RISK_LEVEL, SHOP_KIND_LABEL, TONE_CHIP, cardTags, isHighRisk, itemTags, reviewChip, riskTitle, scoreTone, shopKindOf } from "@/components/search/insight";
import CommandBox from "@/components/search/CommandBox.vue";
import PlatformPicker from "@/components/search/PlatformPicker.vue";
import LiveActivity from "@/components/search/LiveActivity.vue";
import PlanCard from "@/components/search/PlanCard.vue";
import SummaryCard from "@/components/search/SummaryCard.vue";
import ResultCard from "@/components/search/ResultCard.vue";
import ResultDetail from "@/components/search/ResultDetail.vue";
import LinkCard from "@/components/search/LinkCard.vue";
import FilterPopover from "@/components/search/FilterPopover.vue";
import { blankFilters, type ResultFilters } from "@/components/search/filters";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import DealBadge from "@/components/ui/DealBadge.vue";

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
const fVerdict = ref<"good" | "all" | "exact">("good");
const filters = ref<ResultFilters>(blankFilters());
const sort = ref<"rank" | "final" | "risk" | "price_asc" | "price_desc" | "score">("rank");
const showHidden = ref(false);
const detailKey = ref<string | null>(null);
const cmdBox = ref<InstanceType<typeof CommandBox> | null>(null);
/** 有结果后指令框收成一行；点击后展开 */
const cmdOpen = ref(false);

const run = computed(() => search.current);
const hasRun = computed(() => !!run.value);
const running = computed(() => run.value?.status === "running");
const items = computed(() => visibleItems(run.value));
const hasItems = computed(() => items.value.length > 0);
const provisional = computed(() => hasItems.value && !run.value?.results.length);
const canExport = computed(() => run.value?.status === "done" && !!run.value.results.length);

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
    filters.value = blankFilters();
    detailKey.value = null;
    cmdOpen.value = false;
    if (run.value?.instruction) text.value = run.value.instruction;
  },
);
watch(region, () => (selected.value = selected.value.filter((id) => region.value === "all" || platforms.byId[id]?.region === region.value)));

/* ---------------- 指令框（一行 ⇄ 展开） */
function openCmd() {
  cmdOpen.value = true;
  void nextTick(() => cmdBox.value?.focus());
}
function closeCmd() {
  if (hasRun.value) cmdOpen.value = false;
}
const showFullCmd = computed(() => !hasRun.value || cmdOpen.value);
const scopeLabel = computed(() => {
  const scope = selected.value.length
    ? selected.value.length <= 2 ? selected.value.map((id) => platforms.label(id)).join("、") : `${selected.value.length} 个平台`
    : region.value === "all" ? "AI 选平台" : region.value === "cn" ? "国内版" : "国际版";
  return `${scope} · ${DEPTH_LABEL[depth.value]}`;
});

/* ---------------- 检索深度（v26.03）：保存为默认值，本次与之后的搜索都按它执行 */
type Depth = SearchPrefs["depth"];
const DEPTH_LABEL: Record<Depth, string> = { quick: "快速", standard: "标准", deep: "深入" };
const depth = computed<Depth>(() => app.settings?.search.depth ?? "standard");
const depthOpts = computed(() => [
  { value: "quick" as const, label: "快速", title: "只做匹配、风险与店铺分析，不打开商品页读评论（最快）" },
  { value: "standard" as const, label: "标准", title: `为排名靠前的 ${app.settings?.search.reviewTop ?? 6} 件商品读取评论并分析（推荐）` },
  { value: "deep" as const, label: "深入", title: "读取更多商品的评论与店铺评价，耗时更长" },
]);
async function setDepth(v: Depth) {
  if (v === depth.value) return;
  try {
    await app.update({ search: { depth: v } as Partial<SearchPrefs> });
    ui.toast(`检索深度：${DEPTH_LABEL[v]}`, "info", 1600, depthOpts.value.find((o) => o.value === v)?.title);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}

/* ---------------- 粘贴商品链接 */
const linkUrl = computed(() => (isUrl(text.value) ? text.value.trim() : ""));
const link = reactive<{ url: string; info: LinkInfo | null; loading: boolean; error: string | null; watching: boolean; dismissed: string }>({
  url: "", info: null, loading: false, error: null, watching: false, dismissed: "",
});
const showLink = computed(() => !!linkUrl.value && link.url === linkUrl.value && link.dismissed !== linkUrl.value && showFullCmd.value);
let linkTimer = 0;
watch(linkUrl, (u) => {
  window.clearTimeout(linkTimer);
  if (!u) return;
  linkTimer = window.setTimeout(() => void inspect(u), 450);
});
onBeforeUnmount(() => window.clearTimeout(linkTimer));

/** Copilot 对当前结果的界面操作（只看精确匹配 / 按到手价排序），见 composables/actions.ts */
function onResultsOp(e: Event) {
  const op = (e as CustomEvent<{ op: string }>).detail?.op;
  if (op === "filter_exact") {
    fVerdict.value = "exact";
    e.preventDefault();
  } else if (op === "sort_landed") {
    sort.value = "price_asc";
    e.preventDefault();
  }
}
onMounted(() => window.addEventListener(RESULTS_OP_EVENT, onResultsOp));
onBeforeUnmount(() => window.removeEventListener(RESULTS_OP_EVENT, onResultsOp));

async function inspect(url: string) {
  if (link.url === url && (link.info || link.loading)) return;
  Object.assign(link, { url, info: null, loading: true, error: null, dismissed: "" });
  try {
    const info = await api.linkInspect(url);
    if (link.url === url) link.info = info;
  } catch (e) {
    if (link.url === url) link.error = toAppError(e).message;
  } finally {
    if (link.url === url) link.loading = false;
  }
}
function retryLink() {
  const u = link.url;
  link.url = "";
  void inspect(u);
}
async function compareLink() {
  if (!link.info) return;
  const ins = link.info.instruction;
  link.dismissed = link.url;
  text.value = ins;
  await go(ins);
}
async function watchLink() {
  if (!link.info) return;
  link.watching = true;
  try {
    const w = await watchStore.add({ url: link.url, title: link.info.title ?? undefined, platformId: link.info.platformId ?? undefined });
    link.info.watchId = w.id;
    ui.toast("已加入价格追踪", "success", 2800, w.title.slice(0, 32));
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    link.watching = false;
  }
}

/* ---------------- 搜索 */
async function go(instruction?: string, plan?: SearchPlan) {
  const t = (instruction ?? text.value).trim();
  if (!t && !plan) {
    ui.toast("先说说你想找什么", "warn");
    cmdBox.value?.focus();
    return;
  }
  // 输入的是链接：先识别，再由用户选择"找同款"或"追踪"；已识别时回车 = 找同款
  if (!plan && isUrl(t)) {
    if (link.url === t && link.info) return compareLink();
    void inspect(t);
    return;
  }
  if (!app.ai.verified) {
    ui.toast("AI 未连接，无法搜索", "error", 3500, "请先在「设置 · AI 模型」完成连接测试");
    void router.push("/settings/ai");
    return;
  }
  busy.value = true;
  try {
    let runId: string | null;
    if (plan) {
      runId = await search.start(t || plan.instruction, plan);
    } else if (selected.value.length || region.value !== "all") {
      // 在选择器里逐个选的平台 = 指定平台：需要登录的平台必须先登录（v26.03）；只选地区则由 AI 判定并跳过未登录平台
      const explicit = selected.value.length > 0;
      const ids = explicit ? selected.value : platforms.enabled.filter((p) => p.region === region.value).map((p) => p.id);
      if (!ids.length) throw { code: "INVALID", message: "该地区没有启用的平台，请到「平台目录」启用" };
      ui.toast("AI 正在理解你的需求…", "ai", 2000);
      runId = await search.planThenStart(t, ids, (id) => (platforms.byId[id]?.locale ?? "zh").slice(0, 2), explicit);
    } else {
      runId = await search.start(t);
    }
    if (!runId) {
      ui.toast("未开始搜索", "info", 2600, "所选平台需要先登录；登录后再搜索即可");
      return;
    }
    cmdOpen.value = false;
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

/* ---------------- 结果筛选（价格一律按到手价） */
const price = (r: ResultItem) => landedDisplay(r, app.toDisplay);
const marked = (r: ResultItem) => r.feedback === "irrelevant";
/** v26.03：开启"自动折叠高风险"时，高风险商品放进折叠区并写明原因 */
const hideRisk = computed(() => app.settings?.search.hideHighRisk ?? true);
const foldedRisk = (r: ResultItem) => hideRisk.value && isHighRisk(r);
const shown = computed(() => items.value.filter((r) => r.match.verdict !== "mismatch" && !marked(r) && !foldedRisk(r)));
const shopKinds = computed(() => {
  const m = new Map<string, number>();
  for (const r of shown.value) {
    const k = shopKindOf(r);
    if (k) m.set(k, (m.get(k) ?? 0) + 1);
  }
  const order = (k: string) => (OFFICIAL_KINDS.includes(k as ShopKind) ? 0 : 1);
  return [...m.entries()]
    .sort((a, b) => order(a[0]) - order(b[0]) || b[1] - a[1])
    .map(([kind, count]) => ({ kind, label: SHOP_KIND_LABEL[kind as ShopKind] ?? kind, count }));
});
const hasRisk = computed(() => items.value.some((r) => r.risk));
function hiddenReason(r: ResultItem): string | null {
  if (marked(r)) return null;
  if (r.match.verdict === "mismatch") return null;
  if (foldedRisk(r)) return `高风险：${r.risk?.summary || r.risk?.factors.filter((f) => f.weight > 0).map((f) => f.label).slice(0, 2).join("、") || "建议谨慎"}`;
  return null;
}

/* ---------------- 补充阶段指示：评论检索 / 分析进行中时，标出正在处理的靠前商品 */
const pendingKeys = computed(() => {
  const r = run.value;
  if (!r || r.status !== "running" || !r.results.length) return new Map<string, string>();
  const stage = r.stage;
  const m = new Map<string, string>();
  const top = [...r.results]
    .filter((x) => x.match.verdict !== "mismatch")
    .sort((a, b) => a.rank - b.rank)
    .slice(0, app.settings?.search.depth === "deep" ? Math.max(12, app.settings.search.reviewTop) : app.settings?.search.reviewTop ?? 6);
  if (stage === "reviews") for (const x of top) if (!x.reviewInsight) m.set(itemKey(x), "评论读取中…");
  if (stage === "review_ai") for (const x of top) if (x.reviewInsight && !x.reviewInsight.aiChecked && x.reviewInsight.source !== "none") m.set(itemKey(x), "AI 分析评论中…");
  if (stage === "risk") for (const x of r.results) if (!x.risk) m.set(itemKey(x), "风险分析中…");
  if (stage === "shop") for (const x of r.results) if (!x.shopInfo) m.set(itemKey(x), "评估店铺…");
  return m;
});
const resultPlatforms = computed(() => [...new Set(items.value.map((r) => r.platformId))]);
const counts = computed(() => {
  const c: Record<string, number> = {};
  for (const r of shown.value) c[r.platformId] = (c[r.platformId] ?? 0) + 1;
  return c;
});
const filtered = computed(() => {
  const okV: Record<"good" | "all" | "exact", Verdict[]> = { good: ["exact", "close", "related"], all: ["exact", "close", "related"], exact: ["exact"] };
  const f = filters.value;
  const min = f.min ? Number(f.min) : null;
  const max = f.max ? Number(f.max) : null;
  let out = shown.value.filter((r) => okV[fVerdict.value].includes(r.match.verdict));
  if (fVerdict.value === "good") out = out.filter((r) => r.match.verdict !== "related" || r.match.score >= 50);
  if (f.platform !== "all") out = out.filter((r) => r.platformId === f.platform);
  if (f.cond !== "any") out = out.filter((r) => r.condition === f.cond);
  if (min != null) out = out.filter((r) => (price(r) ?? 0) >= min);
  if (max != null) out = out.filter((r) => (price(r) ?? Infinity) <= max);
  if (f.freeShip) out = out.filter((r) => r.shipping === 0);
  if (f.goodDeal) out = out.filter((r) => r.deal?.level === "lowest" || r.deal?.level === "low");
  if (f.risk === "no_high") out = out.filter((r) => r.risk?.level !== "high");
  else if (f.risk === "low") out = out.filter((r) => r.risk?.level === "low");
  if (f.shops.length) out = out.filter((r) => f.shops.includes(shopKindOf(r) ?? "unknown"));
  const s = [...out];
  if (sort.value === "final") s.sort((a, b) => (b.finalScore ?? -1) - (a.finalScore ?? -1) || a.rank - b.rank);
  else if (sort.value === "risk") s.sort((a, b) => (a.risk?.score ?? 50) - (b.risk?.score ?? 50) || (b.finalScore ?? 0) - (a.finalScore ?? 0));
  else if (sort.value === "price_asc") s.sort((a, b) => (price(a) ?? 1e15) - (price(b) ?? 1e15));
  else if (sort.value === "price_desc") s.sort((a, b) => (price(b) ?? -1) - (price(a) ?? -1));
  else if (sort.value === "score") s.sort((a, b) => b.match.score - a.match.score);
  else s.sort((a, b) => a.rank - b.rank);
  return s;
});
const hidden = computed(() => items.value.filter((r) => r.match.verdict === "mismatch" || marked(r) || foldedRisk(r)));
const userMarked = computed(() => hidden.value.filter(marked).length);
const riskFolded = computed(() => hidden.value.filter((r) => !marked(r) && r.match.verdict !== "mismatch" && foldedRisk(r)).length);
const bestId = computed(() => run.value?.summary?.bestPick?.id ?? null);
const showSkeleton = computed(() => running.value && !hasItems.value && ["extract", "verify", "rank", "search", "screen", "platforms"].includes(run.value?.stage ?? ""));
const examples = computed(() => EXAMPLES[app.role] ?? EXAMPLES.consumer);
const recent = computed(() => search.history.slice(0, 4));

/* ---------------- 详情（按稳定键跟踪：初判 → 最终结果时自动切到正式记录） */
const detailItem = computed(() => (detailKey.value ? items.value.find((x) => itemKey(x) === detailKey.value) ?? null : null));
function openDetail(r: ResultItem) {
  detailKey.value = itemKey(r);
}
function ask(r: ResultItem) {
  copilot.openWith(r);
}
function onWatch(r: ResultItem) {
  if (r.provisional) {
    ui.toast("AI 核对完成后即可追踪", "info", 2200);
    return;
  }
  void watchItem(r);
}

/* ---------------- 不相关反馈 + 撤销 */
const undo = ref<ResultItem | null>(null);
let undoTimer = 0;
async function feedback(r: ResultItem, kind: "irrelevant" | "clear") {
  if (!run.value) return;
  try {
    await search.setFeedback(run.value.runId, r, kind);
    window.clearTimeout(undoTimer);
    if (kind === "irrelevant") {
      undo.value = r;
      undoTimer = window.setTimeout(() => (undo.value = null), 6000);
    } else {
      undo.value = null;
      ui.toast("已恢复到结果列表", "success", 1800);
    }
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function undoFeedback() {
  const r = undo.value;
  if (r) void feedback(r, "clear");
}
onBeforeUnmount(() => window.clearTimeout(undoTimer));

/* ---------------- 分享 / 导出 */
async function copyList() {
  if (!run.value) return;
  try {
    const t = await api.runExport(run.value.runId, "text");
    const ok = await copyText(t);
    ui.toast(ok ? "比价清单已复制" : "复制失败", ok ? "success" : "error", 2800, ok ? "可直接粘贴到微信、邮件或备忘录" : undefined);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function exportCsv() {
  if (!run.value) return;
  try {
    const path = await api.runExport(run.value.runId, "csv");
    ui.toast("已导出 CSV（含到手价）", "success", 5000, path);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
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
    <!-- 指令：无结果时是大输入框；有结果后收成一行 -->
    <section v-if="!showFullCmd && run" class="cmd-line glass">
      <span class="cl-ico"><Icon name="spark" :size="15" /></span>
      <button class="cl-text" title="点击修改指令" @click="openCmd">
        <span class="ellipsis">{{ run.instruction || run.plan?.instruction || text }}</span>
      </button>
      <span class="chip outline cl-scope" title="平台范围（展开后可修改）">{{ scopeLabel }}</span>
      <button class="btn sm ghost" @click="openCmd"><Icon name="edit" />修改</button>
      <button class="btn sm" @click="newSearch"><Icon name="plus" />新搜索</button>
    </section>

    <section v-else class="hero" :class="{ compact: hasRun }">
      <div v-if="!hasRun" class="hero-text">
        <p class="eyebrow">AI 多平台购物搜索</p>
        <h1>今天想<span class="grad">找点什么</span>？</h1>
        <p class="muted">说出品牌型号、预算和平台，或直接粘贴商品链接。Hawwwker 会在你登录好的各平台上替你搜索、核对和比价。</p>
      </div>
      <div v-else class="hero-row-top">
        <button class="btn ghost sm" @click="newSearch"><Icon name="plus" />新搜索</button>
        <span class="faint tiny">修改指令后回车即可重新搜索 · Esc 收起</span>
        <span class="spacer"></span>
        <button class="icon-btn" title="收起" @click="closeCmd"><Icon name="up" :size="16" /></button>
      </div>
      <CommandBox
        ref="cmdBox"
        v-model="text"
        :busy="busy || search.pendingPlan || running"
        :compact="hasRun"
        :link="!!linkUrl"
        @run="go()"
        @escape="closeCmd"
      />
      <LinkCard
        v-if="showLink"
        :url="link.url"
        :info="link.info"
        :loading="link.loading"
        :error="link.error"
        :watching="link.watching"
        @compare="compareLink"
        @watch="watchLink"
        @view="(id) => router.push(`/track/watchlist?id=${id}`)"
        @retry="retryLink"
        @close="link.dismissed = link.url"
      />
      <div class="hero-row">
        <GSegmented v-model="region" :options="regionOpts" small />
        <PlatformPicker v-model="selected" :region="region" />
        <span class="depth" title="检索深度：会保存为默认值（也可在「设置 · 通用」修改）">
          <span class="faint tiny">深度</span>
          <GSegmented :model-value="depth" :options="depthOpts" small @update:model-value="setDepth">
            <template #option="{ option }"><span :title="(option as { title?: string }).title">{{ option.label }}</span></template>
          </GSegmented>
        </span>
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

      <div v-else key="run" class="stack run-stack">
        <LiveActivity v-if="run" :run="run" :slim="hasItems" @cancel="cancel" @show-browser="showBrowser" @rerun="go(run.instruction || run.plan?.instruction)" />
        <PlanCard v-if="run?.plan" :plan="run.plan" :running="running" @rerun="(p) => go(p.instruction, p)" @monitor="saveMonitor" />

        <!-- 总结位：先用一条状态占位，总结到达后原位替换，不把结果往下推 -->
        <template v-if="hasItems">
          <SummaryCard v-if="run?.summary" :summary="run.summary" :results="items" @action="runAction" @open="openDetail" />
          <div v-else-if="running" class="sum-wait glass">
            <span class="sw-ico"><Icon name="spark" :size="15" /></span>
            <span v-if="provisional" class="small">
              已出 <b class="num">{{ items.length }}</b> 条初步结果（规则初判）· AI 正在逐条核对，完成后原位更新
            </span>
            <span v-else-if="run?.stage === 'summarize'" class="small">AI 已完成细筛，正在写总结与推荐…</span>
            <span v-else class="small ellipsis">{{ run?.stageLabel || "处理中" }}… 完成后给出总结与推荐</span>
            <span class="spacer"></span>
            <span class="dots"><i></i><i></i><i></i></span>
          </div>
        </template>

        <template v-if="hasItems || showSkeleton">
          <div class="toolbar glass">
            <GSegmented v-model="fVerdict" :options="verdictOpts" small />
            <select v-model="sort" class="select sm" style="width: 132px" title="价格排序按到手价（券后 + 运费）">
              <option value="rank">综合排序</option>
              <option value="final">综合推荐分</option>
              <option value="risk">低风险优先</option>
              <option value="price_asc">到手价从低到高</option>
              <option value="price_desc">到手价从高到低</option>
              <option value="score">匹配度</option>
            </select>
            <FilterPopover
              v-model="filters"
              :platforms="resultPlatforms"
              :currency="app.displayCurrency"
              :counts="counts"
              :shop-kinds="shopKinds"
              :has-risk="hasRisk"
            />
            <span class="spacer"></span>
            <span class="faint tiny">{{ filtered.length }} 条<template v-if="provisional"> · 初判</template></span>
            <button class="btn sm ghost" :disabled="!canExport" :title="canExport ? '复制为可粘贴的比价清单' : '搜索完成后可复制'" @click="copyList"><Icon name="copy" />复制清单</button>
            <button class="btn sm ghost" :disabled="!canExport" :title="canExport ? '导出本次结果（含到手价）为 CSV' : '搜索完成后可导出'" @click="exportCsv"><Icon name="download" />CSV</button>
            <GSegmented v-model="view" :options="viewOpts" small>
              <template #option="{ option }"><span class="vt" :title="option.label"><Icon :name="option.icon!" :size="15" /></span></template>
            </GSegmented>
          </div>

          <div v-if="showSkeleton" class="grid g4">
            <div v-for="i in 8" :key="i" class="skel" style="height: 300px; border-radius: 20px"></div>
          </div>

          <TransitionGroup v-else-if="view === 'card'" tag="div" name="rg" class="grid g4 stagger results">
            <ResultCard
              v-for="(r, i) in filtered"
              :key="itemKey(r)"
              :item="r"
              :best="r.id === bestId"
              :pending="pendingKeys.get(itemKey(r)) ?? null"
              :style="{ '--i': Math.min(i, 12) }"
              @open="openDetail(r)"
              @watch="onWatch(r)"
              @copilot="ask(r)"
              @external="openExternal(r.url)"
              @copy="copyLink(r.url)"
              @irrelevant="feedback(r, 'irrelevant')"
            />
          </TransitionGroup>

          <div v-else-if="view === 'list'" class="glass list stagger">
            <div v-for="(r, i) in filtered" :key="itemKey(r)" class="li" :style="{ '--i': Math.min(i, 12) }" @click="openDetail(r)">
              <img v-if="r.image" :src="r.image" alt="" />
              <div class="li-main">
                <div class="row" style="gap: 8px">
                  <PlatformBadge :id="r.platformId" />
                  <span v-if="r.provisional" class="chip purple"><Icon name="refresh" :size="11" class="spin" />AI 核对中</span>
                  <span v-else class="chip" :class="VERDICT[r.match.verdict].tone">{{ VERDICT[r.match.verdict].label }} {{ r.match.score }}</span>
                  <span v-if="r.id === bestId" class="chip amber">AI 推荐</span>
                  <DealBadge :deal="r.deal" compact />
                  <span v-for="c in landedChips(r)" :key="c.text" class="chip" :class="c.tone" :title="c.title">{{ c.text }}</span>
                  <span v-if="r.risk" class="chip" :class="RISK_LEVEL[r.risk.level].tone" :title="riskTitle(r.risk)">{{ RISK_LEVEL[r.risk.level].label }} {{ r.risk.score }}</span>
                  <span v-for="t in cardTags(r).slice(0, 2)" :key="t.kind + t.label" class="chip" :class="TONE_CHIP[t.tone] ?? ''">{{ t.label }}</span>
                  <span v-if="reviewChip(r.reviewInsight)" class="chip" :class="reviewChip(r.reviewInsight)!.tone" :title="reviewChip(r.reviewInsight)!.title">{{ reviewChip(r.reviewInsight)!.text }}</span>
                  <span v-if="pendingKeys.get(itemKey(r))" class="faint tiny"><Icon name="refresh" :size="11" class="spin" />{{ pendingKeys.get(itemKey(r)) }}</span>
                </div>
                <p class="ellipsis li-title">{{ r.title }}</p>
                <p class="faint tiny ellipsis">{{ r.match.reasons.join(" · ") }}</p>
              </div>
              <span v-if="r.finalScore != null" class="li-score" :class="scoreTone(r.finalScore)" title="综合分"><b class="num">{{ r.finalScore }}</b><small>综合</small></span>
              <PriceTag :price="r.price" :currency="r.currency" :display="r.displayPrice" :landed="r.landedPrice" :display-landed="r.displayLanded" :landed-label="hasLandedInfo(r)" />
              <div class="row" @click.stop>
                <button class="btn sm" title="在平台中打开" @click="openExternal(r.url)"><Icon name="external" /></button>
                <button class="btn sm" :disabled="r.watched || r.provisional" title="追踪" @click="onWatch(r)"><Icon :name="r.watched ? 'check' : 'tag'" /></button>
                <button class="btn sm ghost" title="不相关" @click="feedback(r, 'irrelevant')"><Icon name="x" /></button>
              </div>
            </div>
          </div>

          <div v-else class="glass table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>商品</th><th>平台</th><th class="r">到手价（{{ app.displayCurrency }}）</th><th>匹配</th><th>风险</th><th>店铺</th><th>评论</th><th class="r">综合</th><th></th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="r in [...filtered].sort((a, b) => (price(a) ?? 1e15) - (price(b) ?? 1e15))" :key="itemKey(r)" style="cursor: pointer" @click="openDetail(r)">
                  <td style="max-width: 210px"><div class="ellipsis" :title="r.title">{{ r.title }}</div></td>
                  <td><PlatformBadge :id="r.platformId" /></td>
                  <td class="r">
                    <span class="price" :class="{ amber: r.id === bestId }">{{ money(price(r), app.displayCurrency) }}</span>
                    <div class="tiny faint" :title="`标价 ${money(r.price, r.currency)}`">{{ [money(r.price, r.currency), ...landedChips(r).map((c) => c.text)].join(" · ") }}</div>
                    <DealBadge :deal="r.deal" compact />
                  </td>
                  <td>
                    <span v-if="r.provisional" class="chip purple">核对中</span>
                    <span v-else class="chip" :class="VERDICT[r.match.verdict].tone">{{ VERDICT[r.match.verdict].label }}</span>
                  </td>
                  <td class="nw">
                    <span v-if="r.risk" class="chip" :class="RISK_LEVEL[r.risk.level].tone" :title="riskTitle(r.risk)">{{ RISK_LEVEL[r.risk.level].label }} {{ r.risk.score }}</span>
                    <span v-else class="faint">{{ CONDITION[r.condition] }}</span>
                  </td>
                  <td class="nw">
                    <span v-if="itemTags(r)[0]" class="chip" :class="TONE_CHIP[itemTags(r)[0]!.tone] ?? ''">{{ itemTags(r)[0]!.label }}</span>
                    <div class="ellipsis faint tiny" style="max-width: 108px" :title="r.shop ?? r.shopInfo?.name ?? ''">{{ r.shop ?? r.shopInfo?.name ?? "—" }}</div>
                  </td>
                  <td class="faint tiny" style="max-width: 120px" :title="reviewChip(r.reviewInsight)?.title ?? ''">{{ reviewChip(r.reviewInsight)?.text ?? "—" }}</td>
                  <td class="r"><b v-if="r.finalScore != null" class="num" :class="'t-' + scoreTone(r.finalScore)">{{ r.finalScore }}</b><span v-else class="faint">—</span></td>
                  <td class="r" @click.stop><button class="icon-btn" title="在平台中打开" @click="openExternal(r.url)"><Icon name="external" :size="16" /></button></td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="!showSkeleton && !filtered.length" class="empty glass">
            <div class="empty-icon"><Icon name="filter" /></div>
            <h3>没有符合筛选条件的结果</h3>
            <p>试试放宽到手价区间、切换到「全部相关」或在「筛选」里选择全部平台。</p>
          </div>

          <section v-if="hidden.length" class="hidden-box glass">
            <button class="hb-head" @click="showHidden = !showHidden">
              <Icon name="eye" :size="16" />
              <b>{{ riskFolded ? "可能不相关 / 高风险" : "可能不相关" }} {{ hidden.length }} 条</b>
              <span class="faint small">AI 判断为配件、不同产品或广告<template v-if="riskFolded">；高风险 {{ riskFolded }} 条（可在「设置 · 通用」关闭自动折叠）</template><template v-if="userMarked">，含你标记的 {{ userMarked }} 条</template>，默认折叠以免干扰</span>
              <span class="spacer"></span>
              <Icon name="down" :size="15" :style="{ transform: showHidden ? 'rotate(180deg)' : '', transition: 'transform .3s' }" />
            </button>
            <Transition name="blur">
              <div v-if="showHidden" class="grid g4" style="padding: 0 14px 14px">
                <ResultCard
                  v-for="r in hidden"
                  :key="itemKey(r)"
                  :item="r"
                  :hidden-reason="hiddenReason(r)"
                  @open="openDetail(r)"
                  @watch="onWatch(r)"
                  @copilot="ask(r)"
                  @external="openExternal(r.url)"
                  @copy="copyLink(r.url)"
                  @irrelevant="feedback(r, 'irrelevant')"
                  @restore="feedback(r, 'clear')"
                />
              </div>
            </Transition>
          </section>
        </template>
      </div>
    </Transition>

    <div class="undo-wrap">
      <Transition name="pop">
        <div v-if="undo" class="undo glass-3" role="status">
          <Icon name="eye" :size="15" />
          <span class="ellipsis">已把「{{ undo.title.slice(0, 22) }}{{ undo.title.length > 22 ? "…" : "" }}」标为不相关，折叠到「可能不相关」；同一需求以后自动排除</span>
          <button class="btn sm" @click="undoFeedback">撤销</button>
        </div>
      </Transition>
    </div>

    <ResultDetail
      :id="detailItem?.id ?? null"
      :fallback="detailItem"
      @close="detailKey = null"
      @feedback="(k) => detailItem && feedback(detailItem, k)"
    />
  </div>
</template>

<style scoped>
.run-stack {
  gap: 12px;
}
.cmd-line {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 8px 7px 10px;
  border-radius: 16px;
  min-width: 0;
}
.cl-ico {
  width: 28px;
  height: 28px;
  flex: none;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
}
.cl-text {
  flex: 1;
  min-width: 0;
  display: flex;
  border: none;
  background: transparent;
  text-align: left;
  font-size: 14.5px;
  font-weight: 600;
  color: var(--text);
  cursor: text;
  padding: 6px 4px;
  border-radius: 8px;
}
.cl-text:hover {
  background: var(--hover);
}
.cl-scope {
  flex: none;
}
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
  gap: 10px;
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
.sum-wait {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 14px 8px 12px;
  min-height: 56px;
  color: var(--text-2);
}
.sw-ico {
  width: 28px;
  height: 28px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  background: var(--purple-bg);
  color: var(--purple);
  flex: none;
}
.dots {
  display: inline-flex;
  gap: 4px;
}
.dots i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--purple);
  animation: pulse 1s infinite;
}
.dots i:nth-child(2) {
  animation-delay: 0.2s;
}
.dots i:nth-child(3) {
  animation-delay: 0.4s;
}
.toolbar {
  position: sticky;
  top: 64px;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: 16px;
  flex-wrap: wrap;
}
.vt {
  display: inline-grid;
  place-items: center;
}
.results {
  align-items: stretch;
}
.rg-move {
  transition: transform 520ms var(--spring-soft);
}
.rg-leave-active {
  display: none;
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
.table-wrap .nw,
.table-wrap th {
  white-space: nowrap;
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
.depth {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.li-score {
  display: inline-flex;
  align-items: baseline;
  gap: 3px;
  flex: none;
}
.li-score b {
  font-size: 16px;
}
.li-score small {
  font-size: 10.5px;
  color: var(--text-3);
}
.li-score.green b,
.t-green {
  color: var(--ok);
}
.li-score.blue b,
.t-blue {
  color: var(--brand-ink);
}
.li-score.orange b,
.t-orange {
  color: var(--warn);
}
.li-score.red b,
.t-red {
  color: var(--err);
}
.undo-wrap {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 26px;
  z-index: 70;
  display: flex;
  justify-content: center;
  pointer-events: none;
}
.undo {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 10px;
  max-width: min(680px, calc(100vw - 48px));
  padding: 8px 8px 8px 14px;
  border-radius: 14px;
  box-shadow: var(--shadow-3);
  font-size: 13px;
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
  .cl-scope {
    display: none;
  }
}
</style>
