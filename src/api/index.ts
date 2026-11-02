/**
 * 前端访问后端的唯一入口。在 Tauri 中走 IPC；在普通浏览器（npm run dev 预览）中自动切换为 mock 数据。
 */
import type {
  AiConfig, AiConfigInput, AiPreset, AiStatus, AiUsage, AppError, AppSettings, AppSettingsPatch, Bootstrap, BrowserInfo,
  CopilotMessage, CopilotSendInput, CopilotSkill, CustomPlatformInput, FxRates, ListingDetail, LoginState, MarketOverview,
  Monitor, MonitorCreateInput, MonitorPatch, Notice, PlatformInfo, PlatformPatch, PricePoint, RunDetail, RunEvent,
  RunSummary, SearchPlan, TrendSeries, WatchAddInput, WatchItem, WatchPatch,
  LinkInfo, LocalAi, Recommendation, SelfTestResult,
  LoginCheck, ModelList,
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Args = Record<string, unknown> | undefined;

async function call<T>(cmd: string, args?: Args): Promise<T> {
  if (!isTauri) {
    const { mockInvoke } = await import("./mock");
    return (await mockInvoke(cmd, args)) as T;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "code" in e && "message" in e) return e as AppError;
  if (typeof e === "string") {
    try {
      const parsed = JSON.parse(e);
      if (parsed && parsed.code) return parsed as AppError;
    } catch {
      /* not json */
    }
    return { code: "INTERNAL", message: e };
  }
  if (e instanceof Error) return { code: "INTERNAL", message: e.message };
  return { code: "INTERNAL", message: String(e) };
}

export const api = {
  /* app & settings */
  bootstrap: () => call<Bootstrap>("app_bootstrap"),
  settingsUpdate: (patch: AppSettingsPatch) => call<AppSettings>("settings_update", { patch }),
  openDataDir: () => call<string>("open_data_dir"),
  dataExport: (kind: "watches" | "listings" | "runs") => call<string>("data_export", { kind }),
  fxRates: () => call<FxRates>("fx_rates"),

  /* ai */
  aiGetConfig: () => call<AiConfig>("ai_get_config"),
  aiSaveConfig: (input: AiConfigInput) => call<AiConfig>("ai_save_config", { input }),
  aiTest: () => call<AiStatus>("ai_test"),
  aiPresets: () => call<AiPreset[]>("ai_presets"),
  aiUsage: () => call<AiUsage>("ai_usage"),
  aiDetectLocal: () => call<LocalAi>("ai_detect_local"),
  /** v26.03：列出服务商当前可用的模型（不传 input 时用已保存的配置） */
  aiListModels: (input?: AiConfigInput) => call<ModelList>("ai_list_models", { input: input ?? null }),

  /* platforms & login */
  platformsList: () => call<PlatformInfo[]>("platforms_list"),
  platformUpdate: (id: string, patch: PlatformPatch) => call<PlatformInfo>("platform_update", { id, patch }),
  platformAddCustom: (input: CustomPlatformInput) => call<PlatformInfo>("platform_add_custom", { input }),
  platformRemoveCustom: (id: string) => call<void>("platform_remove_custom", { id }),
  platformsRecommend: (timezone: string, language: string) => call<Recommendation>("platforms_recommend", { timezone, language }),
  platformSelftest: (ids: string[], query?: string) => call<SelfTestResult[]>("platform_selftest", { ids, query: query ?? null }),
  loginStates: (refresh = false) => call<LoginState[]>("login_states", { refresh }),
  loginStart: (platformId: string) => call<LoginState>("login_start", { platformId }),
  loginConfirm: (platformId: string) => call<LoginState>("login_confirm", { platformId }),
  loginCancel: (platformId: string) => call<LoginState>("login_cancel", { platformId }),
  logout: (platformId: string) => call<LoginState>("logout", { platformId }),
  /** v26.03：搜索前登录预检；verify = true 会对较久未验证的平台做主动验证 */
  loginPreflight: (platformIds: string[], verify = true) => call<LoginCheck[]>("login_preflight", { platformIds, verify }),
  /** v26.03：登录体检（主动验证所有已启用、需要登录的平台） */
  loginCheckAll: () => call<LoginState[]>("login_check_all"),
  browserInfo: () => call<BrowserInfo>("browser_info"),
  browserShow: () => call<void>("browser_show"),
  browserWipe: () => call<void>("browser_wipe"),

  /* search */
  searchPlan: (instruction: string) => call<SearchPlan>("search_plan", { instruction }),
  searchStart: (instruction: string, plan?: SearchPlan) =>
    call<{ runId: string; plan: SearchPlan }>("search_start", { instruction, plan: plan ?? null }),
  searchCancel: (runId: string) => call<void>("search_cancel", { runId }),
  searchEvents: (runId: string, since: number) => call<RunEvent[]>("search_events", { runId, since }),
  searchRuns: (limit = 50) => call<RunSummary[]>("search_runs", { limit }),
  searchRun: (runId: string) => call<RunDetail>("search_run", { runId }),
  searchDeleteRun: (runId: string) => call<void>("search_delete_run", { runId }),
  listingGet: (id: string) => call<ListingDetail>("listing_get", { id }),
  linkInspect: (url: string) => call<LinkInfo>("link_inspect", { url }),
  /** kind: "irrelevant" | "clear" */
  feedbackSet: (listingId: string, runId: string, kind: "irrelevant" | "clear") => call<void>("feedback_set", { listingId, runId, kind }),
  /** format: "text" 返回可粘贴清单；"csv" 写文件并返回路径 */
  runExport: (runId: string, format: "text" | "csv") => call<string>("run_export", { runId, format }),

  /* watch */
  watchList: () => call<WatchItem[]>("watch_list"),
  watchAdd: (input: WatchAddInput) => call<WatchItem>("watch_add", { input }),
  watchUpdate: (id: number, patch: WatchPatch) => call<WatchItem>("watch_update", { id, patch }),
  watchRemove: (id: number) => call<void>("watch_remove", { id }),
  watchCheck: (id: number) => call<WatchItem>("watch_check", { id }),
  watchHistory: (id: number, days = 90) => call<PricePoint[]>("watch_history", { id, days }),

  /* monitors */
  monitorList: () => call<Monitor[]>("monitor_list"),
  monitorCreate: (input: MonitorCreateInput) => call<Monitor>("monitor_create", { input }),
  monitorUpdate: (id: number, patch: MonitorPatch) => call<Monitor>("monitor_update", { id, patch }),
  monitorDelete: (id: number) => call<void>("monitor_delete", { id }),
  monitorRun: (id: number) => call<{ runId: string }>("monitor_run", { id }),

  /* notices */
  noticeList: (limit = 100) => call<Notice[]>("notice_list", { limit }),
  noticeRead: (id: number) => call<void>("notice_read", { id }),
  noticeReadAll: () => call<void>("notice_read_all"),

  /* insights */
  insightsOverview: (days = 30) => call<MarketOverview>("insights_overview", { days }),
  insightsTrends: (watchIds: number[], monitorIds: number[], days = 30) =>
    call<TrendSeries[]>("insights_trends", { watchIds, monitorIds, days }),
  insightsReport: (scope: string, days = 30) => call<{ reportId: string }>("insights_report", { scope, days }),

  /* copilot */
  copilotSkills: () => call<CopilotSkill[]>("copilot_skills"),
  copilotHistory: (sessionId: string) => call<CopilotMessage[]>("copilot_history", { sessionId }),
  copilotSend: (input: CopilotSendInput) => call<{ messageId: string }>("copilot_send", { input }),
  copilotReset: (sessionId: string) => call<void>("copilot_reset", { sessionId }),
};

/** 在系统默认浏览器中打开外部链接（商品页 / 店铺页）。 */
export async function openExternal(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return;
  if (!isTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export type { AppError };
