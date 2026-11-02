<script setup lang="ts">
/* 结果筛选浮层：平台、成色、到手价区间、包邮、好价。工具栏上只留一个按钮 + 已启用条件数 */
import { computed, onBeforeUnmount, ref } from "vue";
import { usePlatformStore } from "@/stores/platforms";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";
import GSegmented from "@/components/ui/GSegmented.vue";
import { blankFilters, type ResultFilters } from "./filters";

const props = defineProps<{ modelValue: ResultFilters; platforms: string[]; currency: string; counts: Record<string, number> }>();
const emit = defineEmits<{ "update:modelValue": [ResultFilters] }>();
const store = usePlatformStore();
const open = ref(false);
const root = ref<HTMLElement | null>(null);

const active = computed(() => {
  const f = props.modelValue;
  return [f.platform !== "all", f.cond !== "any", !!f.min || !!f.max, f.freeShip, f.goodDeal].filter(Boolean).length;
});
function set<K extends keyof ResultFilters>(k: K, v: ResultFilters[K]) {
  emit("update:modelValue", { ...props.modelValue, [k]: v });
}
function reset() {
  emit("update:modelValue", blankFilters());
}
function onDoc(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node)) close();
}
function close() {
  open.value = false;
  document.removeEventListener("pointerdown", onDoc);
}
function toggle() {
  if (open.value) return close();
  open.value = true;
  setTimeout(() => document.addEventListener("pointerdown", onDoc), 0);
}
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDoc));

const condOpts = [
  { value: "any" as const, label: "不限" },
  { value: "new" as const, label: "全新" },
  { value: "used" as const, label: "二手" },
];
</script>

<template>
  <div ref="root" class="fp">
    <button v-ripple class="btn sm" :class="{ on: active }" @click="toggle">
      <Icon name="filter" />筛选<span v-if="active" class="cnt">{{ active }}</span>
    </button>
    <Transition name="pop">
      <div v-if="open" class="fp-pop glass-3" @keydown.esc="close">
        <div class="fp-head">
          <b>筛选结果</b>
          <span class="spacer"></span>
          <button class="btn ghost sm" :disabled="!active" @click="reset">重置</button>
        </div>

        <div class="fp-sec">
          <span class="fp-l">平台</span>
          <div class="fp-plats">
            <button class="fp-p" :class="{ on: modelValue.platform === 'all' }" @click="set('platform', 'all')">全部平台</button>
            <button v-for="id in platforms" :key="id" class="fp-p" :class="{ on: modelValue.platform === id }" :title="store.label(id)" @click="set('platform', id)">
              <PlatformBadge :id="id" /><span class="faint tiny">{{ counts[id] ?? 0 }}</span>
            </button>
          </div>
        </div>

        <div class="fp-sec">
          <span class="fp-l">成色</span>
          <GSegmented :model-value="modelValue.cond" :options="condOpts" small @update:model-value="(v) => set('cond', v)" />
        </div>

        <div class="fp-sec">
          <span class="fp-l">到手价</span>
          <div class="row" style="gap: 6px">
            <input :value="modelValue.min" class="input sm" type="number" min="0" placeholder="最低" @input="set('min', ($event.target as HTMLInputElement).value)" />
            <span class="faint">–</span>
            <input :value="modelValue.max" class="input sm" type="number" min="0" placeholder="最高" @input="set('max', ($event.target as HTMLInputElement).value)" />
            <span class="faint tiny">{{ currency }}</span>
          </div>
        </div>

        <div class="fp-sec checks">
          <label class="ck"><input type="checkbox" :checked="modelValue.freeShip" @change="set('freeShip', ($event.target as HTMLInputElement).checked)" />只看包邮 / 免运费</label>
          <label class="ck"><input type="checkbox" :checked="modelValue.goodDeal" @change="set('goodDeal', ($event.target as HTMLInputElement).checked)" />只看好价（历史最低 / 近期低位）</label>
        </div>
        <p class="faint tiny">价格区间与排序都按到手价（券后 + 运费，已换算）计算。</p>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.fp {
  position: relative;
}
.btn.on {
  color: var(--brand-ink);
  border-color: color-mix(in srgb, var(--brand-1) 40%, transparent);
}
.cnt {
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--brand-grad);
  color: #fff;
  font-size: 11px;
  font-weight: 700;
  display: inline-grid;
  place-items: center;
  margin-left: 2px;
}
.fp-pop {
  position: absolute;
  top: 40px;
  left: 0;
  width: 420px;
  padding: 12px 14px;
  border-radius: 18px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.fp-head {
  display: flex;
  align-items: center;
}
.fp-sec {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.fp-l {
  font-size: 11.5px;
  font-weight: 650;
  color: var(--text-3);
}
.fp-plats {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  max-height: 150px;
  overflow-y: auto;
}
.fp-p {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 9px 0 6px;
  border-radius: 10px;
  border: 1px solid transparent;
  background: var(--hover);
  font-size: 12.5px;
  cursor: pointer;
}
.fp-p:first-child {
  padding-left: 10px;
}
.fp-p.on {
  background: var(--info-bg);
  border-color: color-mix(in srgb, var(--brand-1) 35%, transparent);
  color: var(--brand-ink);
  font-weight: 600;
}
.input.sm {
  width: 96px;
}
.checks {
  gap: 4px;
}
.ck {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  cursor: pointer;
}
</style>
