<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { PricePoint, WatchItem } from "@/api/types";
import { useWatchStore } from "@/stores/watch";
import { useUiStore } from "@/stores/ui";
import { ago, dateTime, interval, money, pct } from "@/utils/format";
import GSheet from "@/components/ui/GSheet.vue";
import GSegmented from "@/components/ui/GSegmented.vue";
import GSwitch from "@/components/ui/GSwitch.vue";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import PriceTag from "@/components/ui/PriceTag.vue";
import LineChart from "@/components/charts/LineChart.vue";

const props = defineProps<{ item: WatchItem | null }>();
const emit = defineEmits<{ close: [] }>();
const store = useWatchStore();
const ui = useUiStore();
const open = computed({ get: () => !!props.item, set: (v) => !v && emit("close") });
const days = ref(90);
const hist = ref<PricePoint[]>([]);
const loading = ref(false);
const target = ref("");
const iv = ref(360);
const note = ref("");

const live = computed(() => (props.item ? store.items.find((x) => x.id === props.item!.id) ?? props.item : null));

async function load() {
  if (!props.item) return;
  loading.value = true;
  try {
    hist.value = await api.watchHistory(props.item.id, days.value);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  } finally {
    loading.value = false;
  }
}
watch(
  () => props.item?.id,
  () => {
    if (!props.item) return;
    target.value = props.item.targetPrice != null ? String(props.item.targetPrice) : "";
    iv.value = props.item.intervalMinutes;
    note.value = props.item.note;
    void load();
  },
  { immediate: true },
);
watch(days, load);
watch(() => live.value?.lastCheckedAt, load);

const series = computed(() => [{ key: "w", label: "价格", color: "#3d7bff", points: hist.value.map((h) => ({ t: new Date(h.t).getTime(), v: h.price })) }]);

async function save() {
  if (!live.value) return;
  try {
    await store.update(live.value.id, { targetPrice: target.value.trim() ? Number(target.value) : null, intervalMinutes: iv.value, note: note.value });
    ui.toast("已保存", "success", 1500);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function check() {
  if (!live.value) return;
  try {
    await store.check(live.value.id);
    ui.toast("正在打开商品页读取最新价格…", "info", 2000);
  } catch (e) {
    ui.toast(toAppError(e).message, "error");
  }
}
async function toggle(v: boolean) {
  if (live.value) await store.update(live.value.id, { enabled: v });
}
async function remove() {
  if (!live.value) return;
  if (!(await ui.confirm("停止追踪这件商品？", "历史价格会保留在市场分析中。", "停止追踪", true))) return;
  await store.remove(live.value.id);
  emit("close");
}
const intervals = [30, 60, 180, 360, 720, 1440];
const dayOpts = [
  { value: 7, label: "7 天" },
  { value: 30, label: "30 天" },
  { value: 90, label: "90 天" },
];
</script>

<template>
  <GSheet v-model:open="open" :width="640">
    <template #head>
      <div v-if="live" class="row" style="gap: 10px">
        <PlatformBadge :id="live.platformId" size="md" />
        <span class="chip" :class="live.status === 'error' ? 'red' : live.status === 'checking' ? 'blue' : 'green'">
          {{ live.status === "error" ? "读价失败" : live.status === "checking" ? "读价中…" : "正常" }}
        </span>
      </div>
    </template>
    <div v-if="live" class="stack">
      <div class="top">
        <img v-if="live.image" :src="live.image" alt="" />
        <div class="col" style="gap: 6px; min-width: 0">
          <h2 class="selectable">{{ live.title }}</h2>
          <div class="row" style="gap: 14px; flex-wrap: wrap">
            <PriceTag :price="live.lastPrice" :currency="live.currency" size="lg" amber />
            <span v-if="live.changePct != null" class="chip" :class="live.changePct < 0 ? 'green' : live.changePct > 0 ? 'red' : ''">
              <Icon :name="live.changePct < 0 ? 'drop' : 'rise'" />较首次 {{ pct(live.changePct) }}
            </span>
          </div>
          <p class="faint small">最低 {{ money(live.lowest, live.currency) }} · 最高 {{ money(live.highest, live.currency) }} · 上次检查 {{ ago(live.lastCheckedAt) }} · 下次 {{ ago(live.nextCheckAt) }}</p>
          <p v-if="live.lastError" class="banner" style="padding: 8px 12px"><Icon name="warn" />{{ live.lastError }}</p>
          <div class="row wrap">
            <button v-ripple class="btn primary" @click="openExternal(live.url)"><Icon name="external" />打开商品页</button>
            <button v-ripple class="btn" :disabled="live.status === 'checking'" @click="check"><Icon name="refresh" :class="{ spin: live.status === 'checking' }" />立即检查</button>
          </div>
        </div>
      </div>

      <section class="glass card">
        <div class="card-head">
          <h3>价格走势</h3>
          <span class="spacer"></span>
          <GSegmented v-model="days" :options="dayOpts" small />
        </div>
        <div v-if="loading && !hist.length" class="skel" style="height: 210px"></div>
        <LineChart v-else :series="series" :height="210" :format="(v) => money(v, live!.currency)" :target="live.targetPrice ?? null" />
      </section>

      <section class="glass card">
        <div class="card-head"><h3>追踪设置</h3><span class="spacer"></span><span class="faint tiny">启用</span><GSwitch :model-value="live.enabled" small @update:model-value="toggle" /></div>
        <div class="form-grid">
          <label class="field">
            <span>目标价（{{ live.currency }}）</span>
            <input v-model="target" class="input" type="number" placeholder="不设置则只提醒降价" />
          </label>
          <label class="field">
            <span>检查间隔</span>
            <select v-model.number="iv" class="select">
              <option v-for="m in intervals" :key="m" :value="m">每 {{ interval(m) }}</option>
            </select>
          </label>
          <label class="field full">
            <span>备注</span>
            <input v-model="note" class="input" placeholder="例如：等双十一 / 竞品对标" />
          </label>
        </div>
        <div class="row" style="margin-top: 12px">
          <button class="btn ghost danger sm" @click="remove"><Icon name="trash" />停止追踪</button>
          <span class="spacer"></span>
          <span class="faint tiny">创建于 {{ dateTime(live.createdAt) }}</span>
          <button v-ripple class="btn primary sm" @click="save"><Icon name="check" />保存</button>
        </div>
      </section>

      <section class="glass card">
        <div class="card-head"><h3>检查记录</h3><span class="faint tiny">最近 {{ Math.min(12, hist.length) }} 条</span></div>
        <table class="table">
          <tbody>
            <tr v-for="h in [...hist].reverse().slice(0, 12)" :key="h.t">
              <td class="faint">{{ dateTime(h.t) }}</td>
              <td><span class="chip" :class="h.source === 'detail' ? 'blue' : ''">{{ h.source === "detail" ? "商品页读价" : "搜索时记录" }}</span></td>
              <td class="r"><span class="price">{{ money(h.price, h.currency) }}</span></td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
  </GSheet>
</template>

<style scoped>
.top {
  display: flex;
  gap: 16px;
}
.top img {
  width: 150px;
  height: 150px;
  border-radius: 18px;
  object-fit: cover;
  flex: none;
}
.top h2 {
  font-size: 16.5px;
  line-height: 1.45;
}
</style>
