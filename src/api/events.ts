/**
 * 后端事件订阅。Tauri 中使用 @tauri-apps/api/event；浏览器预览中由 mock 模拟。
 */
import { isTauri } from "./index";
import type {
  CopilotDelta, CopilotDone, InsightDelta, InsightDone, LoginState, Notice, RunEventPayload, WatchItem,
} from "./types";

export interface EventMap {
  "run://event": RunEventPayload;
  "login://state": LoginState;
  "notice://new": Notice;
  "watch://updated": WatchItem;
  "copilot://delta": CopilotDelta;
  "copilot://done": CopilotDone;
  "insight://delta": InsightDelta;
  "insight://done": InsightDone;
}

export type Unlisten = () => void;

export async function on<K extends keyof EventMap>(event: K, cb: (payload: EventMap[K]) => void): Promise<Unlisten> {
  if (!isTauri) {
    const { mockListen } = await import("./mock");
    return mockListen(event, cb as (p: unknown) => void);
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<EventMap[K]>(event, (e) => cb(e.payload));
}
