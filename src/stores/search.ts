import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { api, toAppError } from "@/api";
import { on } from "@/api/events";
import type {
  AiSummary, PlatformRunState, PlatformSkip, ResultItem, RunEvent, RunStats, RunStatus, RunSummary, SearchPlan, Stage,
} from "@/api/types";
import { usePlatformStore } from "@/stores/platforms";
import { PIPELINE, normStage } from "@/components/search/insight";

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
  /** 渐进式结果：各平台搜完即推送的规则初判结果（最终 results 到达后清空） */
  partial: Record<string, ResultItem[]>;
  summary: AiSummary | null;
  stats: RunStats | null;
  /** v26.03 平台判定：参与搜索的平台、跳过的平台与理由 */
  platformPlan: { chosen: string[]; skipped: PlatformSkip[]; reason: string } | null;
  /** v26.03 漏斗：每个筛选阶段的输入 / 保留（按阶段去重，保持到达顺序） */
  funnel: FunnelStep[];
  /** 本次运行实际经过的阶段（未经过的在进度条上显示为"跳过"，例如快速模式不读评论） */
  seen: Stage[];
  logs: { level: string; message: string; t: number }[];
  error: string | null;
  lastSeq: number;
  startedAt: number;
  finishedAt: number | null;
  /** 从历史加载（不是实时运行） */
  replay: boolean;
}

export interface FunnelStep {
  stage: Stage;
  label: string;
  input: number;
  kept: number;
  note?: string | null;
}

/** 稳定键：同一商品在初判（p:…）与最终结果（listing uuid）之间保持不变，用于原位替换 */
export function itemKey(x: ResultItem): string {
  return `${x.platformId}:${x.externalId}`;
}

/** 当前运行要展示的结果：最终结果优先，否则合并各平台的初判结果（按规则分排序） */
export function visibleItems(r: RunState | null): ResultItem[] {
  if (!r) return [];
  if (r.results.length) return r.results;
  const all = Object.values(r.partial).flat();
  return all.sort((a, b) => b.match.score - a.match.score).map((x, i) => ({ ...x, rank: i + 1 }));
}

/** v26.03：理解需求 → 平台判定 → 搜索 → 粗筛 → 风险 → 店铺 → 评论检索 → 评论分析 → 细筛 → 总结 */
export const STAGES = PIPELINE;

function blank(runId: string, instruction: string, plan: SearchPlan | null): RunState {
  return {
    runId, instruction, plan, status: "running", stage: "plan", stageLabel: "理解指令", progress: 0.02, platforms: {},
    filter: null, verify: null, results: [], partial: {}, summary: null, stats: null, platformPlan: null, funnel: [], seen: ["plan"],
    logs: [], error: null, lastSeq: 0,
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
      case "stage": {
        r.stage = ev.stage;
        r.stageLabel = ev.label;
        r.progress = Math.max(r.progress, ev.progress);
        const k = normStage(ev.stage);
        if (!r.seen.includes(k)) r.seen.push(k);
        break;
      }
      case "platform_plan": {
        r.platformPlan = { chosen: ev.chosen, skipped: ev.skipped, reason: ev.reason };
        for (const id of ev.chosen) if (!r.platforms[id]) r.platforms[id] = { state: "queued", count: 0 };
        for (const s of ev.skipped) {
          r.platforms[s.platformId] = { state: s.reason === "login_required" ? "login_required" : "skipped", count: 0, message: s.message };
        }
        // 计划里列过、但最终没参与也没说明理由的平台：从进度里去掉
        const keep = new Set([...ev.chosen, ...ev.skipped.map((s) => s.platformId)]);
        for (const [id, p] of Object.entries(r.platforms)) if (!keep.has(id) && p.state === "queued") delete r.platforms[id];
        break;
      }
      case "funnel": {
        const step: FunnelStep = { stage: ev.stage, label: ev.label, input: ev.input, kept: ev.kept, note: ev.note ?? null };
        const i = r.funnel.findIndex((f) => f.stage === ev.stage);
        if (i >= 0) r.funnel.splice(i, 1, step);
        else r.funnel.push(step);
        break;
      }
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
      case "partial":
        // 用户在初判阶段标记过的反馈要保留
        r.partial[ev.platformId] = ev.items.map((x) => ({ ...x, feedback: x.feedback ?? feedbackOf(r, x) }));
        break;
      case "results":
        // v26.03：粗筛后与每个补充阶段（风险 / 店铺 / 评论 / 细筛）都会推送一次；按稳定键原位替换
        r.results = ev.items.map((x) => ({ ...x, feedback: x.feedback ?? feedbackOf(r, x), watched: x.watched || watchedOf(r, x) }));
        r.partial = {};
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

  /** 运行中追踪过的商品：后续 results 更新不要把"追踪中"状态冲掉 */
  function watchedOf(r: RunState, x: ResultItem): boolean {
    const k = itemKey(x);
    return r.results.some((o) => itemKey(o) === k && o.watched);
  }

  /** 某条结果在当前运行里是否已被用户标记（按 平台 + 外部 id，初判与最终结果通用） */
  function feedbackOf(r: RunState, x: ResultItem): string | null {
    const k = itemKey(x);
    const hit = r.results.find((o) => itemKey(o) === k) ?? Object.values(r.partial).flat().find((o) => itemKey(o) === k);
    return hit?.feedback ?? null;
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

  /** 只保留指定平台（同时裁剪各平台搜索词） */
  function restrict(plan: SearchPlan, ids: string[]): SearchPlan {
    const set = new Set(ids);
    return { ...plan, platforms: plan.platforms.filter((id) => set.has(id)), queries: plan.queries.filter((q) => set.has(q.platformId)) };
  }

  /**
   * 发起搜索。v26.03：指定了平台（选择器 / 计划里 platformsExplicit）时，先过登录门槛——需要登录的平台没登录就不能搜。
   * 返回 runId；用户在登录门槛里取消时返回 null（不发起搜索）。
   */
  async function start(instruction: string, plan?: SearchPlan): Promise<string | null> {
    const platformStore = usePlatformStore();
    let p = plan;
    if (p?.platformsExplicit && p.platforms.length) {
      const ok = await platformStore.ensureLoggedIn(p.platforms);
      if (!ok) return null;
      p = restrict(p, ok);
      if (!p.platforms.length) throw { code: "INVALID", message: "没有可以搜索的平台：请先登录，或换一个平台" };
    }
    try {
      return await launch(instruction, p);
    } catch (e) {
      const err = toAppError(e);
      if (err.code !== "LOGIN_REQUIRED" || !err.platforms?.length) throw err;
      // 指令里点名的平台需要先登录：弹出登录门槛，完成后重试一次（用户移除的平台不再搜索）
      // 后端只列出未登录的平台；指令里点名的其他平台仍可搜索，所以允许全部移除
      const ok = await platformStore.ensureLoggedIn(err.platforms, { othersRemain: true });
      if (!ok) return null;
      const dropped = err.platforms.filter((id) => !ok.includes(id));
      if (dropped.length) {
        const base = p ?? { ...(await api.searchPlan(instruction)), platformsExplicit: true };
        p = restrict(base, base.platforms.filter((id) => !dropped.includes(id)));
        if (!p.platforms.length) throw { code: "INVALID", message: "移除未登录的平台后没有可以搜索的平台了" };
      }
      return await launch(instruction, p);
    }
  }

  async function launch(instruction: string, plan?: SearchPlan): Promise<string> {
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

  /**
   * 先让 AI 生成计划，再按用户选择的平台调整后启动。
   * explicit = true：平台是用户在选择器里逐个选的（需要先登录）；false：只是按地区筛选。
   */
  async function planThenStart(instruction: string, platforms: string[], langOf: (id: string) => string, explicit = true) {
    pendingPlan.value = true;
    try {
      const plan = await api.searchPlan(instruction);
      const set = new Set(platforms);
      plan.platforms = platforms;
      plan.platformsExplicit = explicit;
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
    r.seen = d.summary.status === "done" ? PIPELINE.map((s) => s.key) : ["plan"];
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

  /** "不相关"反馈：kind = irrelevant | clear。初判（p: 开头）与最终结果都支持。 */
  async function setFeedback(runId: string, item: ResultItem, kind: "irrelevant" | "clear") {
    await api.feedbackSet(item.id, runId, kind);
    const r = runs[runId];
    if (!r) return;
    const k = itemKey(item);
    const val = kind === "clear" ? null : kind;
    for (const x of r.results) if (itemKey(x) === k) x.feedback = val;
    for (const list of Object.values(r.partial)) for (const x of list) if (itemKey(x) === k) x.feedback = val;
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
    loadHistory, remove, markWatched, clearCurrent, onFinish, catchUp, setFeedback,
  };
});
