/** 侧栏一级入口与页面标签共用的徽标：检索中 / 未读提醒 / 达到目标价。 */
import { useSearchStore } from "@/stores/search";
import { useNoticeStore } from "@/stores/notices";
import { useWatchStore } from "@/stores/watch";
import type { NavGroup } from "@/nav";

export function useNavBadges() {
  const search = useSearchStore();
  const notices = useNoticeStore();
  const watch = useWatchStore();
  return (path: string): string | null => {
    if (path === "/discover/search" && search.active.length) return "live";
    if (path === "/track/alerts" && notices.unread) return String(notices.unread > 99 ? "99+" : notices.unread);
    if (path === "/track/watchlist" && watch.hitTarget.length) return String(watch.hitTarget.length);
    return null;
  };
}

/** 一级入口的汇总徽标："live" 优先，其次数字相加。 */
export function groupBadge(g: NavGroup, badge: (p: string) => string | null): string | null {
  let live = false;
  let n = 0;
  let any = false;
  for (const it of g.items) {
    const b = badge(it.path);
    if (!b) continue;
    any = true;
    if (b === "live") live = true;
    else n += b.endsWith("+") ? 99 : Number(b) || 0;
  }
  if (live) return "live";
  if (!any) return null;
  return n > 99 ? "99+" : String(n);
}
