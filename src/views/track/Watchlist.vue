<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { toAppError } from "@/api";
import type { WatchItem } from "@/api/types";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { ago, interval, money, pct } from "@/utils/format";
import GSegmented from "@/components/ui/GSegmented.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import CountUp from "@/components/ui/CountUp.vue";
import Sparkline from "@/components/charts/Sparkline.vue";
import WatchDetail from "@/components/track/WatchDetail.vue";

const store = useWatchStore();
const ui = useUiStore();
const app = useAppStore();
const route = useRoute();
const router = useRouter();
const url = ref("");
const target = ref("");
const adding = ref(false);
const sel = ref<WatchItem | null>(null);
const filter = ref<"all" | "drop" | "hit" | "error">("all");
const sort = ref<"recent" | "change" | "target">("recent");
const urlInput = ref<HTMLInputElement | null>(null);

onMounted(async () => {
  if (!store.loaded) await store.load();
  openFromQuery();
  if (route.query.add) setTimeout(() => urlInput.value?.focus(), 200);
});
watch(() => route.query.id, openFromQuery);
function openFromQuery() {
  const id = Number(route.query.id);
  if (id) sel.value = store.items.find((w) => w.id === id) ?? null;
}

const rows = computed(() => {
  let r = store.items;
  if (filter.value === "drop") r = r.filter((w) => (w.changePct ?? 0) < 0);
  if (filter.value === "hit") r = r.filter((w) => w.targetPrice != null && w.lastPrice != null && w.lastPrice <= w.targetPrice);
  if (filter.value === "error") r = r.filter((w) => w.status === "error");
  const s = [...r];
  if (sort.value === "change") s.sort((a, b) => (a.changePct ?? 0) - (b.changePct ?? 0));
  if (sort.value === "target") s.sort((a, b) => gap(a) - gap(b));
  return s;
});
function gap(w: WatchItem) {
  if (w.targetPrice == null || w.lastPrice == null) return 999;
  return (w.lastPrice - w.targetPrice) / w.targetPrice;
}
function progress(w: WatchItem) {
  if (w.targetPrice == null || w.lastPrice == null || w.firstPrice == null) return null;
  if (w.lastPrice <= w.targetPrice) return 100;
  const total = Math.max(1e-9, (w.highest ?? w.firstPrice) - w.targetPrice);
  return Math.max(4, Math.min(100, (((w.highest ?? w.firstPrice) - w.lastPrice) / total) * 100));
}
const kpi = computed(() => ({
  total: store.items.length,
  drops: store.drops,
  hits: store.hitTarget.length,
  errors: store.items.filter((w) => w.status === "error").length,
}));

async function add() {
  const u = url.value.trim();
  if (!/^https?:\/\//i.test(u)) {
    ui.toast("请粘贴完整的商品链接（以 http 开头）", "warn");
    return;
  }
  adding.value = true;
  try {
    const w = await store.add({ url: u, targetPrice: target.value ? Number(target.value) : null });
    url.value = "";
    target.value = "";
    ui.toast("已加入追踪，正在读取首个价格", "success", 2600, w.title);
    if (route.query.add) void router.replace({ query: {} });
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    adding.value = false;
  }
}
async function check(w: WatchItem) {
  try {
    await store.check(w.id);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
function openItem(w: WatchItem) {
  sel.value = w;
  void router.replace({ query: { id: String(w.id) } });
}
function closeItem() {
  sel.value = null;
  void router.replace({ query: {} });
}
const filterOpts = [
  { value: "all" as const, label: "全部" },
  { value: "drop" as const, label: "已降价" },
  { value: "hit" as const, label: "达到目标价" },
  { value: "error" as const, label: "读价失败" },
];
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1>价格追踪</h1>
        <p>盯住单件商品：定时打开商品页读价，降价或达到目标价时提醒你。</p>
      </div>
    </div>

    <div class="grid g4">
      <div class="kpi glass"><span class="faint small">追踪中</span><b><CountUp :value="kpi.total" /></b><Icon name="tag" class="k-ico" /></div>
      <div class="kpi glass"><span class="faint small">较首次降价</span><b class="down"><CountUp :value="kpi.drops" /></b><Icon name="drop" class="k-ico" /></div>
      <div class="kpi glass hit"><span class="faint small">达到目标价</span><b class="amber-t"><CountUp :value="kpi.hits" /></b><Icon name="target" class="k-ico" /></div>
      <div class="kpi glass"><span class="faint small">读价失败</span><b :class="{ up: kpi.errors }"><CountUp :value="kpi.errors" /></b><Icon name="warn" class="k-ico" /></div>
    </div>

    <section class="adder glass">
      <Icon name="link" :size="18" class="faint" />
      <input ref="urlInput" v-model="url" class="bare-input" placeholder="粘贴任意平台的商品链接（Amazon、京东、淘宝、eBay、idealo…）" @keydown.enter="add" />
      <input v-model="target" class="input sm" type="number" style="width: 130px" placeholder="目标价（可选）" />
      <button v-ripple class="btn primary" :disabled="adding" @click="add"><Icon :name="adding ? 'refresh' : 'plus'" :class="{ spin: adding }" />追踪</button>
    </section>

    <div class="row">
      <GSegmented v-model="filter" :options="filterOpts" small />
      <span class="spacer"></span>
      <select v-model="sort" class="select sm" style="width: 150px">
        <option value="recent">最近添加</option>
        <option value="change">降幅最大</option>
        <option value="target">最接近目标价</option>
      </select>
    </div>

    <div v-if="!store.loaded" class="grid g3">
      <div v-for="i in 6" :key="i" class="skel" style="height: 220px; border-radius: 20px"></div>
    </div>
    <div v-else-if="!rows.length" class="empty glass">
      <div class="empty-icon"><Icon name="tag" /></div>
      <h3>{{ store.items.length ? "没有符合条件的追踪项" : "还没有追踪任何商品" }}</h3>
      <p>在搜索结果上点「追踪」，或在上方粘贴商品链接。</p>
    </div>
    <TransitionGroup v-else name="list" tag="div" class="grid g3 cards">
      <article v-for="w in rows" :key="w.id" v-tilt="2" class="wc glass" :class="{ off: !w.enabled }" @click="openItem(w)">
        <div class="wc-top">
          <img v-if="w.image" :src="w.image" alt="" />
          <div class="wc-info">
            <PlatformBadge :id="w.platformId" />
            <p class="clamp2 wc-title">{{ w.title }}</p>
          </div>
          <span class="st-dot dot" :class="w.status === 'error' ? 'red' : w.status === 'checking' ? 'blue pulse' : w.enabled ? 'green' : ''" :title="w.status"></span>
        </div>
        <div class="wc-price">
          <div>
            <span class="price">{{ money(w.lastPrice, w.currency) }}</span>
            <span v-if="w.currency !== app.displayCurrency && w.lastPrice != null" class="faint tiny conv">≈ {{ money(app.toDisplay(w.lastPrice, w.currency), app.displayCurrency) }}</span>
            <div class="tiny" :class="(w.changePct ?? 0) < 0 ? 'down' : (w.changePct ?? 0) > 0 ? 'up' : 'faint'">
              <Icon :name="(w.changePct ?? 0) < 0 ? 'drop' : 'rise'" :size="12" />较首次 {{ pct(w.changePct) }}
            </div>
          </div>
          <Sparkline :data="w.spark" :width="110" :height="38" />
        </div>
        <div v-if="w.targetPrice != null" class="target">
          <div class="row tiny" style="justify-content: space-between">
            <span class="faint">目标 {{ money(w.targetPrice, w.currency) }}</span>
            <span v-if="w.lastPrice != null && w.lastPrice <= w.targetPrice" class="amber-t"><Icon name="check" :size="12" />已达到</span>
            <span v-else-if="w.lastPrice != null" class="faint">还差 {{ pct(((w.lastPrice - w.targetPrice) / w.targetPrice) * 100, 1, false) }}</span>
          </div>
          <div class="tbar"><i :style="{ width: (progress(w) ?? 0) + '%' }" :class="{ hit: (progress(w) ?? 0) >= 100 }"></i></div>
        </div>
        <p v-if="w.lastError" class="err tiny ellipsis"><Icon name="warn" :size="12" /> {{ w.lastError }}</p>
        <div class="wc-foot" @click.stop>
          <span class="faint tiny"><Icon name="clock" :size="12" /> 每 {{ interval(w.intervalMinutes) }} · {{ w.lastCheckedAt ? ago(w.lastCheckedAt) + '检查' : w.nextCheckAt ? ago(w.nextCheckAt) + '检查' : '等待首次检查' }}</span>
          <span class="spacer"></span>
          <button class="icon-btn" title="立即检查" :disabled="w.status === 'checking'" @click="check(w)"><Icon name="refresh" :size="16" :class="{ spin: w.status === 'checking' }" /></button>
        </div>
      </article>
    </TransitionGroup>

    <WatchDetail :item="sel" @close="closeItem" />
  </div>
</template>

<style scoped>
.kpi {
  position: relative;
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow: hidden;
}
.kpi b {
  font-size: 28px;
  font-weight: 720;
  font-family: var(--font-num);
}
.k-ico {
  position: absolute;
  right: 14px;
  top: 14px;
  width: 22px !important;
  height: 22px !important;
  color: var(--text-3);
  opacity: 0.6;
}
.amber-t {
  color: var(--amber-ink);
}
.adder {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 8px 8px 16px;
  border-radius: 18px;
}
.bare-input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  font-size: 14px;
  height: 36px;
}
.cards {
  position: relative;
}
.wc {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  border-radius: 20px;
  cursor: pointer;
}
.wc.off {
  opacity: 0.6;
}
.wc-top {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}
.wc-top img {
  width: 58px;
  height: 58px;
  border-radius: 14px;
  object-fit: cover;
  flex: none;
}
.wc-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.wc-title {
  font-size: 13px;
  font-weight: 580;
  line-height: 1.45;
}
.st-dot {
  margin-top: 6px;
}
.wc-price {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 10px;
}
.wc-price .price {
  font-size: 22px;
}
.wc-price .tiny {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.conv {
  margin-left: 6px;
}
.tbar {
  height: 6px;
  border-radius: 4px;
  background: var(--hover);
  margin-top: 5px;
  overflow: hidden;
}
.tbar i {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: var(--brand-grad);
  transition: width 700ms var(--spring-soft);
}
.tbar i.hit {
  background: linear-gradient(90deg, #ffbe3d, #ff9a1f);
}
.err {
  color: var(--err);
}
.wc-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  border-top: 1px solid var(--line);
  padding-top: 8px;
  margin-top: auto;
}
.wc-foot .faint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
</style>
