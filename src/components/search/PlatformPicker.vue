<script setup lang="ts">
/* 平台选择浮层：空数组 = 由 AI 按指令自动选择 */
import { computed, onBeforeUnmount, ref } from "vue";
import { usePlatformStore } from "@/stores/platforms";
import type { Region } from "@/api/types";
import Icon from "@/components/ui/Icon.vue";
import PlatformBadge from "@/components/ui/PlatformBadge.vue";

const props = defineProps<{ modelValue: string[]; region: "all" | Region }>();
const emit = defineEmits<{ "update:modelValue": [string[]] }>();
const store = usePlatformStore();
const open = ref(false);
const root = ref<HTMLElement | null>(null);

const pool = computed(() => store.enabled.filter((p) => props.region === "all" || p.region === props.region));
const groups = computed(() => [
  { key: "cn", label: "国内版", items: pool.value.filter((p) => p.region === "cn") },
  { key: "intl", label: "国际版", items: pool.value.filter((p) => p.region === "intl") },
].filter((g) => g.items.length));

const summary = computed(() => {
  const n = props.modelValue.length;
  if (!n) return props.region === "all" ? "AI 自动选择平台" : `AI 在${props.region === "cn" ? "国内版" : "国际版"}中选择`;
  if (n <= 2) return props.modelValue.map((id) => store.label(id)).join("、");
  return `已选 ${n} 个平台`;
});

function toggle(id: string) {
  const s = new Set(props.modelValue);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  emit("update:modelValue", [...s]);
}
function onDoc(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}
function toggleOpen() {
  open.value = !open.value;
  if (open.value) setTimeout(() => document.addEventListener("pointerdown", onDoc), 0);
  else document.removeEventListener("pointerdown", onDoc);
}
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDoc));
</script>

<template>
  <div ref="root" class="pp">
    <button v-ripple class="pp-btn" :class="{ on: modelValue.length }" @click="toggleOpen">
      <Icon name="layers" :size="15" />
      <span>{{ summary }}</span>
      <span v-if="modelValue.length" class="stack">
        <PlatformBadge v-for="id in modelValue.slice(0, 4)" :key="id" :id="id" :show-name="false" />
      </span>
      <Icon name="down" :size="14" class="caret" :class="{ open }" />
    </button>
    <Transition name="pop">
      <div v-if="open" class="pp-pop glass-3">
        <div class="pp-head">
          <b>搜索平台</b>
          <span class="muted tiny">不选 = 由 AI 根据指令自动选择</span>
          <span class="spacer"></span>
          <button class="btn ghost sm" @click="emit('update:modelValue', [])">清空</button>
        </div>
        <div v-for="g in groups" :key="g.key" class="pp-group">
          <div class="pp-gl">{{ g.label }}</div>
          <div class="pp-grid">
            <button v-for="p in g.items" :key="p.id" class="pp-item" :class="{ on: modelValue.includes(p.id) }" @click="toggle(p.id)">
              <PlatformBadge :id="p.id" />
              <span class="ed">{{ p.edition }}</span>
              <span class="ck"><Icon name="check" :size="12" :stroke="2.6" /></span>
            </button>
          </div>
        </div>
        <router-link to="/platforms/catalog" class="pp-foot" @click="open = false">管理平台目录 <Icon name="arrow" :size="13" /></router-link>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.pp {
  position: relative;
}
.pp-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 12px;
  border-radius: 11px;
  border: 1px solid var(--line-2);
  background: var(--glass-1);
  font-size: 13px;
  font-weight: 560;
  cursor: pointer;
  color: var(--text-2);
  transition: transform var(--t-mid) var(--spring);
}
.pp-btn.on {
  color: var(--text);
  border-color: color-mix(in srgb, var(--brand-1) 40%, transparent);
}
.pp-btn:active {
  transform: scale(0.97);
}
.stack {
  display: inline-flex;
}
.stack > * + * {
  margin-left: -5px;
}
.caret {
  transition: transform var(--t-mid) var(--spring);
  color: var(--text-3);
}
.caret.open {
  transform: rotate(180deg);
}
.pp-pop {
  position: absolute;
  top: 42px;
  left: 0;
  width: 520px;
  max-height: 440px;
  overflow-y: auto;
  padding: 10px;
  border-radius: 18px;
  z-index: 60;
}
.pp-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 6px 8px;
}
.pp-gl {
  font-size: 11.5px;
  color: var(--text-3);
  font-weight: 650;
  padding: 6px 6px 4px;
}
.pp-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px;
}
.pp-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid transparent;
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: background var(--t-fast), border-color var(--t-fast);
}
.pp-item:hover {
  background: var(--hover);
}
.pp-item.on {
  background: var(--info-bg);
  border-color: color-mix(in srgb, var(--brand-1) 30%, transparent);
}
.ed {
  font-size: 11.5px;
  color: var(--text-3);
  flex: 1;
  white-space: nowrap;
}
.ck {
  width: 18px;
  height: 18px;
  border-radius: 6px;
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
.pp-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  justify-content: flex-end;
  font-size: 12.5px;
  padding: 10px 6px 2px;
}
</style>
