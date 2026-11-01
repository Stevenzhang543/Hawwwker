<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { findNav } from "@/nav";
import { useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { useSearchStore } from "@/stores/search";
import { useNoticeStore } from "@/stores/notices";
import Icon from "@/components/ui/Icon.vue";
import NotificationPopover from "./NotificationPopover.vue";

const props = defineProps<{ elRef?: (el: HTMLElement | null) => void }>();
const emit = defineEmits<{ goRun: [] }>();

const app = useAppStore();
const ui = useUiStore();
const search = useSearchStore();
const notices = useNoticeStore();
const route = useRoute();
const root = ref<HTMLElement | null>(null);
const bellOpen = ref(false);

const crumb = computed(() => findNav(route.path));
const running = computed(() => search.active[0] ?? null);
const runningPct = computed(() => Math.round((running.value?.progress ?? 0) * 100));

onMounted(() => props.elRef?.(root.value));
</script>

<template>
  <header ref="root" class="topbar">
    <div class="crumb">
      <template v-if="crumb">
        <span class="c1">{{ crumb.group.label }}</span>
        <Icon name="chevron" :size="14" class="sep" />
        <span class="c2">{{ crumb.item.label }}</span>
      </template>
    </div>

    <button v-ripple class="palette-trigger" @click="ui.paletteOpen = true">
      <Icon name="search" :size="16" />
      <span>搜索商品、输入指令或跳转…</span>
      <kbd>Ctrl K</kbd>
    </button>

    <div class="actions">
      <Transition name="pop">
        <button v-if="running" class="run-pill" @click="emit('goRun')">
          <svg class="ring" viewBox="0 0 20 20">
            <circle cx="10" cy="10" r="8" class="bg" />
            <circle cx="10" cy="10" r="8" class="fg" :style="{ strokeDashoffset: 50.3 - (50.3 * runningPct) / 100 }" />
          </svg>
          <span>检索中 {{ runningPct }}%</span>
        </button>
      </Transition>
      <button class="icon-btn" :title="app.resolvedTheme === 'dark' ? '切换为浅色' : '切换为深色'" @click="app.toggleTheme()">
        <Transition name="pop" mode="out-in">
          <Icon :key="app.resolvedTheme" :name="app.resolvedTheme === 'dark' ? 'sun' : 'moon'" />
        </Transition>
      </button>
      <div class="bell-wrap">
        <button class="icon-btn" title="通知" @click="bellOpen = !bellOpen">
          <Icon name="bell" />
          <b v-if="notices.unread" class="bell-dot">{{ notices.unread > 9 ? "9+" : notices.unread }}</b>
        </button>
        <NotificationPopover v-if="bellOpen" @close="bellOpen = false" />
      </div>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  --sb: 0;
  position: absolute;
  z-index: 15;
  top: 0;
  left: 0;
  right: 0;
  height: 58px;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 18px 0 24px;
  background: color-mix(in srgb, var(--glass-2) calc(var(--sb) * 100%), transparent);
  -webkit-backdrop-filter: blur(calc(var(--sb) * 26px)) saturate(calc(100% + var(--sb) * 70%));
  backdrop-filter: blur(calc(var(--sb) * 26px)) saturate(calc(100% + var(--sb) * 70%));
  border-bottom: 1px solid color-mix(in srgb, var(--line) calc(var(--sb) * 100%), transparent);
  box-shadow: 0 10px 30px -18px rgba(20, 30, 60, calc(var(--sb) * 0.35));
  transition: background 120ms linear;
}
.crumb {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 160px;
  font-size: 13px;
}
.c1 {
  color: var(--text-3);
}
.c2 {
  font-weight: 650;
  color: var(--text);
  font-size: 14px;
}
.sep {
  color: var(--text-3);
}
.palette-trigger {
  flex: 1;
  max-width: 460px;
  margin: 0 auto;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 36px;
  padding: 0 8px 0 12px;
  border-radius: 12px;
  border: 1px solid var(--line-2);
  background: var(--glass-1);
  -webkit-backdrop-filter: blur(14px);
  backdrop-filter: blur(14px);
  color: var(--text-3);
  cursor: text;
  transition: border-color var(--t-fast), box-shadow var(--t-mid), transform var(--t-mid) var(--spring);
}
.palette-trigger:hover {
  border-color: color-mix(in srgb, var(--brand-1) 45%, transparent);
  box-shadow: 0 0 0 4px rgba(61, 123, 255, 0.1);
}
.palette-trigger span {
  flex: 1;
  text-align: left;
  font-size: 13px;
}
.actions {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 160px;
  justify-content: flex-end;
}
.run-pill {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 12px 0 8px;
  border-radius: 999px;
  border: 1px solid rgba(255, 176, 32, 0.4);
  background: rgba(255, 176, 32, 0.12);
  color: var(--amber-ink);
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  margin-right: 6px;
}
.ring {
  width: 20px;
  height: 20px;
  transform: rotate(-90deg);
}
.ring circle {
  fill: none;
  stroke-width: 2.6;
}
.ring .bg {
  stroke: rgba(255, 176, 32, 0.25);
}
.ring .fg {
  stroke: var(--amber);
  stroke-dasharray: 50.3;
  stroke-linecap: round;
  transition: stroke-dashoffset 400ms var(--ease-out);
}
.bell-wrap {
  position: relative;
}
.bell-dot {
  position: absolute;
  top: 3px;
  right: 2px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 8px;
  background: var(--err);
  color: #fff;
  font-size: 10px;
  display: grid;
  place-items: center;
  box-shadow: 0 0 0 2px var(--bg-solid);
}
</style>
