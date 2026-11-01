import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api, toAppError } from "@/api";
import { on } from "@/api/events";
import type { CopilotMessage, CopilotSkill, ResultItem } from "@/api/types";
import { useAppStore } from "./app";
import { useUiStore } from "./ui";

const SESSION = "main";

export interface CopilotContextChip {
  label: string;
  data: unknown;
}

export const useCopilotStore = defineStore("copilot", () => {
  const messages = ref<CopilotMessage[]>([]);
  const skills = ref<CopilotSkill[]>([]);
  const streaming = ref<{ messageId: string; text: string } | null>(null);
  const sending = ref(false);
  const skill = ref<string | null>(null);
  const context = ref<CopilotContextChip | null>(null);
  const error = ref<string | null>(null);
  let listening = false;
  let loaded = false;

  const sortedSkills = computed(() => {
    const role = useAppStore().role;
    return [...skills.value].sort((a, b) => Number(b.roles.includes(role)) - Number(a.roles.includes(role)));
  });

  async function init() {
    if (!listening) {
      listening = true;
      await on("copilot://delta", (d) => {
        if (d.sessionId !== SESSION) return;
        if (!streaming.value || streaming.value.messageId !== d.messageId) streaming.value = { messageId: d.messageId, text: "" };
        streaming.value.text += d.delta;
      });
      await on("copilot://done", (d) => {
        if (d.sessionId !== SESSION) return;
        messages.value.push(d.message);
        streaming.value = null;
        sending.value = false;
      });
    }
    if (!loaded) {
      loaded = true;
      try {
        const [s, h] = await Promise.all([api.copilotSkills(), api.copilotHistory(SESSION)]);
        skills.value = s;
        messages.value = h;
      } catch (e) {
        error.value = toAppError(e).message;
      }
    }
  }

  async function send(text: string) {
    const t = text.trim();
    if (!t || sending.value) return;
    error.value = null;
    sending.value = true;
    messages.value.push({ id: "local-" + Date.now(), sessionId: SESSION, role: "user", content: t, actions: [], createdAt: new Date().toISOString() });
    try {
      const { messageId } = await api.copilotSend({ sessionId: SESSION, text: t, skill: skill.value ?? undefined, context: context.value?.data });
      streaming.value = { messageId, text: "" };
    } catch (e) {
      sending.value = false;
      error.value = toAppError(e).message;
    }
  }

  async function reset() {
    await api.copilotReset(SESSION);
    messages.value = [];
    streaming.value = null;
    sending.value = false;
    context.value = null;
  }

  function openWith(item?: ResultItem, prompt?: string) {
    const ui = useUiStore();
    if (item) {
      context.value = {
        label: item.title.length > 20 ? item.title.slice(0, 20) + "…" : item.title,
        data: { kind: "listing", item: { id: item.id, title: item.title, platformId: item.platformId, price: item.price, currency: item.currency, match: item.match, shop: item.shop, url: item.url } },
      };
      skill.value = "compare";
    }
    ui.copilotOpen = true;
    if (prompt) void send(prompt);
  }

  function setContext(c: CopilotContextChip | null) {
    context.value = c;
  }

  return { messages, skills, sortedSkills, streaming, sending, skill, context, error, init, send, reset, openWith, setContext };
});
