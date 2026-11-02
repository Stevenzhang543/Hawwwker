import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { api, toAppError } from "@/api";
import { on } from "@/api/events";
import type { CopilotAction, CopilotMessage, CopilotSkill, ResultItem } from "@/api/types";
import { useActions } from "@/composables/actions";
import { findNav } from "@/nav";
import { router } from "@/router";
import { useAppStore } from "./app";
import { usePlatformStore } from "./platforms";
import { useSearchStore, type RunState } from "./search";
import { useUiStore } from "./ui";

const SESSION = "main";

export interface CopilotContextChip {
  label: string;
  data: unknown;
}

/** Copilot 消息里某个动作的执行状态 */
export interface ActionExec {
  status: "running" | "done" | "failed";
  note: string;
  runId?: string;
}

/** Copilot 发起（或从 Copilot 消息点击发起）的搜索：完成后自动请 Copilot 点评 */
interface TrackedRun {
  runId: string;
  messageId: string;
  instruction: string;
  followedUp: boolean;
}

const MAX_AUTO = 3;

/**
 * 前端兜底：即使后端放行，也只自动执行安全类型——搜索、跳转页面、修改设置、结果操作（取消除外）；
 * 登录只在用户这句话明确要求登录时。打开链接、追踪、创建监控、取消搜索一律需要用户点击。
 */
export function autoAllowed(a: CopilotAction, userText: string): boolean {
  switch (a.type) {
    case "search":
    case "navigate":
    case "setting":
      return true;
    case "results":
      return a.op !== "cancel";
    case "login":
      return /登录|登陆|log\s*in|sign\s*in/i.test(userText);
    default:
      return false;
  }
}

/** 把不允许自动执行的动作降级为按钮（返回新的消息对象） */
function sanitize(m: CopilotMessage, userText: string | null): CopilotMessage {
  let n = 0;
  const actions = m.actions.map((a) => {
    const ok = userText !== null && !!a.auto && autoAllowed(a, userText) && n < MAX_AUTO;
    if (ok) n++;
    return a.auto === ok ? a : ({ ...a, auto: ok } as CopilotAction);
  });
  return { ...m, actions };
}

/** 结果的精简表示：发给 Copilot 点评用 */
export function compactResult(r: ResultItem, platformLabel: (id: string) => string) {
  return {
    id: r.id,
    title: r.title.slice(0, 80),
    platform: platformLabel(r.platformId),
    shop: r.shopInfo?.label ?? r.shop ?? null,
    tags: (r.tags ?? []).slice(0, 4).map((t) => t.label),
    landed: r.displayLanded ?? r.displayPrice ?? null,
    price: r.price ?? null,
    currency: r.currency,
    verdict: r.match.verdict,
    match: r.match.score,
    risk: r.risk ? { level: r.risk.level, score: r.risk.score, why: r.risk.summary || r.risk.factors.slice(0, 2).map((f) => f.label).join("；") } : null,
    reviews: r.reviewInsight ? { score: r.reviewInsight.score ?? null, rating: r.reviewInsight.rating ?? null, count: r.reviewInsight.count ?? null, cons: r.reviewInsight.cons.slice(0, 2), redFlags: r.reviewInsight.redFlags.slice(0, 2) } : null,
    deal: r.deal?.label ?? null,
    finalScore: r.finalScore ?? null,
  };
}

export const useCopilotStore = defineStore("copilot", () => {
  const messages = ref<CopilotMessage[]>([]);
  const skills = ref<CopilotSkill[]>([]);
  const streaming = ref<{ messageId: string; text: string } | null>(null);
  const sending = ref(false);
  const skill = ref<string | null>(null);
  const context = ref<CopilotContextChip | null>(null);
  const error = ref<string | null>(null);
  /** messageId → 每个动作的执行状态（只记录本次会话中执行过的） */
  const exec = reactive<Record<string, Record<number, ActionExec>>>({});
  const tracked = ref<TrackedRun[]>([]);
  let listening = false;
  let loaded = false;
  let followQueue: Promise<void> = Promise.resolve();

  const sortedSkills = computed(() => {
    const role = useAppStore().role;
    return [...skills.value].sort((a, b) => Number(b.roles.includes(role)) - Number(a.roles.includes(role)));
  });
  /** 正在进行的 Copilot 搜索（抽屉里显示进度卡） */
  const activeRun = computed<RunState | null>(() => {
    const search = useSearchStore();
    for (const t of [...tracked.value].reverse()) {
      const r = search.runs[t.runId];
      if (r && (r.status === "running" || !t.followedUp)) return r;
    }
    return null;
  });

  /** 自动附带的 App 上下文：所在页面 + 当前搜索（精简） */
  function appContext() {
    const search = useSearchStore();
    const platforms = usePlatformStore();
    const path = router.currentRoute.value.path;
    const nav = findNav(path);
    const run = search.current;
    const out: Record<string, unknown> = { page: { path, label: nav ? `${nav.group.label} · ${nav.item.label}` : path } };
    if (run) {
      const visible = run.results.filter((r) => r.match.verdict !== "mismatch" && !r.feedback);
      out.currentSearch = {
        runId: run.runId,
        instruction: run.instruction,
        status: run.status,
        results: visible.length,
        top: path.startsWith("/discover/search") ? visible.slice(0, 8).map((r) => compactResult(r, (id) => platforms.label(id))) : undefined,
      };
    }
    return out;
  }

  function payload() {
    const base = appContext();
    return context.value ? { ...base, attached: context.value.data } : base;
  }

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
        const lastUser = [...messages.value].reverse().find((x) => x.role === "user" || x.role === "event");
        // App 事件（搜索完成点评）触发的回答不自动执行任何动作
        const msg = sanitize(d.message, lastUser?.role === "user" ? lastUser.content : null);
        messages.value.push(msg);
        streaming.value = null;
        sending.value = false;
        void runAuto(msg);
      });
      useSearchStore().onFinish((r) => void onRunFinished(r));
    }
    if (!loaded) {
      loaded = true;
      try {
        const [s, h] = await Promise.all([api.copilotSkills(), api.copilotHistory(SESSION)]);
        skills.value = s;
        // 历史消息不再自动执行：auto 动作显示为按钮
        messages.value = h.map((m) => sanitize(m, null));
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
      const { messageId } = await api.copilotSend({ sessionId: SESSION, text: t, skill: skill.value ?? undefined, context: payload() });
      streaming.value = { messageId, text: "" };
    } catch (e) {
      sending.value = false;
      error.value = toAppError(e).message;
    }
  }

  /* ------------------------------------------------------------ 执行动作 */
  async function execAction(messageId: string, index: number, a: CopilotAction) {
    if (!exec[messageId]) exec[messageId] = {};
    const slot = exec[messageId]!;
    if (slot[index]?.status === "running") return;
    slot[index] = { status: "running", note: a.type === "search" ? "正在发起搜索…" : "正在执行…" };
    const { execute } = useActions();
    const r = await execute(a);
    slot[index] = { status: r.ok ? "done" : "failed", note: r.note, runId: r.runId };
    if (r.ok && r.runId && a.type === "search") {
      tracked.value.push({ runId: r.runId, messageId, instruction: a.instruction, followedUp: false });
      if (tracked.value.length > 6) tracked.value.splice(0, tracked.value.length - 6);
    }
    return r;
  }

  /** 新回答里 auto = true 的动作：依次执行（最多 3 个） */
  async function runAuto(m: CopilotMessage) {
    const autos = m.actions.map((a, i) => [a, i] as const).filter(([a]) => a.auto).slice(0, MAX_AUTO);
    // 双保险：sanitize 之后仍逐个检查类型
    if (autos.some(([a]) => !autoAllowed(a, "登录"))) return;
    for (const [a, i] of autos) {
      const r = await execAction(m.id, i, a);
      if (r && !r.ok) break;
    }
  }

  /** 用户点击 Copilot 消息里的动作按钮 */
  async function clickAction(m: CopilotMessage, index: number) {
    const a = m.actions[index];
    if (!a) return;
    const r = await execAction(m.id, index, a);
    if (r) useUiStore().toast(r.note, r.ok ? "success" : "error", r.ok ? 2000 : 4000);
  }

  /* ------------------------------------------------------------ 搜索完成 → 自动点评 */
  function onRunFinished(r: RunState) {
    const t = tracked.value.find((x) => x.runId === r.runId);
    if (!t || t.followedUp) return;
    t.followedUp = true;
    followQueue = followQueue.then(() => followUp(t, r)).catch(() => undefined);
  }

  async function waitIdle() {
    for (let i = 0; i < 240 && sending.value; i++) await new Promise((res) => setTimeout(res, 500));
  }

  async function followUp(t: TrackedRun, r: RunState) {
    const platforms = usePlatformStore();
    const visible = r.results.filter((x) => x.match.verdict !== "mismatch" && !x.feedback);
    const exact = visible.filter((x) => x.match.verdict === "exact").length;
    const event = (content: string): CopilotMessage => ({ id: `event-${r.runId}`, sessionId: SESSION, role: "event", content, actions: [], createdAt: new Date().toISOString() });
    if (r.status !== "done") {
      messages.value.push(event(r.status === "cancelled" ? `搜索「${t.instruction}」已取消` : `搜索「${t.instruction}」没有完成：${r.error ?? "出错了"}`));
      return;
    }
    if (!visible.length) {
      messages.value.push(event(`搜索「${t.instruction}」完成，但没有找到符合要求的商品`));
      return;
    }
    await waitIdle();
    const text = `搜索「${t.instruction}」已完成：共 ${visible.length} 条相关结果，其中精确匹配 ${exact} 条。请根据结果点评并给出推荐。`;
    messages.value.push(event(text));
    sending.value = true;
    try {
      const { messageId } = await api.copilotSend({
        sessionId: SESSION,
        text,
        kind: "run_followup",
        context: { kind: "search_results", runId: r.runId, instruction: t.instruction, results: visible.slice(0, 12).map((x) => compactResult(x, (id) => platforms.label(id))) },
      });
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
    tracked.value = [];
    for (const k of Object.keys(exec)) delete exec[k];
  }

  function openWith(item?: ResultItem, prompt?: string) {
    const ui = useUiStore();
    if (item) {
      context.value = {
        label: item.title.length > 20 ? item.title.slice(0, 20) + "…" : item.title,
        data: {
          kind: "listing",
          item: { ...compactResult(item, (id) => usePlatformStore().label(id)), platformId: item.platformId, url: item.url, reasons: item.match.reasons },
        },
      };
      skill.value = "compare";
    }
    ui.copilotOpen = true;
    if (prompt) void send(prompt);
  }

  function setContext(c: CopilotContextChip | null) {
    context.value = c;
  }

  return {
    messages, skills, sortedSkills, streaming, sending, skill, context, error, exec, tracked, activeRun,
    init, send, reset, openWith, setContext, clickAction, appContext,
  };
});
