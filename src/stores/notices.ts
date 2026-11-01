import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import { on } from "@/api/events";
import type { Notice } from "@/api/types";
import { useUiStore } from "./ui";

export const NOTICE_META: Record<Notice["kind"], { label: string; icon: string; tone: string }> = {
  price_drop: { label: "降价", icon: "drop", tone: "green" },
  target_hit: { label: "达到目标价", icon: "target", tone: "amber" },
  new_items: { label: "新品", icon: "spark", tone: "blue" },
  login: { label: "登录", icon: "key", tone: "orange" },
  error: { label: "错误", icon: "warn", tone: "red" },
  system: { label: "系统", icon: "info", tone: "" },
};

export const useNoticeStore = defineStore("notices", () => {
  const list = ref<Notice[]>([]);
  const unreadBoot = ref(0);
  const loaded = ref(false);
  let listening = false;

  const unread = computed(() => (loaded.value ? list.value.filter((n) => !n.read).length : unreadBoot.value));

  async function load() {
    list.value = await api.noticeList(200);
    loaded.value = true;
  }
  async function listen() {
    if (listening) return;
    listening = true;
    await on("notice://new", (n) => {
      if (!loaded.value) unreadBoot.value += 1;
      list.value.unshift(n);
      const kind = n.level === "error" ? "error" : n.level === "warning" ? "warn" : n.level === "success" ? "success" : "info";
      useUiStore().toast(n.title, kind, 4200, n.body);
    });
  }
  async function read(id: number) {
    const n = list.value.find((x) => x.id === id);
    if (n && !n.read) {
      n.read = true;
      await api.noticeRead(id);
    }
  }
  async function readAll() {
    await api.noticeReadAll();
    list.value.forEach((n) => (n.read = true));
    unreadBoot.value = 0;
  }

  return { list, unread, unreadBoot, loaded, load, listen, read, readAll };
});
