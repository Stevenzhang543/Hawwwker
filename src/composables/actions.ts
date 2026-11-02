import { nextTick } from "vue";
import { api, openExternal, toAppError } from "@/api";
import type { CopilotAction, ResultItem, ThemeMode, Role, Region } from "@/api/types";
import { useAppStore } from "@/stores/app";
import { usePlatformStore } from "@/stores/platforms";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { useWatchStore } from "@/stores/watch";
import { copyText } from "@/utils/format";
import { router } from "@/router";

/** 动作执行结果：用于 Copilot 消息里的"已执行"状态行 */
export interface ActionOutcome {
  ok: boolean;
  /** 给用户看的一句话 */
  note: string;
  /** search 动作启动的运行 id（Copilot 用来跟踪进度并在完成后点评） */
  runId?: string;
}

/**
 * 对当前搜索结果的界面操作（只看精确匹配、按到手价排序）由搜索页自己执行：
 * 通过 window 事件 `hawwwker:results-op`（detail: { op }）通知；搜索页处理后调用 `event.preventDefault()` 表示已应用。
 */
export const RESULTS_OP_EVENT = "hawwwker:results-op";

/**
 * 统一执行 AI 给出的动作（总结卡、Copilot 共用）以及结果卡上的常用操作。
 * 不依赖组件上下文（直接使用 router 单例），因此 Copilot 仓库也能调用。
 */
export function useActions() {
  const app = useAppStore();
  const platforms = usePlatformStore();
  const search = useSearchStore();
  const ui = useUiStore();
  const watch = useWatchStore();

  async function goSearchPage() {
    if (!router.currentRoute.value.path.startsWith("/discover/search")) await router.push("/discover/search");
  }

  async function startSearch(instruction: string, ids?: string[] | null): Promise<ActionOutcome> {
    let runId: string | null | undefined;
    if (ids && ids.length) {
      // 指定平台：必须先登录（登录门槛由平台仓库处理）
      const ok = await platforms.ensureLoggedIn(ids);
      if (!ok || !ok.length) return { ok: false, note: "已取消：所选平台需要先登录" };
      await goSearchPage();
      runId = await search.planThenStart(instruction, ok, (id) => platforms.byId[id]?.locale?.split("-")[0] ?? "zh");
    } else {
      await goSearchPage();
      runId = await search.start(instruction);
    }
    if (!runId) return { ok: false, note: "搜索没有开始" };
    return { ok: true, note: ids?.length ? `已在 ${ids.map((i) => platforms.label(i)).join("、")} 开始搜索` : "已开始搜索", runId };
  }

  async function resultsOp(op: string): Promise<ActionOutcome> {
    const run = search.current;
    if (op === "cancel") {
      if (!run || run.status !== "running") return { ok: false, note: "当前没有正在进行的搜索" };
      await search.cancel(run.runId);
      return { ok: true, note: "已取消当前搜索" };
    }
    if (!run) return { ok: false, note: "还没有搜索结果" };
    if (op === "export_csv" || op === "copy_list") {
      if (run.status !== "done" && !run.replay) return { ok: false, note: "搜索完成后才能导出" };
      if (op === "export_csv") {
        const path = await api.runExport(run.runId, "csv");
        return { ok: true, note: `已导出 CSV：${path}` };
      }
      const text = await api.runExport(run.runId, "text");
      return (await copyText(text)) ? { ok: true, note: "比价清单已复制到剪贴板" } : { ok: false, note: "复制失败" };
    }
    await goSearchPage();
    await nextTick(); // 等搜索页挂载后再派发
    const ev = new CustomEvent(RESULTS_OP_EVENT, { detail: { op }, cancelable: true });
    const handled = !window.dispatchEvent(ev);
    const label = op === "filter_exact" ? "只看精确匹配" : op === "sort_landed" ? "按到手价排序" : op;
    return handled ? { ok: true, note: `已${label}` } : { ok: false, note: `请在结果工具栏中手动${label}` };
  }

  async function setting(key: string, value: string): Promise<ActionOutcome> {
    if (key === "theme") await app.update({ theme: value as ThemeMode });
    else if (key === "displayCurrency") await app.update({ displayCurrency: value });
    else if (key === "role") await app.update({ role: value as Role });
    else if (key === "depth") await app.update({ search: { depth: value as "quick" | "standard" | "deep" } });
    else if (key === "region") await app.update({ search: { defaultRegion: value as "all" | Region } });
    else return { ok: false, note: `不支持的设置：${key}` };
    return { ok: true, note: "设置已更新" };
  }

  /** 执行一个动作并返回结果（不弹提示，交给调用方展示） */
  async function execute(a: CopilotAction): Promise<ActionOutcome> {
    try {
      switch (a.type) {
        case "search":
          return await startSearch(a.instruction, a.platforms);
        case "watch": {
          if (a.listingId) await watch.add({ listingId: a.listingId, targetPrice: a.targetPrice ?? null });
          else if (a.url) await watch.add({ url: a.url, targetPrice: a.targetPrice ?? null });
          else return { ok: false, note: "缺少要追踪的商品" };
          return { ok: true, note: "已加入价格追踪" };
        }
        case "open":
          await openExternal(a.url);
          return { ok: true, note: "已在浏览器打开" };
        case "navigate":
          await router.push(a.route);
          return { ok: true, note: "已打开页面" };
        case "monitor": {
          await watch.createMonitor({ instruction: a.instruction, intervalMinutes: a.intervalMinutes ?? 360, targetPrice: a.targetPrice ?? null });
          return { ok: true, note: `已创建定时监控（每 ${Math.round((a.intervalMinutes ?? 360) / 60)} 小时）` };
        }
        case "login":
          await router.push("/platforms/accounts");
          await platforms.loginStart(a.platformId);
          return { ok: true, note: `已打开 ${platforms.label(a.platformId)} 登录窗口，请在窗口中完成登录` };
        case "setting":
          return await setting(a.key, a.value);
        case "results":
          return await resultsOp(a.op);
      }
    } catch (e) {
      return { ok: false, note: toAppError(e).message };
    }
    return { ok: false, note: "未知动作" };
  }

  /** 点击按钮执行：带提示 */
  async function runAction(a: CopilotAction) {
    const r = await execute(a);
    ui.toast(r.note, r.ok ? (a.type === "search" ? "ai" : "success") : "error", r.ok ? 2400 : 4000, a.type === "search" ? a.instruction : undefined);
    return r;
  }

  async function watchItem(it: ResultItem, targetPrice?: number | null) {
    try {
      await watch.add({ listingId: it.id, targetPrice: targetPrice ?? null });
      ui.toast("已加入价格追踪", "success", 2400, it.title.slice(0, 30));
    } catch (e) {
      ui.toast(toAppError(e).message, "error");
    }
  }

  async function copyLink(url: string) {
    ui.toast((await copyText(url)) ? "链接已复制" : "复制失败", "success", 1500);
  }

  return { runAction, execute, watchItem, copyLink };
}
