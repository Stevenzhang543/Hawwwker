<script setup lang="ts">
/* 商品详情抽屉（三级）：价格历史、同款比价、AI 匹配理由、打开商品页 / 联系商家 / 追踪 */
import { computed, ref, watch } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { ListingDetail } from "@/api/types";
import { usePlatformStore } from "@/stores/platforms";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { useCopilotStore } from "@/stores/copilot";
import { VERDICT, FLAG, CONDITION } from "@/utils/labels";
import { copyText, dateTime, money } from "@/utils/format";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import LineChart from "@/components/charts/LineChart.vue";

const props = defineProps<{ id: string | null }>();
const emit = defineEmits<{ close: [] }>();
const platforms = usePlatformStore();
const watchStore = useWatchStore();
const ui = useUiStore();
const app = useAppStore();
const copilot = useCopilotStore();
const d = ref<ListingDetail | null>(null);
const loading = ref(false);
const target = ref<string>("");
const open = computed({ get: () => !!props.id, set: (v) => !v && emit("close") });

watch(
  () => props.id,
  async (id) => {
    if (!id) return;
    loading.value = true;
    d.value = null;
    try {
      d.value = await api.listingGet(id);
      const p = d.value.item.price;
      target.value = p ? String(Math.round(p * 0.92)) : "";
    } catch (e) {
      ui.toast(toAppError(e).message, "error");
      emit("close");
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

const it = computed(() => d.value?.item ?? null);
const series = computed(() =>
  d.value && d.value.history.length
    ? [{ key: "p", label: "价格", color: "#3d7bff", points: d.value.history.map((h) => ({ t: new Date(h.t).getTime(), v: h.price })) }]
    : [],
);
const stats = computed(() => {
  const h = d.value?.history ?? [];
  if (!h.length) return null;
  const ps = h.map((x) => x.price);
  return { lo: Math.min(...ps), hi: Math.max(...ps), n: h.length, since: h[0]!.t };
});
const compare = computed(() => {
  if (!it.value) return [];
  const self = { listingId: it.value.id, platformId: it.value.platformId, price: it.value.price, currency: it.value.currency, url: it.value.url, self: true };
  return [self, ...(d.value?.alsoOn ?? []).map((a) => ({ ...a, self: false }))]
    .map((r) => ({ ...r, conv: app.toDisplay(r.price, r.currency) }))
    .sort((a, b) => (a.conv ?? 1e15) - (b.conv ?? 1e15));
});

async function addWatch() {
  if (!it.value) return;
  try {
    const t = target.value.trim() ? Number(target.value) : null;
    await watchStore.add({ listingId: it.value.id, targetPrice: t });
    if (d.value) d.value.item.watched = true;
    ui.toast("已加入价格追踪", "success", 2600, t ? `目标价 ${money(t, it.value.currency)}` : "价格变化时会提醒你");
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function copy() {
  if (it.value && (await copyText(it.value.url))) ui.toast("链接已复制", "success", 1600);
}
function ask() {
  if (!it.value) return;
  copilot.openWith(it.value);
}
</script>

<template>
  <GSheet v-model:open="open" :width="640">
    <template #head>
      <div v-if="it" class="dh">
        <PlatformBadge :id="it.platformId" size="md" />
        <span class="chip" :class="VERDICT[it.match.verdict].tone">{{ VERDICT[it.match.verdict].label }} · {{ it.match.score }}</span>
      </div>
      <div v-else class="skel" style="height: 26px; width: 200px"></div>
    </template>

    <div v-if="loading || !it" class="col">
      <div class="skel" style="height: 220px"></div>
      <div class="skel" style="height: 24px; width: 70%"></div>
      <div class="skel" style="height: 160px"></div>
    </div>

    <div v-else class="stack">
      <div class="top">
        <img v-if="it.image" :src="it.image" alt="" class="hero" />
        <div class="top-info">
          <h2 class="selectable">{{ it.title }}</h2>
          <PriceTag :price="it.price" :currency="it.currency" :display="it.displayPrice" :original="it.originalPrice" size="lg" amber />
          <div class="meta">
            <span v-if="it.shop"><Icon name="shop" :size="13" />{{ it.shop }}</span>
            <span v-if="it.location"><Icon name="pin" :size="13" />{{ it.location }}</span>
            <span>{{ CONDITION[it.condition] }}</span>
            <span v-if="it.rating"><Icon name="star" :size="12" />{{ it.rating }}<template v-if="it.reviews">（{{ it.reviews }} 评价）</template></span>
          </div>
          <div class="row wrap" style="margin-top: 6px">
            <button v-ripple class="btn primary" @click="openExternal(it.url)"><Icon name="external" />打开商品页</button>
            <button v-ripple class="btn" title="在平台内联系商家 / 店铺客服" @click="openExternal(it.url)"><Icon name="chat" />联系商家</button>
            <button class="btn icon" title="复制链接" @click="copy"><Icon name="copy" /></button>
            <button class="btn icon" title="问 Copilot" @click="ask"><Icon name="bot" /></button>
          </div>
        </div>
      </div>

      <section class="glass card">
        <div class="card-head">
          <h3>价格历史</h3>
          <span v-if="stats" class="faint tiny">{{ stats.n }} 个价格点 · 自 {{ dateTime(stats.since) }}</span>
          <span class="spacer"></span>
          <span v-if="stats" class="chip green">最低 {{ money(stats.lo, it.currency) }}</span>
          <span v-if="stats" class="chip">最高 {{ money(stats.hi, it.currency) }}</span>
        </div>
        <LineChart v-if="series.length" :series="series" :height="190" :format="(v) => money(v, it!.currency)" :target="target ? Number(target) : null" />
        <p v-else class="faint small">还没有历史价格，追踪后会定期记录。</p>
      </section>

      <section v-if="compare.length > 1" class="glass card">
        <div class="card-head"><h3>同款比价</h3><span class="faint tiny">已换算为 {{ app.displayCurrency }}，不含运费与进口税</span></div>
        <table class="table">
          <tbody>
            <tr v-for="(r, i) in compare" :key="r.listingId">
              <td><PlatformBadge :id="r.platformId" /> <span v-if="r.self" class="chip blue" style="margin-left: 6px">当前</span></td>
              <td class="r"><span class="price" :class="{ amber: i === 0 }">{{ money(r.price, r.currency) }}</span></td>
              <td class="r faint">{{ r.conv != null ? "≈ " + money(r.conv, app.displayCurrency) : "" }}</td>
              <td class="r"><button class="btn ghost sm" @click="openExternal(r.url)"><Icon name="external" />打开</button></td>
            </tr>
          </tbody>
        </table>
      </section>

      <section class="glass card">
        <div class="card-head"><h3>AI 匹配判断</h3><span class="chip" :class="VERDICT[it.match.verdict].tone">{{ VERDICT[it.match.verdict].desc }}</span></div>
        <ul class="reasons">
          <li v-for="r in it.match.reasons" :key="r">{{ r }}</li>
        </ul>
        <div v-if="it.match.flags.length" class="chips" style="margin-top: 8px">
          <span v-for="f in it.match.flags" :key="f" class="chip red">{{ FLAG[f] ?? f }}</span>
        </div>
      </section>

      <section class="glass card watch-box">
        <div class="card-head"><h3>价格追踪</h3><span class="faint tiny">低于目标价时通过系统通知与应用内提醒告诉你</span></div>
        <div v-if="it.watched || d?.watchId" class="row">
          <span class="chip green"><Icon name="check" />已在追踪中</span>
          <span class="spacer"></span>
          <router-link class="btn sm" :to="`/track/watchlist?id=${d?.watchId ?? ''}`">查看追踪</router-link>
        </div>
        <div v-else class="row">
          <label class="field" style="flex: 1">
            <span>目标价（{{ it.currency }}，可留空）</span>
            <input v-model="target" class="input" type="number" min="0" placeholder="例如 199" />
          </label>
          <button v-ripple class="btn amber lg" style="align-self: flex-end" @click="addWatch"><Icon name="tag" />开始追踪</button>
        </div>
      </section>
      <p class="faint tiny">首次发现 {{ dateTime(it.firstSeenAt) }} · 最近更新 {{ dateTime(it.lastSeenAt) }} · {{ platforms.label(it.platformId) }}</p>
    </div>
  </GSheet>
</template>

<style scoped>
.dh {
  display: flex;
  align-items: center;
  gap: 10px;
}
.top {
  display: flex;
  gap: 16px;
}
.hero {
  width: 200px;
  height: 200px;
  border-radius: 18px;
  object-fit: cover;
  flex: none;
  box-shadow: var(--shadow-1);
}
.top-info {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
.top-info h2 {
  font-size: 17px;
  line-height: 1.45;
}
.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  font-size: 12.5px;
  color: var(--text-2);
}
.meta span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.reasons {
  margin: 0;
  padding-left: 18px;
  color: var(--text-2);
  display: grid;
  gap: 3px;
}
.watch-box {
  border-color: rgba(255, 176, 32, 0.3);
}
</style>
