import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import { on } from "@/api/events";
import type { CustomPlatformInput, LoginState, PlatformInfo, PlatformPatch, Region } from "@/api/types";

export const LOGIN_LABEL: Record<LoginState["status"], string> = {
  unknown: "未检测",
  logged_out: "未登录",
  logged_in: "已登录",
  expired: "已过期",
  not_needed: "无需登录",
};

export const KIND_LABEL: Record<PlatformInfo["kind"], string> = {
  retail: "零售",
  marketplace: "综合电商",
  c2c: "二手",
  social: "社区",
  wholesale: "批发",
  compare: "比价",
};

export const usePlatformStore = defineStore("platforms", () => {
  const list = ref<PlatformInfo[]>([]);
  const logins = ref<Record<string, LoginState>>({});
  let listening = false;

  const byId = computed(() => Object.fromEntries(list.value.map((p) => [p.id, p])) as Record<string, PlatformInfo>);
  const enabled = computed(() => list.value.filter((p) => p.enabled));
  const families = computed(() => {
    const m = new Map<string, { family: string; name: string; editions: PlatformInfo[] }>();
    for (const p of list.value) {
      let f = m.get(p.family);
      if (!f) m.set(p.family, (f = { family: p.family, name: p.familyName, editions: [] }));
      f.editions.push(p);
    }
    for (const f of m.values()) f.editions.sort((a, b) => (a.region === b.region ? 0 : a.region === "cn" ? -1 : 1));
    return [...m.values()];
  });

  function set(platforms: PlatformInfo[], states: LoginState[]) {
    list.value = platforms;
    logins.value = Object.fromEntries(states.map((s) => [s.platformId, s]));
  }

  async function listen() {
    if (listening) return;
    listening = true;
    await on("login://state", (s) => {
      logins.value = { ...logins.value, [s.platformId]: s };
    });
  }

  function label(id: string, withEdition = true): string {
    const p = byId.value[id];
    if (!p) return id;
    if (!withEdition) return p.name;
    const sameName = list.value.filter((o) => o.name === p.name).length > 1;
    return sameName ? `${p.name} · ${p.edition}` : p.name;
  }

  function inRegion(r: "all" | Region) {
    return list.value.filter((p) => r === "all" || p.region === r);
  }

  async function refresh() {
    list.value = await api.platformsList();
  }
  async function refreshLogins(force = false) {
    const states = await api.loginStates(force);
    logins.value = Object.fromEntries(states.map((s) => [s.platformId, s]));
  }

  function upsert(p: PlatformInfo) {
    const i = list.value.findIndex((x) => x.id === p.id);
    if (i >= 0) list.value.splice(i, 1, p);
    else list.value.push(p);
  }
  async function update(id: string, patch: PlatformPatch) {
    const p = await api.platformUpdate(id, patch);
    upsert(p);
    return p;
  }
  async function addCustom(input: CustomPlatformInput) {
    const p = await api.platformAddCustom(input);
    upsert(p);
    await refreshLogins();
    return p;
  }
  async function removeCustom(id: string) {
    await api.platformRemoveCustom(id);
    list.value = list.value.filter((p) => p.id !== id);
  }

  function applyLogin(s: LoginState) {
    logins.value = { ...logins.value, [s.platformId]: s };
  }
  async function loginStart(id: string) {
    applyLogin(await api.loginStart(id));
  }
  async function loginConfirm(id: string) {
    applyLogin(await api.loginConfirm(id));
  }
  async function loginCancel(id: string) {
    applyLogin(await api.loginCancel(id));
  }
  async function logout(id: string) {
    applyLogin(await api.logout(id));
  }

  return {
    list, logins, byId, enabled, families, set, listen, label, inRegion, refresh, refreshLogins, update, addCustom, removeCustom,
    loginStart, loginConfirm, loginCancel, logout,
  };
});
