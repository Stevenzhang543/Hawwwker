<script setup lang="ts">
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { NAV, type NavGroup } from "@/nav";
import { ROLE_HOME, ROLE_LABEL, useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { useSearchStore } from "@/stores/search";
import { useNoticeStore } from "@/stores/notices";
import { useWatchStore } from "@/stores/watch";
import { useWindowWidth } from "@/composables/motion";
import type { Role } from "@/api/types";
import Icon from "@/components/ui/Icon.vue";
import BrandMark from "./BrandMark.vue";

const app = useAppStore();
const ui = useUiStore();
const search = useSearchStore();
const notices = useNoticeStore();
const watch = useWatchStore();
const route = useRoute();
const router = useRouter();
const width = useWindowWidth();

const rail = computed(() => width.value < 1100 || !!app.settings?.sidebarCollapsed);
const collapsed = ref<Record<string, boolean>>({});
const roleOpen = ref(false);
const flyout = ref<string | null>(null);
let flyTimer = 0;

const activeGroup = computed(() => NAV.find((g) => g.items.some((i) => route.path.startsWith(i.path)))?.key);

function badge(path: string): string | null {
  if (path === "/discover/search" && search.active.length) return "live";
  if (path === "/track/alerts" && notices.unread) return String(notices.unread > 99 ? "99+" : notices.unread);
  if (path === "/track/watchlist" && watch.hitTarget.length) return String(watch.hitTarget.length);
  return null;
}

function toggleGroup(g: NavGroup) {
  collapsed.value = { ...collapsed.value, [g.key]: !collapsed.value[g.key] };
}

async function setRole(r: Role) {
  roleOpen.value = false;
  if (r === app.role) return;
  await app.update({ role: r });
  ui.toast(`已切换为「${ROLE_LABEL[r]}」工作台`, "success", 2200);
  void router.push(ROLE_HOME[r]);
}

function toggleRail() {
  void app.update({ sidebarCollapsed: !app.settings?.sidebarCollapsed });
}

function showFly(key: string) {
  window.clearTimeout(flyTimer);
  flyout.value = key;
}
function hideFly() {
  flyTimer = window.setTimeout(() => (flyout.value = null), 160);
}
const roles: Role[] = ["consumer", "merchant", "analyst"];
const roleIcon: Record<Role, string> = { consumer: "cart", merchant: "shop", analyst: "chart" };
</script>

<template>
  <aside class="sidebar glass" :class="{ rail }">
    <div class="sb-top">
      <div class="brand">
        <BrandMark :size="rail ? 34 : 30" />
        <div v-if="!rail" class="brand-text">
          <strong>Hawwwker</strong>
          <small>货客 · AI 比价</small>
        </div>
      </div>

      <div class="role" :class="{ open: roleOpen }">
        <button v-ripple class="role-pill" :title="'角色：' + ROLE_LABEL[app.role]" @click="roleOpen = !roleOpen">
          <Icon :name="roleIcon[app.role]" :size="16" />
          <template v-if="!rail">
            <span>{{ ROLE_LABEL[app.role] }}工作台</span>
            <Icon name="down" :size="14" class="caret" />
          </template>
        </button>
        <Transition name="pop">
          <div v-if="roleOpen" class="role-menu glass-3" @mouseleave="roleOpen = false">
            <button v-for="r in roles" :key="r" class="role-opt" :class="{ on: r === app.role }" @click="setRole(r)">
              <Icon :name="roleIcon[r]" :size="16" />
              <span>{{ ROLE_LABEL[r] }}</span>
              <Icon v-if="r === app.role" name="check" :size="14" class="ok" />
            </button>
          </div>
        </Transition>
      </div>
    </div>

    <nav class="nav">
      <div
        v-for="(g, gi) in NAV"
        :key="g.key"
        class="group"
        :class="{ active: activeGroup === g.key, closed: collapsed[g.key] }"
        @mouseenter="rail && showFly(g.key)"
        @mouseleave="rail && hideFly()"
      >
        <button v-if="!rail" class="group-head" @click="toggleGroup(g)">
          <span>{{ g.label }}</span>
          <small>{{ g.en }}</small>
          <span class="spacer"></span>
          <kbd class="alt">Alt {{ gi + 1 }}</kbd>
          <Icon name="down" :size="13" class="chev" />
        </button>
        <router-link v-else :to="g.items[0]?.path ?? '/'" class="rail-btn" :class="{ on: activeGroup === g.key }" :title="g.label">
          <Icon :name="g.icon" :size="20" />
          <i v-if="g.items.some((i) => badge(i.path))" class="rail-dot"></i>
        </router-link>

        <div v-if="!rail" class="items">
          <router-link v-for="it in g.items" :key="it.path" v-ripple :to="it.path" class="item" active-class="on">
            <Icon :name="it.icon" :size="17" />
            <span>{{ it.label }}</span>
            <span class="spacer"></span>
            <b v-if="badge(it.path) === 'live'" class="live"><i></i></b>
            <b v-else-if="badge(it.path)" class="badge">{{ badge(it.path) }}</b>
          </router-link>
        </div>

        <Transition name="pop">
          <div v-if="rail && flyout === g.key" class="flyout glass-3" @mouseenter="showFly(g.key)" @mouseleave="hideFly">
            <div class="fly-title">{{ g.label }}<small>{{ g.en }}</small></div>
            <router-link v-for="it in g.items" :key="it.path" :to="it.path" class="item" active-class="on" @click="flyout = null">
              <Icon :name="it.icon" :size="17" />
              <span>{{ it.label }}</span>
              <span class="spacer"></span>
              <b v-if="badge(it.path) && badge(it.path) !== 'live'" class="badge">{{ badge(it.path) }}</b>
            </router-link>
          </div>
        </Transition>
      </div>
    </nav>

    <div class="sb-foot">
      <button v-ripple v-magnet="0.12" class="copilot-btn" :title="'Copilot（Ctrl+J）'" @click="ui.copilotOpen = !ui.copilotOpen">
        <span class="cp-orb"><Icon name="bot" :size="17" /></span>
        <template v-if="!rail">
          <span class="cp-text"><b>Copilot</b><small>礼物 · 比价 · 行业判断</small></span>
          <kbd>Ctrl J</kbd>
        </template>
      </button>
      <router-link v-if="!rail" to="/settings/ai" class="ai-state" :class="{ bad: !app.ai.verified }">
        <span class="dot" :class="app.ai.verified ? 'green' : 'red'"></span>
        <span class="ellipsis">{{ app.ai.verified ? `AI 已连接 · ${app.ai.model}` : "AI 未连接" }}</span>
      </router-link>
      <button v-if="width >= 1100" class="collapse-btn icon-btn" :title="rail ? '展开侧栏' : '收起侧栏'" @click="toggleRail">
        <Icon name="sidebar" :size="16" />
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-w);
  flex: none;
  display: flex;
  flex-direction: column;
  padding: 14px 10px 10px;
  border-radius: var(--r-xl);
  transition: width 420ms var(--spring-soft);
  z-index: 20;
}
.sidebar.rail {
  width: var(--rail-w);
  padding: 14px 8px 10px;
  align-items: center;
}
.sb-top {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 0 4px 10px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 4px;
}
.brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.2;
}
.brand-text strong {
  font-size: 16px;
  letter-spacing: -0.01em;
}
.brand-text small {
  font-size: 11.5px;
  color: var(--text-3);
}
.role {
  position: relative;
}
.role-pill {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 10px;
  border-radius: 11px;
  border: 1px solid var(--line-2);
  background: var(--glass-2);
  font-size: 13px;
  font-weight: 560;
  cursor: pointer;
  color: var(--text);
  transition: transform var(--t-mid) var(--spring);
}
.rail .role-pill {
  width: 40px;
  padding: 0;
  justify-content: center;
}
.role-pill:active {
  transform: scale(0.96);
}
.role-pill .caret {
  margin-left: auto;
  color: var(--text-3);
  transition: transform var(--t-mid) var(--spring);
}
.role.open .caret {
  transform: rotate(180deg);
}
.role-menu {
  position: absolute;
  top: 40px;
  left: 0;
  min-width: 180px;
  padding: 6px;
  border-radius: 14px;
  z-index: 40;
}
.role-opt {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 10px;
  border: none;
  background: transparent;
  border-radius: 9px;
  cursor: pointer;
  text-align: left;
  font-size: 13.5px;
}
.role-opt:hover {
  background: var(--hover);
}
.role-opt.on {
  color: var(--brand-ink);
  font-weight: 600;
}
.role-opt .ok {
  margin-left: auto;
}

.nav {
  flex: 1;
  overflow-y: auto;
  overflow-x: visible;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px 0;
  margin: 0 -4px;
  padding: 4px;
}
.rail .nav {
  overflow: visible;
  align-items: center;
  gap: 6px;
}
.group {
  position: relative;
}
.group-head {
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: 6px;
  padding: 6px 8px 3px;
  border: none;
  background: transparent;
  cursor: pointer;
  color: var(--text-3);
  font-size: 12px;
  font-weight: 650;
  letter-spacing: 0.04em;
}
.group-head small {
  font-size: 10.5px;
  font-weight: 500;
  opacity: 0.8;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.group-head .alt {
  opacity: 0;
  height: 16px;
  font-size: 10px;
  transition: opacity var(--t-fast);
}
.group-head:hover .alt {
  opacity: 1;
}
.group-head .chev {
  align-self: center;
  transition: transform var(--t-mid) var(--spring);
}
.group.closed .chev {
  transform: rotate(-90deg);
}
.group.active .group-head {
  color: var(--text-2);
}
.items {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 360ms var(--spring-soft), opacity 240ms;
}
.group.closed .items {
  grid-template-rows: 0fr;
  opacity: 0;
  overflow: hidden;
}
.items > * {
  min-height: 0;
}
.item {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 34px;
  padding: 0 10px;
  margin: 1px 0;
  border-radius: 11px;
  color: var(--text-2);
  font-size: 13.5px;
  font-weight: 540;
  transition: background var(--t-fast), color var(--t-fast), transform var(--t-mid) var(--spring);
  overflow: hidden;
}
.items .item {
  overflow: hidden;
}
.item:hover {
  background: var(--hover);
  color: var(--text);
}
.item:active {
  transform: scale(0.97);
}
.item.on {
  color: var(--text);
  background: var(--glass-2);
  box-shadow: var(--shadow-1), inset 0 0 0 1px var(--line);
  font-weight: 620;
}
.item.on :deep(.ico) {
  color: var(--brand);
}
.badge {
  min-width: 20px;
  height: 18px;
  padding: 0 6px;
  border-radius: 9px;
  background: var(--brand-grad);
  color: #fff;
  font-size: 11px;
  display: grid;
  place-items: center;
}
.live {
  width: 18px;
  height: 18px;
  display: grid;
  place-items: center;
}
.live i {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--amber);
  box-shadow: 0 0 0 0 rgba(255, 176, 32, 0.6);
  animation: ping 1.4s infinite;
}
@keyframes ping {
  70% {
    box-shadow: 0 0 0 7px rgba(255, 176, 32, 0);
  }
  100% {
    box-shadow: 0 0 0 0 rgba(255, 176, 32, 0);
  }
}

.rail-btn {
  position: relative;
  width: 44px;
  height: 44px;
  border-radius: 13px;
  display: grid;
  place-items: center;
  color: var(--text-2);
  transition: background var(--t-fast), color var(--t-fast), transform var(--t-mid) var(--spring);
}
.rail-btn:hover {
  background: var(--hover);
  color: var(--text);
}
.rail-btn.on {
  background: var(--glass-2);
  color: var(--brand);
  box-shadow: var(--shadow-1);
}
.rail-dot {
  position: absolute;
  top: 9px;
  right: 9px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--amber);
}
.flyout {
  position: absolute;
  left: 54px;
  top: -6px;
  width: 210px;
  padding: 8px;
  border-radius: 16px;
  z-index: 60;
}
.fly-title {
  padding: 4px 10px 8px;
  font-weight: 650;
  font-size: 13px;
  display: flex;
  gap: 6px;
  align-items: baseline;
}
.fly-title small {
  font-size: 10.5px;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.sb-foot {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
  position: relative;
}
.copilot-btn {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 9px 10px;
  border-radius: 14px;
  border: 1px solid rgba(138, 92, 255, 0.25);
  background: linear-gradient(135deg, rgba(61, 123, 255, 0.14), rgba(138, 92, 255, 0.16));
  cursor: pointer;
  text-align: left;
  color: var(--text);
  transition: transform var(--t-mid) var(--spring), box-shadow var(--t-mid);
}
.copilot-btn:hover {
  box-shadow: 0 8px 22px rgba(110, 90, 255, 0.22);
}
.rail .copilot-btn {
  width: 46px;
  height: 46px;
  padding: 0;
  justify-content: center;
}
.cp-orb {
  width: 30px;
  height: 30px;
  flex: none;
  border-radius: 10px;
  display: grid;
  place-items: center;
  background: var(--brand-grad);
  color: #fff;
  box-shadow: 0 4px 12px rgba(80, 100, 255, 0.4);
}
.cp-text {
  display: flex;
  flex-direction: column;
  line-height: 1.25;
  min-width: 0;
  flex: 1;
}
.cp-text b {
  font-size: 13.5px;
}
.cp-text small {
  font-size: 11px;
  color: var(--text-3);
  white-space: nowrap;
}
.ai-state {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  font-size: 12px;
  color: var(--text-3);
  border-radius: 8px;
  min-width: 0;
  padding-right: 36px;
}
.ai-state:hover {
  background: var(--hover);
}
.ai-state.bad {
  color: var(--err);
}
.collapse-btn {
  position: absolute;
  right: 0;
  bottom: -2px;
  width: 30px;
  height: 30px;
}
.rail .collapse-btn {
  position: static;
  margin: 0 auto;
}
</style>
