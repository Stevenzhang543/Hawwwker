import { useRouter } from "vue-router";
import { openExternal, toAppError } from "@/api";
import type { CopilotAction, ResultItem } from "@/api/types";
import { useSearchStore } from "@/stores/search";
import { useUiStore } from "@/stores/ui";
import { useWatchStore } from "@/stores/watch";
import { copyText } from "@/utils/format";

/** 统一执行 AI 给出的动作（总结卡、Copilot 共用）以及结果卡上的常用操作 */
export function useActions() {
  const router = useRouter();
  const search = useSearchStore();
  const ui = useUiStore();
  const watch = useWatchStore();

  async function runAction(a: CopilotAction) {
    try {
      if (a.type === "search") {
        await router.push("/discover/search");
        await search.start(a.instruction);
        ui.toast("已开始新的搜索", "ai", 2000, a.instruction);
      } else if (a.type === "watch") {
        await watch.add({ listingId: a.listingId, targetPrice: a.targetPrice ?? null });
        ui.toast("已加入价格追踪", "success");
      } else if (a.type === "open") {
        await openExternal(a.url);
      } else {
        await router.push(a.route);
      }
    } catch (e) {
      ui.toast(toAppError(e).message, "error", 4000);
    }
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

  return { runAction, watchItem, copyLink };
}
