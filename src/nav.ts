/** 一级分组 / 二级菜单（侧栏、面包屑、命令面板共用） */
export interface NavItem {
  path: string;
  label: string;
  icon: string;
  desc: string;
}
export interface NavGroup {
  key: string;
  label: string;
  en: string;
  icon: string;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    key: "discover", label: "发现", en: "Discover", icon: "search",
    items: [
      { path: "/discover/search", label: "智能搜索", icon: "spark", desc: "一句话，多平台搜索与比价" },
      { path: "/discover/history", label: "搜索记录", icon: "history", desc: "回看历史运行与结果" },
    ],
  },
  {
    key: "track", label: "追踪", en: "Track", icon: "eye",
    items: [
      { path: "/track/watchlist", label: "价格追踪", icon: "tag", desc: "单品盯价与目标价提醒" },
      { path: "/track/monitors", label: "定时监控", icon: "clock", desc: "保存的搜索，定时重跑" },
      { path: "/track/alerts", label: "提醒中心", icon: "bell", desc: "降价、新品、登录与错误" },
    ],
  },
  {
    key: "insights", label: "洞察", en: "Insights", icon: "chart",
    items: [
      { path: "/insights/trends", label: "价格走势", icon: "trend", desc: "多商品价格曲线对比" },
      { path: "/insights/market", label: "市场分析", icon: "pie", desc: "平台价差、价格带与 AI 报告" },
    ],
  },
  {
    key: "platforms", label: "平台", en: "Platforms", icon: "layers",
    items: [
      { path: "/platforms/accounts", label: "账号与登录", icon: "key", desc: "国内版 / 国际版登录状态" },
      { path: "/platforms/catalog", label: "平台目录", icon: "globe", desc: "启用平台、搜索模板、自定义" },
    ],
  },
  {
    key: "settings", label: "设置", en: "Settings", icon: "gear",
    items: [
      { path: "/settings/ai", label: "AI 模型", icon: "cpu", desc: "连接与测试大模型（必需）" },
      { path: "/settings/general", label: "通用", icon: "sliders", desc: "角色、货币、外观、搜索偏好" },
      { path: "/settings/notify", label: "通知", icon: "bell", desc: "系统通知与群机器人" },
      { path: "/settings/data", label: "数据", icon: "database", desc: "数据目录、导出与汇率" },
    ],
  },
];

export function findNav(path: string): { group: NavGroup; item: NavItem } | null {
  for (const group of NAV) {
    const item = group.items.find((i) => path.startsWith(i.path));
    if (item) return { group, item };
  }
  return null;
}
