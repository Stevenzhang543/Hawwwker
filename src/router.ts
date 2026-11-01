import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";
import { useAppStore } from "@/stores/app";

const routes: RouteRecordRaw[] = [
  { path: "/onboarding", name: "onboarding", component: () => import("@/views/onboarding/Onboarding.vue"), meta: { bare: true } },
  {
    path: "/",
    component: () => import("@/components/shell/AppShell.vue"),
    children: [
      { path: "", name: "home", redirect: () => useAppStore().homeRoute },
      { path: "discover", redirect: "/discover/search" },
      { path: "discover/search", name: "search", component: () => import("@/views/discover/Search.vue") },
      { path: "discover/history", name: "history", component: () => import("@/views/discover/History.vue") },
      { path: "track", redirect: "/track/watchlist" },
      { path: "track/watchlist", name: "watchlist", component: () => import("@/views/track/Watchlist.vue") },
      { path: "track/monitors", name: "monitors", component: () => import("@/views/track/Monitors.vue") },
      { path: "track/alerts", name: "alerts", component: () => import("@/views/track/Alerts.vue") },
      { path: "insights", redirect: "/insights/trends" },
      { path: "insights/trends", name: "trends", component: () => import("@/views/insights/Trends.vue") },
      { path: "insights/market", name: "market", component: () => import("@/views/insights/Market.vue") },
      { path: "platforms", redirect: "/platforms/accounts" },
      { path: "platforms/accounts", name: "accounts", component: () => import("@/views/platforms/Accounts.vue") },
      { path: "platforms/catalog", name: "catalog", component: () => import("@/views/platforms/Catalog.vue") },
      { path: "settings", redirect: "/settings/ai" },
      { path: "settings/ai", name: "settings-ai", component: () => import("@/views/settings/AiSettings.vue"), meta: { aiFree: true } },
      { path: "settings/general", name: "settings-general", component: () => import("@/views/settings/General.vue"), meta: { aiFree: true } },
      { path: "settings/notify", name: "settings-notify", component: () => import("@/views/settings/Notify.vue"), meta: { aiFree: true } },
      { path: "settings/data", name: "settings-data", component: () => import("@/views/settings/Data.vue"), meta: { aiFree: true } },
    ],
  },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
});

/**
 * 守卫：AI 必须接入。
 * - 未完成引导 → /onboarding
 * - 已引导但 AI 未验证（Key 失效等）→ /settings/ai（设置页可访问，其余页面拦截）
 */
router.beforeEach((to) => {
  const app = useAppStore();
  if (!app.ready) return true;
  const onboarded = !!app.settings?.onboarded;
  if (to.name === "onboarding") {
    if (onboarded && app.ai.verified) return app.homeRoute;
    return true;
  }
  if (!onboarded) return { name: "onboarding" };
  if (!app.ai.verified && !to.meta.aiFree) return { path: "/settings/ai", query: { gate: "1" } };
  return true;
});
