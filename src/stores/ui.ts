import { defineStore } from "pinia";
import { computed, ref } from "vue";

export type IslandKind = "info" | "success" | "warn" | "error" | "ai";
export interface IslandMsg {
  id: number;
  text: string;
  kind: IslandKind;
  sub?: string;
}

export interface ConfirmReq {
  title: string;
  body: string;
  okText: string;
  danger: boolean;
  resolve: (v: boolean) => void;
}

/** 全局 UI：灵动岛、命令面板、Copilot 抽屉、确认框、景深层级 */
export const useUiStore = defineStore("ui", () => {
  const island = ref<IslandMsg | null>(null);
  let islandTimer = 0;
  let seq = 0;
  const paletteOpen = ref(false);
  const copilotOpen = ref(false);
  const confirmReq = ref<ConfirmReq | null>(null);
  const sheets = ref(0);
  /** Esc 关闭栈：后注册的先关 */
  const escStack: (() => void)[] = [];

  const depth = computed(() => sheets.value + (paletteOpen.value ? 1 : 0) + (confirmReq.value ? 1 : 0));

  function toast(text: string, kind: IslandKind = "info", ms = 2800, sub?: string) {
    window.clearTimeout(islandTimer);
    island.value = { id: ++seq, text, kind, sub };
    islandTimer = window.setTimeout(() => (island.value = null), ms);
  }
  function hideIsland() {
    window.clearTimeout(islandTimer);
    island.value = null;
  }

  function confirm(title: string, body: string, okText = "确定", danger = false): Promise<boolean> {
    return new Promise((resolve) => {
      confirmReq.value = { title, body, okText, danger, resolve };
    });
  }
  function settleConfirm(v: boolean) {
    confirmReq.value?.resolve(v);
    confirmReq.value = null;
  }

  function pushEsc(fn: () => void) {
    escStack.push(fn);
    return () => {
      const i = escStack.lastIndexOf(fn);
      if (i >= 0) escStack.splice(i, 1);
    };
  }
  function popEsc(): boolean {
    if (confirmReq.value) {
      settleConfirm(false);
      return true;
    }
    if (paletteOpen.value) {
      paletteOpen.value = false;
      return true;
    }
    const fn = escStack.pop();
    if (fn) {
      fn();
      return true;
    }
    if (copilotOpen.value) {
      copilotOpen.value = false;
      return true;
    }
    return false;
  }

  return { island, paletteOpen, copilotOpen, confirmReq, sheets, depth, toast, hideIsland, confirm, settleConfirm, pushEsc, popEsc };
});
