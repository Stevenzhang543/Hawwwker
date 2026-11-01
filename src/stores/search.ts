import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { api, toAppError } from "@/api";
import { on } from "@/api/events";
import type {
  AiSummary, PlatformRunState, ResultItem, RunEvent, RunStats, RunStatus, RunSummary, SearchPlan, Stage,
} from "@/api/types";

export interface PlatformProgress {
  state: PlatformRunState;
  count: number;
  message?: string | null;
  ms?: number | null;
}

export interface RunState {
  runId: string;
  instruction: string;
  plan: SearchPlan | null;
  status: RunStatus;
  stage: Stage;
  stageLabel: string;
  progress: number;
  platforms: Record<string, PlatformProgress>;
  filter: { fetched: number; kept: number; dropped: Record<string, number>; duplicates: number } | null;
  verify: { checked: number; matched: number; hidden: number } | null;
  results: ResultItem[];
  summary: AiSummary | null;
  stats: RunStats | null;
  logs: { level: string; message: string; t: number }[];
  error: string | null;
  lastSeq: number;
  startedAt: number;
  finishedAt: number | null;
  /** 从历史加载（不是实时运行） */
  replay: boolean;
}

export const STAGES: { key: Stage; label: string }[] = [
  { key: "plan", label: "理解" },
  { key: "search", label: "搜索" },
  { key: "extract", label: "抽取" },
  { key: "verify", label: "AI 校验" },
  { key: "rank", label: "排序" },
  { key: "summarize", label: "总结" },
];

function blank(runId: string, instruction: string, plan: SearchPlan | null): RunState {
  return {
    runId, instruction, plan, status: "running", stage: "plan", stageLabel: "理解指令", progress: 0.02, platforms: {},
    filter: null, verify: null, results: [], summary: null, stats: null, logs: [], error: null, lastSeq: 0,
    startedAt: Date.now(), finishedAt: null, replay: false,
  };
}

export const useSearchStore = defineStore("search", () => {
  const runs = reactive<Record<string, RunState>>({});
  const currentId = ref<string | null>(null);
  const history = ref<RunSummary[]>([]);
  const pendingPlan = ref(false);
  let listening = false;
  const catching = new Set<string>();
  /** 完成回调（用于在其他页面提示） */
  const finishHandlers = new Set<(r: RunState) => void>();

  const current = computed(() => (currentId.value ? runs[currentId.value] ?? null : null));
  const active = computed(() => Object.values(runs).filter((r) => r.status === "running"));

  function apply(runId: string, ev: RunEvent) {
    const r = runs[runId];
    if (!r) return;
    if (ev.seq <= r.lastSeq) return; // 去重
    if (ev.seq > r.lastSeq + 1 && !r.replay) {
      void catchUp(runId); // 丢了事件：补拉
      return;
    }
    r.lastSeq = ev.seq;
    switch (ev.type) {
      case "stage":
        r.stage = ev.stage;
        r.stageLabel = ev.label;
        r.progress = Math.max(r.progress, ev.progress);
        break;
      case "plan":
        r.plan = ev.plan;
        for (const id of ev.plan.platforms) if (!r.platforms[id]) r.platforms[id] = { state: "queued", count: 0 };
        break;
      case "platform": {
        const prev = r.platforms[ev.platformId];
        r.platforms[ev.platformId] = { state: ev.state, count: ev.count ?? prev?.count ?? 0, message: ev.message ?? null, ms: ev.ms ?? prev?.ms ?? null };
        break;
      }
      case "filter":
        r.filter = { fetched: ev.fetched, kept: ev.kept, dropped: ev.dropped, duplicates: ev.duplicates };
        break;
      case "verify":
        r.verify = { checked: ev.checked, matched: ev.matched, hidden: ev.hidden };
        break;
      case "results":
        r.results = ev.items;
        break;
      case "summary":
        r.summary = ev.summary;
        break;
      case "log":
        r.logs.push({ level: ev.level, message: ev.message, t: ev.t });
        break;
      case "done":
        r.stats = ev.stats;
        finish(r, "done");
        break;
      case "error":
        r.error = ev.message;
        finish(r, "error");
        break;
      case "cancelled":
        finish(r, "cancelled");
        break;
    }
  }

  function finish(r: RunState, status: RunStatus) {
    r.status = status;
    r.progress = status === "done" ? 1 : r.progress;
    if (status === "done") r.stage = "done";
    r.finishedAt = Date.now();
    finishHandlers.forEach((fn) => fn(r));
    void loadHistory();
  }

  async function catchUp(runId: string) {
    if (catching.has(runId)) return;
    catching.add(runId);
    try {
      const r = runs[runId];
      if (!r) return;
      const evs = await api.searchEvents(runId, r.lastSeq);
      for (const ev of evs.sort((a, b) => a.seq - b.seq)) {
        if (ev.seq === r.lastSeq + 1) apply(runId, ev);
      }
    } catch (e) {
      console.warn("catchUp", e);
    } finally {
      catching.delete(runId);
    }
  }

  async function listen() {
    if (listening) return;
    listening = true;
    await on("run://event", ({ runId, event }) => {
      if (!runs[runId]) {
        // 由监控触发、本页面不知道的运行：建一个占位并补拉
        runs[runId] = blank(runId, "", null);
        void catchUp(runId);
        return;
      }
      apply(runId, event);
    });
  }

  /** 应用启动时接上仍在运行的检索 */
  async function resume(runIds: string[]) {
    for (const id of runIds) {
      if (!runs[id]) {
        runs[id] = blank(id, "", null);
        try {
          const d = await api.searchRun(id);
          runs[id].instruction = d.summary.instruction;
          runs[id].plan = d.plan;
          runs[id].startedAt = new Date(d.summary.startedAt).getTime();
        } catch {
          /* ignore */
        }
      }
      await catchUp(id);
      currentId.value = currentId.value ?? id;
    }
  }

  async function start(instruction: string, plan?: SearchPlan) {
    const { runId, plan: p } = await api.searchStart(instruction, plan);
    const existing = runs[runId];
    if (existing) {
      // 事件可能先于 invoke 返回到达：保留已收到的进度
      existing.instruction = instruction || p.instruction;
      existing.plan = existing.plan ?? p;
    } else {
      runs[runId] = blank(runId, instruction || p.instruction, p);
    }
    const rs = runs[runId]!;
    for (const id of p.platforms) if (!rs.platforms[id]) rs.platforms[id] = { state: "queued", count: 0 };
    currentId.value = runId;
    void catchUp(runId);
    return runId;
  }

  /** 先让 AI 生成计划，再按用户选择的平台调整后启动 */
  async function planThenStart(instruction: string, platforms: string[], langOf: (id: string) => string) {
    pendingPlan.value = true;
    try {
      const plan = await api.searchPlan(instruction);
      const set = new Set(platforms);
      plan.platforms = platforms;
      const kw = plan.keywords[0] || instruction;
      const qs = plan.queries.filter((q) => set.has(q.platformId));
      for (const id of platforms) if (!qs.some((q) => q.platformId === id)) qs.push({ platformId: id, q: kw, lang: langOf(id) });
      plan.queries = qs;
      return await start(instruction, plan);
    } finally {
      pendingPlan.value = false;
    }
  }

  async function cancel(runId: string) {
    try {
      await api.searchCancel(runId);
    } catch (e) {
      throw toAppError(e);
    }
  }

  async function loadRun(runId: string) {
    const d = await api.searchRun(runId);
    const r = blank(runId, d.summary.instruction, d.plan);
    r.status = d.summary.status;
    r.results = d.results;
    r.summary = d.aiSummary ?? null;
    r.stats = d.stats ?? null;
    r.error = d.summary.error ?? null;
    r.progress = d.summary.status === "done" ? 1 : r.progress;
    r.stage = d.summary.status === "done" ? "done" : "plan";
    r.startedAt = new Date(d.summary.startedAt).getTime();
    r.finishedAt = d.summary.finishedAt ? new Date(d.summary.finishedAt).getTime() : null;
    r.replay = d.summary.status !== "running";
    for (const p of d.stats?.platforms ?? []) r.platforms[p.platformId] = { state: p.state, count: p.count, message: p.message, ms: p.ms };
    if (!d.stats) for (const id of d.plan.platforms) r.platforms[id] = { state: "skipped", count: 0 };
    runs[runId] = r;
    currentId.value = runId;
    if (d.summary.status === "running") {
      r.replay = false;
      await catchUp(runId);
    }
    return r;
  }

  async function loadHistory(limit = 60) {
    try {
      history.value = await api.searchRuns(limit);
    } catch (e) {
      console.warn(e);
    }
  }

  async function remove(runId: string) {
    await api.searchDeleteRun(runId);
    history.value = history.value.filter((h) => h.id !== runId);
    delete runs[runId];
    if (currentId.value === runId) currentId.value = null;
  }

  function markWatched(listingId: string, watched = true) {
    for (const r of Object.values(runs)) {
      const it = r.results.find((x) => x.id === listingId);
      if (it) it.watched = watched;
    }
  }

  function clearCurrent() {
    currentId.value = null;
  }

  function onFinish(fn: (r: RunState) => void) {
    finishHandlers.add(fn);
    return () => finishHandlers.delete(fn);
  }

  return {
    runs, currentId, current, active, history, pendingPlan, listen, resume, start, planThenStart, cancel, loadRun,
    loadHistory, remove, markWatched, clearCurrent, onFinish, catchUp,
  };
});
