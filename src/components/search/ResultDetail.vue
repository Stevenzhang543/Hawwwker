<script setup lang="ts">
/* 商品详情抽屉（三级）：价格历史、同款比价、AI 匹配理由、打开商品页 / 联系商家 / 追踪 */
import { computed, ref, watch } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { ListingDetail, ResultItem } from "@/api/types";
import { usePlatformStore } from "@/stores/platforms";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { useAppStore } from "@/stores/app";
import { useCopilotStore } from "@/stores/copilot";
import { VERDICT, FLAG, CONDITION } from "@/utils/labels";
import { copyText, dateTime, money } from "@/utils/format";
import { hasLandedInfo } from "@/utils/landed";
import DealBadge from "@/components/ui/DealBadge.vue";
import GSheet from "@/components/ui/GSheet.vue";
import Icon from "@/components/ui/Icon.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import LineChart from "@/components/charts/LineChart.vue";

/** fallback：列表里的那条结果。AI 核对中的初判结果（id 以 p: 开头）还没入库，直接用它展示 */
const props = defineProps<{ id: string | null; fallback?: ResultItem | null }>();
const emit = defineEmits<{ close: []; feedback: ["irrelevant" | "clear"] }>();
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
    const local = props.fallback && props.fallback.id === id ? props.fallback : null;
    if (id.startsWith("p:") || local?.provisional) {
      d.value = { item: { ...(local as ResultItem) }, history: [], alsoOn: [], watchId: null, deal: null };
      target.value = "";
      return;
    }
    loading.value = true;
    // 初判 → 最终结果原位替换时保留已显示的内容，避免闪烁
    if (!d.value || !local || d.value.item.externalId !== local.externalId) d.value = null;
    try {
      d.value = await api.listingGet(id);
      const p = d.value.item.couponPrice ?? d.value.item.price;
      target.value = p ? String(Math.round(p * 0.92)) : "";
    } catch (e) {
      if (local) {
        d.value = { item: { ...local }, history: [], alsoOn: local.alsoOn, watchId: null, deal: local.deal ?? null };
      } else {
        ui.toast(toAppError(e).message, "error");
        emit("close");
      }
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

const it = computed(() => d.value?.item ?? null);
const provisional = computed(() => !!it.value?.provisional);
const deal = computed(() => d.value?.deal ?? it.value?.deal ?? null);
const feedback = computed(() => props.fallback?.feedback ?? it.value?.feedback ?? null);

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
        <span v-if="provisional" class="chip purple"><Icon name="refresh" :size="12" class="spin" />AI 核对中 · 规则初判 {{ it.match.score }}</span>
        <span v-else class="chip" :class="VERDICT[it.match.verdict].tone">{{ VERDICT[it.match.verdict].label }} · {{ it.match.score }}</span>
        <DealBadge v-if="!provisional" :deal="deal" />
      </div>
      <div v-else class="skel" style="height: 26px; width: 200px"></div>
    </template>

    <div v-if="!it" class="col">
      <div class="skel" style="height: 220px"></div>
      <div class="skel" style="height: 24px; width: 70%"></div>
      <div class="skel" style="height: 160px"></div>
    </div>

    <div v-else class="stack">
      <div class="top">
        <img v-if="it.image" :src="it.image" alt="" class="hero" />
        <div class="top-info">
          <h2 class="selectable">{{ it.title }}</h2>
          <PriceTag
            :price="it.price"
            :currency="it.currency"
            :display="it.displayPrice"
            :original="it.originalPrice"
            :landed="it.landedPrice"
            :display-landed="it.displayLanded"
            :landed-label="hasLandedInfo(it)"
            size="lg"
            amber
          />
          <div v-if="hasLandedInfo(it)" class="landed">
            <span>标价 <b>{{ money(it.price, it.currency) }}</b></span>
            <span v-if="it.couponPrice != null && it.price != null && it.couponPrice < it.price">券后 / 满减 <b>{{ money(it.couponPrice, it.currency) }}</b></span>
            <span v-if="it.shipping === 0" class="ok">包邮</span>
            <span v-else-if="it.shipping != null">运费 <b>{{ money(it.shipping, it.currency) }}</b></span>
            <span>= 到手 <b class="amber-t">{{ money(it.landedPrice ?? it.price, it.currency) }}</b></span>
          </div>
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
            <button v-if="feedback === 'irrelevant'" class="btn sm" title="撤销标记" @click="emit('feedback', 'clear')"><Icon name="refresh" />撤销「不相关」</button>
            <button v-else class="btn sm ghost fb" title="折叠这条结果；同一需求 / 监控以后自动排除" @click="emit('feedback', 'irrelevant')"><Icon name="x" />不相关</button>
          </div>
        </div>
      </div>

      <div v-if="provisional" class="banner info"><Icon name="spark" />这是规则初判的结果，AI 正在逐条核对。核对完成后可看价格历史、同款比价并加入追踪。</div>

      <section v-if="deal && !provisional" class="glass card deal-box">
        <div class="card-head">
          <h3>现在入手合适吗？</h3>
          <DealBadge :deal="deal" show-new />
          <span class="spacer"></span>
          <span v-if="deal.low != null" class="chip green">最低 {{ money(deal.low, it.currency) }}</span>
          <span v-if="deal.median != null" class="chip">中位 {{ money(deal.median, it.currency) }}</span>
          <span v-if="deal.high != null" class="chip">最高 {{ money(deal.high, it.currency) }}</span>
        </div>
        <p class="muted small">{{ deal.basis }}<template v-if="deal.points"> · 依据本机记录的 {{ deal.points }} 次价格<template v-if="deal.days">（{{ deal.days }} 天）</template></template></p>
      </section>

      <section v-if="!provisional" class="glass card">
        <div class="card-head">
          <h3>价格历史</h3>
          <span v-if="stats" class="faint tiny">{{ stats.n }} 个价格点 · 自 {{ dateTime(stats.since) }}</span>
          <span class="spacer"></span>
          <span v-if="stats" class="chip green">最低 {{ money(stats.lo, it.currency) }}</span>
          <span v-if="stats" class="chip">最高 {{ money(stats.hi, it.currency) }}</span>
        </div>
        <LineChart v-if="(stats?.n ?? 0) >= 3" :series="series" :height="190" :format="(v) => money(v, it!.currency)" :target="target ? Number(target) : null" />
        <div v-else class="new-hist">
          <Icon name="clock" :size="18" />
          <div>
            <b>新商品：已记录 {{ stats?.n ?? 0 }} 次价格</b>
            <p class="faint small">至少 3 次记录才画走势。加入追踪后会按间隔自动读价（默认每天），也会在每次搜索到它时记录。</p>
          </div>
        </div>
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
        <div class="card-head">
          <h3>{{ provisional ? "匹配初判" : "AI 匹配判断" }}</h3>
          <span v-if="provisional" class="chip outline">规则判断 · 等待 AI 核对</span>
          <span v-else class="chip" :class="VERDICT[it.match.verdict].tone">{{ VERDICT[it.match.verdict].desc }}</span>
        </div>
        <ul class="reasons">
          <li v-for="r in it.match.reasons" :key="r">{{ r }}</li>
        </ul>
        <div v-if="it.match.flags.length" class="chips" style="margin-top: 8px">
          <span v-for="f in it.match.flags" :key="f" class="chip red">{{ FLAG[f] ?? f }}</span>
        </div>
      </section>

      <section v-if="!provisional" class="glass card watch-box">
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
.landed {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 12px;
  font-size: 12.5px;
  color: var(--text-2);
}
.landed b {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
.landed .ok {
  color: var(--ok);
  font-weight: 600;
}
.amber-t {
  color: var(--amber-ink) !important;
}
.new-hist {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  padding: 12px 14px;
  border-radius: 14px;
  background: var(--hover);
  color: var(--text-2);
}
.deal-box {
  border-color: color-mix(in srgb, var(--ok) 25%, transparent);
}
.btn.fb:hover {
  color: var(--err);
}
.watch-box {
  border-color: rgba(255, 176, 32, 0.3);
}
</style>
