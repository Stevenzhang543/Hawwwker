<script setup lang="ts">
import { onMounted, ref } from "vue";
import { isTauri } from "@/api";
import { useAppStore } from "@/stores/app";
import BrandMark from "./BrandMark.vue";
import Icon from "@/components/ui/Icon.vue";

const app = useAppStore();
const maximized = ref(false);

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}
async function minimize() {
  if (isTauri) await (await win()).minimize();
}
async function toggleMax() {
  if (!isTauri) return;
  const w = await win();
  await w.toggleMaximize();
  maximized.value = await w.isMaximized();
}
async function close() {
  if (isTauri) await (await win()).close();
}

onMounted(async () => {
  if (!isTauri) return;
  const w = await win();
  maximized.value = await w.isMaximized();
  await w.onResized(async () => {
    maximized.value = await w.isMaximized();
  });
});
</script>

<template>
  <header class="titlebar" data-tauri-drag-region @dblclick="toggleMax">
    <div class="tb-brand" data-tauri-drag-region>
      <BrandMark :size="18" />
      <span class="tb-name" data-tauri-drag-region>Hawwwker <b>货客</b></span>
      <span class="tb-ver" data-tauri-drag-region>v{{ app.version }}</span>
    </div>
    <div class="tb-drag" data-tauri-drag-region></div>
    <div class="tb-controls" :class="{ inert: !isTauri }">
      <button class="tb-btn" title="最小化" @click="minimize"><Icon name="win_min" :size="15" /></button>
      <button class="tb-btn" :title="maximized ? '还原' : '最大化'" @click="toggleMax"><Icon name="win_max" :size="13" /></button>
      <button class="tb-btn close" title="关闭" @click="close"><Icon name="win_close" :size="15" /></button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  position: relative;
  z-index: 50;
  height: var(--titlebar-h);
  display: flex;
  align-items: center;
  flex: none;
  padding-left: 14px;
  user-select: none;
}
.tb-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text-2);
}
.tb-name b {
  font-weight: 600;
  color: var(--text);
}
.tb-ver {
  font-size: 11px;
  color: var(--text-3);
  padding: 1px 6px;
  border-radius: 6px;
  background: var(--hover);
}
.tb-drag {
  flex: 1;
  height: 100%;
}
.tb-controls {
  display: flex;
  height: 100%;
}
.tb-controls.inert {
  opacity: 0.55;
}
.tb-btn {
  width: 46px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--text-2);
  display: grid;
  place-items: center;
  cursor: pointer;
  transition: background var(--t-fast), color var(--t-fast);
}
.tb-btn:hover {
  background: var(--hover);
  color: var(--text);
}
.tb-btn.close:hover {
  background: #e81123;
  color: #fff;
}
</style>
