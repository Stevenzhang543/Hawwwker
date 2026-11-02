import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import { on } from "@/api/events";
import type { CustomPlatformInput, LoginCheck, LoginState, PlatformInfo, PlatformPatch, Region } from "@/api/types";

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
  /** 「我已登录」/「验证」：流程中立即确认；没有流程时主动验证一次。返回最新状态（message 为验证结论）。 */
  async function loginConfirm(id: string) {
    const s = await api.loginConfirm(id);
    applyLogin(s);
    return s;
  }
  async function loginCancel(id: string) {
    applyLogin(await api.loginCancel(id));
  }
  async function logout(id: string) {
    const s = await api.logout(id);
    applyLogin(s);
    return s;
  }

  /** v26.03 登录体检：主动验证所有已启用、需要登录的平台 */
  const checkingAll = ref(false);
  async function checkAll() {
    checkingAll.value = true;
    try {
      const states = await api.loginCheckAll();
      logins.value = Object.fromEntries(states.map((s) => [s.platformId, s]));
      return states;
    } finally {
      checkingAll.value = false;
    }
  }

  /* ------------------------------------------------------------ v26.03 登录门槛 */
  /** 当前打开的登录门槛：需要用户处理的预检结果 */
  const gate = ref<{ ids: string[]; checks: LoginCheck[]; othersRemain?: boolean } | null>(null);
  const preflighting = ref(false);
  let gateResolve: ((ids: string[] | null) => void) | null = null;

  function localCheck(id: string): LoginCheck {
    const p = byId.value[id];
    const s = logins.value[id];
    const need = p?.login ?? "optional";
    const status = s?.status ?? "unknown";
    return { platformId: id, need, status, ok: need === "optional" || status === "logged_in" || status === "not_needed", verifiedNow: false };
  }

  /** 门槛里某个平台现在是否可以直接搜索（实时跟随 login://state） */
  function readyFor(id: string): boolean {
    return localCheck(id).ok;
  }

  /**
   * v26.03 登录门槛：搜索前调用。所选平台里需要登录但没登录的，弹出登录门槛（LoginGate），
   * 用户登录完成 / 移除这些平台 / 选择"不登录也搜"（仅限"建议登录"）后 resolve。
   * 返回最终可以搜索的平台 id；null = 用户取消，不要发起搜索。
   */
  /**
   * opts.othersRemain：本次搜索还有其他（已就绪的）平台，因此即使把这里列出的平台全部移除也可以继续
   * （例如后端 LOGIN_REQUIRED 只返回了未登录的那几个）。
   */
  async function ensureLoggedIn(platformIds: string[], opts?: { othersRemain?: boolean }): Promise<string[] | null> {
    const ids = [...new Set(platformIds)].filter((id) => byId.value[id] || !list.value.length);
    if (!ids.length) return platformIds;
    let checks: LoginCheck[];
    preflighting.value = true;
    try {
      checks = await api.loginPreflight(ids, true);
    } catch {
      // 预检失败（例如浏览器暂时启动不了）：按本机记录的状态判断
      checks = ids.map(localCheck);
    } finally {
      preflighting.value = false;
    }
    for (const c of checks) {
      const cur = logins.value[c.platformId];
      if (cur && cur.status !== c.status) logins.value = { ...logins.value, [c.platformId]: { ...cur, status: c.status } };
    }
    if (checks.every((c) => c.ok)) return ids;
    settleGate(null); // 已有门槛：先取消旧的
    gate.value = { ids, checks: checks.filter((c) => !c.ok), othersRemain: !!opts?.othersRemain };
    return new Promise((resolve) => {
      gateResolve = resolve;
    });
  }

  /** 关闭门槛并返回结果（LoginGate 调用） */
  function settleGate(result: string[] | null) {
    const r = gateResolve;
    gateResolve = null;
    gate.value = null;
    r?.(result);
  }


  return {
    list, logins, byId, enabled, families, set, listen, label, inRegion, refresh, refreshLogins, update, addCustom, removeCustom,
    loginStart, loginConfirm, loginCancel, logout, ensureLoggedIn, gate, preflighting, settleGate, readyFor, checkAll, checkingAll,
  };
});
