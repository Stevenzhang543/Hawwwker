import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { api, isTauri, toAppError } from "@/api";
import type { AiStatus, AppSettings, AppSettingsPatch, FxRates, Role } from "@/api/types";
import { convert } from "@/utils/format";

export const ROLE_LABEL: Record<Role, string> = { consumer: "消费者", merchant: "商户", analyst: "平台分析" };
export const ROLE_HOME: Record<Role, string> = {
  consumer: "/discover/search",
  merchant: "/track/watchlist",
  analyst: "/insights/market",
};

export const useAppStore = defineStore("app", () => {
  const ready = ref(false);
  const bootError = ref<string | null>(null);
  const version = ref("26.01");
  const dataDir = ref("");
  const settings = ref<AppSettings | null>(null);
  const ai = ref<AiStatus>({ configured: false, verified: false, model: "", provider: "" });
  const fx = ref<FxRates | null>(null);
  const systemDark = ref(typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches);

  const role = computed<Role>(() => settings.value?.role ?? "consumer");
  const homeRoute = computed(() => ROLE_HOME[role.value]);
  const displayCurrency = computed(() => settings.value?.displayCurrency ?? "CNY");
  const resolvedTheme = computed<"light" | "dark">(() => {
    const t = settings.value?.theme ?? "system";
    return t === "system" ? (systemDark.value ? "dark" : "light") : t;
  });
  const aiReady = computed(() => ai.value.verified);

  async function bootstrap() {
    try {
      const b = await api.bootstrap();
      version.value = b.version;
      dataDir.value = b.dataDir;
      settings.value = b.settings;
      ai.value = b.ai;
      fx.value = b.fx;
      ready.value = true;
      return b;
    } catch (e) {
      bootError.value = toAppError(e).message;
      throw e;
    }
  }

  async function update(patch: AppSettingsPatch) {
    settings.value = await api.settingsUpdate(patch);
    return settings.value;
  }

  function setAi(s: AiStatus) {
    ai.value = s;
  }

  function toDisplay(amount: number | null | undefined, from: string): number | null {
    return convert(amount, from, displayCurrency.value, fx.value);
  }

  /* ---------------- 主题 / 动效 / 窗口特效 应用到 DOM 与原生窗口 */
  function applyDom() {
    const root = document.documentElement;
    const s = settings.value;
    root.dataset.theme = resolvedTheme.value;
    root.dataset.motion = s?.motion === "reduced" ? "reduced" : "full";
    root.dataset.effect = isTauri ? (s?.windowEffect ?? "mica") : "none";
  }

  async function applyNative() {
    if (!isTauri || !settings.value) return;
    try {
      const { getCurrentWindow, Effect } = await import("@tauri-apps/api/window");
      const w = getCurrentWindow();
      const t = settings.value.theme;
      await w.setTheme(t === "system" ? null : t);
      const eff = settings.value.windowEffect;
      if (eff === "none") await w.clearEffects();
      else await w.setEffects({ effects: [eff === "mica" ? Effect.Mica : Effect.Acrylic] });
    } catch (e) {
      console.warn("window effect", e);
    }
  }

  if (typeof matchMedia !== "undefined") {
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
      systemDark.value = e.matches;
    });
  }

  watch(
    () => [resolvedTheme.value, settings.value?.motion, settings.value?.windowEffect],
    () => applyDom(),
    { immediate: true },
  );
  watch(
    () => [settings.value?.theme, settings.value?.windowEffect],
    () => void applyNative(),
  );

  function toggleTheme() {
    const next = resolvedTheme.value === "dark" ? "light" : "dark";
    void update({ theme: next });
  }

  return {
    ready, bootError, version, dataDir, settings, ai, fx, role, homeRoute, displayCurrency, resolvedTheme, aiReady,
    bootstrap, update, setAi, toDisplay, toggleTheme, applyDom, applyNative,
  };
});
