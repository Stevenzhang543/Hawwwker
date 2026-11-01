import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import { on } from "@/api/events";
import type { Monitor, MonitorCreateInput, MonitorPatch, WatchAddInput, WatchItem, WatchPatch } from "@/api/types";
import { useSearchStore } from "./search";

export const useWatchStore = defineStore("watch", () => {
  const items = ref<WatchItem[]>([]);
  const monitors = ref<Monitor[]>([]);
  const loaded = ref(false);
  let listening = false;

  const drops = computed(() => items.value.filter((w) => (w.changePct ?? 0) < 0).length);
  const hitTarget = computed(() => items.value.filter((w) => w.targetPrice != null && w.lastPrice != null && w.lastPrice <= w.targetPrice));

  async function listen() {
    if (listening) return;
    listening = true;
    await on("watch://updated", (w) => upsert(w));
  }

  function upsert(w: WatchItem) {
    const i = items.value.findIndex((x) => x.id === w.id);
    if (i >= 0) items.value.splice(i, 1, w);
    else items.value.unshift(w);
  }

  async function load() {
    items.value = await api.watchList();
    loaded.value = true;
  }
  async function add(input: WatchAddInput) {
    const w = await api.watchAdd(input);
    upsert(w);
    if (input.listingId) useSearchStore().markWatched(input.listingId, true);
    return w;
  }
  async function update(id: number, patch: WatchPatch) {
    const w = await api.watchUpdate(id, patch);
    upsert(w);
    return w;
  }
  async function remove(id: number) {
    const w = items.value.find((x) => x.id === id);
    await api.watchRemove(id);
    items.value = items.value.filter((x) => x.id !== id);
    if (w?.listingId) useSearchStore().markWatched(w.listingId, false);
  }
  async function check(id: number) {
    upsert(await api.watchCheck(id));
  }

  async function loadMonitors() {
    monitors.value = await api.monitorList();
  }
  async function createMonitor(input: MonitorCreateInput) {
    const m = await api.monitorCreate(input);
    monitors.value.unshift(m);
    return m;
  }
  async function updateMonitor(id: number, patch: MonitorPatch) {
    const m = await api.monitorUpdate(id, patch);
    const i = monitors.value.findIndex((x) => x.id === id);
    if (i >= 0) monitors.value.splice(i, 1, m);
    return m;
  }
  async function deleteMonitor(id: number) {
    await api.monitorDelete(id);
    monitors.value = monitors.value.filter((m) => m.id !== id);
  }

  return { items, monitors, loaded, drops, hitTarget, listen, load, add, update, remove, check, loadMonitors, createMonitor, updateMonitor, deleteMonitor, upsert };
});
