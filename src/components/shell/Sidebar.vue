<script setup lang="ts">
/**
 * 侧栏（v26.02）：只保留 5 个一级入口 + Copilot + 状态。二级页面在页面顶部以标签显示（PageTabs）。
 * 点一级入口回到该入口最近访问的标签页。
 */
import { computed, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { NAV, findGroup, type NavGroup } from "@/nav";
import { ROLE_HOME, ROLE_LABEL, useAppStore } from "@/stores/app";
import { useUiStore } from "@/stores/ui";
import { useWindowWidth } from "@/composables/motion";
import type { Role } from "@/api/types";
import Icon from "@/components/ui/Icon.vue";
import BrandMark from "./BrandMark.vue";
import { groupBadge, useNavBadges } from "./navBadges";

const app = useAppStore();
const ui = useUiStore();
const route = useRoute();
const router = useRouter();
const width = useWindowWidth();
const badgeOf = useNavBadges();

const rail = computed(() => width.value < 1100 || !!app.settings?.sidebarCollapsed);
const roleOpen = ref(false);

const activeGroup = computed(() => findGroup(route.path)?.key);
const badges = computed(() => Object.fromEntries(NAV.map((g) => [g.key, groupBadge(g, badgeOf)])) as Record<string, string | null>);

function target(g: NavGroup): string {
  const last = ui.lastTab[g.key];
  return last && g.items.some((i) => last.startsWith(i.path)) ? last : g.items[0]?.path ?? "/";
}
function go(g: NavGroup) {
  // 已在该入口内：再点一次回到第一个标签
  const to = activeGroup.value === g.key ? g.items[0]?.path ?? "/" : target(g);
  if (to !== route.fullPath) void router.push(to);
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
      <button
        v-for="(g, gi) in NAV"
        :key="g.key"
        v-ripple
        class="entry"
        :class="{ on: activeGroup === g.key, rail }"
        :title="rail ? `${g.label} · ${g.items.map((i) => i.label).join(' / ')}` : g.items.map((i) => i.label).join(' · ')"
        @click="go(g)"
      >
        <span class="e-ico"><Icon :name="g.icon" :size="rail ? 20 : 18" /></span>
        <template v-if="!rail">
          <span class="e-label">{{ g.label }}</span>
          <span class="spacer"></span>
          <b v-if="badges[g.key] === 'live'" class="live"><i></i></b>
          <b v-else-if="badges[g.key]" class="badge">{{ badges[g.key] }}</b>
          <kbd v-else class="alt">Alt {{ gi + 1 }}</kbd>
        </template>
        <i v-else-if="badges[g.key]" class="rail-dot"></i>
      </button>
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
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0 -4px;
  padding: 8px 4px;
}
.rail .nav {
  overflow: visible;
  align-items: center;
  gap: 6px;
}
.entry {
  position: relative;
  display: flex;
  align-items: center;
  gap: 11px;
  height: 42px;
  padding: 0 10px 0 6px;
  border: none;
  background: transparent;
  border-radius: 13px;
  color: var(--text-2);
  font-size: 14px;
  font-weight: 580;
  cursor: pointer;
  text-align: left;
  transition: background var(--t-fast), color var(--t-fast), transform var(--t-mid) var(--spring), box-shadow var(--t-mid);
}
.entry:hover {
  background: var(--hover);
  color: var(--text);
}
.entry:active {
  transform: scale(0.97);
}
.entry.on {
  color: var(--text);
  background: var(--glass-2);
  box-shadow: var(--shadow-1), inset 0 0 0 1px var(--line);
  font-weight: 650;
}
.e-ico {
  width: 30px;
  height: 30px;
  flex: none;
  border-radius: 10px;
  display: grid;
  place-items: center;
  transition: background var(--t-mid), color var(--t-mid);
}
.entry.on .e-ico {
  background: var(--brand-grad);
  color: #fff;
  box-shadow: 0 4px 12px rgba(80, 100, 255, 0.32);
}
.entry.rail {
  width: 46px;
  height: 46px;
  padding: 0;
  justify-content: center;
}
.entry.rail .e-ico {
  width: 36px;
  height: 36px;
  border-radius: 12px;
}
.entry .alt {
  opacity: 0;
  height: 16px;
  font-size: 10px;
  transition: opacity var(--t-fast);
}
.entry:hover .alt {
  opacity: 1;
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
.rail-dot {
  position: absolute;
  top: 7px;
  right: 7px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--amber);
  box-shadow: 0 0 0 2px var(--bg-solid);
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
