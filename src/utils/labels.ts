import type { Goal, PlatformRunState, RunStatus, SortKey, Verdict } from "@/api/types";

export const VERDICT: Record<Verdict, { label: string; tone: string; desc: string }> = {
  exact: { label: "精确匹配", tone: "green", desc: "品牌、型号、规格都对上" },
  close: { label: "接近", tone: "blue", desc: "同款不同配置/颜色/套餐" },
  related: { label: "相关", tone: "orange", desc: "同品类但不是目标，或二手/翻新" },
  mismatch: { label: "不符", tone: "red", desc: "配件、不同产品或广告" },
};

export const FLAG: Record<string, string> = {
  accessory: "配件",
  used: "二手",
  refurbished: "翻新",
  bundle: "套装",
  different_model: "型号不同",
  ad: "广告",
  price_suspicious: "价格异常",
  unknown_price: "价格未知",
};

export const GOAL: Record<Goal, string> = {
  buy: "购买",
  compare: "比价",
  track: "盯价",
  research: "调研",
  gift: "送礼",
  source: "找货源",
};

export const SORT: Record<SortKey, string> = {
  relevance: "匹配度优先",
  price_asc: "价格从低到高",
  price_desc: "价格从高到低",
  sales: "销量优先",
  rating: "评分优先",
  recent: "最新优先",
};

export const CONDITION: Record<string, string> = { any: "不限成色", new: "全新", used: "二手", unknown: "成色未知" };

export const PSTATE: Record<PlatformRunState, { label: string; tone: string }> = {
  queued: { label: "排队", tone: "" },
  searching: { label: "搜索中", tone: "blue" },
  waiting_user: { label: "等你操作", tone: "orange" },
  done: { label: "完成", tone: "green" },
  error: { label: "失败", tone: "red" },
  skipped: { label: "跳过", tone: "" },
  login_required: { label: "需要登录", tone: "orange" },
};

export const RUN_STATUS: Record<RunStatus, { label: string; tone: string }> = {
  running: { label: "进行中", tone: "blue" },
  done: { label: "完成", tone: "green" },
  error: { label: "失败", tone: "red" },
  cancelled: { label: "已取消", tone: "" },
  interrupted: { label: "已中断", tone: "orange" },
};

export const DROP_LABEL: Record<string, string> = {
  exclude: "排除词",
  price: "价格",
  condition: "成色",
  duplicate: "重复",
  irrelevant: "无关",
  location: "地区",
  invalid: "无效",
};

export const EXAMPLES: Record<string, { title: string; text: string; icon: string }[]> = {
  consumer: [
    { title: "跨国比价", text: "Sony WH-1000XM5 在亚马逊德国和 eBay 上 200 欧以内，不要二手", icon: "scale" },
    { title: "国内全网最低", text: "Switch 2 马力欧套装 京东 拼多多 淘宝 哪里最便宜", icon: "tag" },
    { title: "送礼灵感", text: "给喜欢露营的爸爸挑 500 元以内的生日礼物", icon: "gift" },
    { title: "看真实口碑", text: "戴森 V15 在小红书和 RedNote 上的真实评价和到手价", icon: "chat" },
  ],
  merchant: [
    { title: "竞品定价", text: "40000mAh 户外电源 亚马逊德国 前 20 名价格和评分", icon: "chart" },
    { title: "货源对比", text: "露营灯 1688 和 Alibaba.com 批发价，起订量 100 以内", icon: "box" },
    { title: "低价冲击", text: "蓝牙耳机 Temu AliExpress 价格带，看 30 欧以下的爆款", icon: "drop" },
    { title: "二手残值", text: "iPhone 15 Pro 闲鱼和 Kleinanzeigen 二手价格对比", icon: "refresh" },
  ],
  analyst: [
    { title: "品类价格带", text: "降噪耳机 国内与欧洲各平台价格分布，按平台对比中位价", icon: "pie" },
    { title: "平台价差", text: "Switch 2 在京东、拼多多、Amazon.co.jp、Amazon.de 的价差", icon: "globe" },
    { title: "新品追踪", text: "扫地机器人 新品 最近一周 各平台上架情况，每天监控", icon: "clock" },
    { title: "促销观察", text: "Dyson V15 idealo 与 Amazon.de 最近 30 天最低价", icon: "trend" },
  ],
};
