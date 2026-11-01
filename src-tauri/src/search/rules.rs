//! 规则预解析：在调用 AI 之前，从指令中抽取确定性的数字与约束（价格+币种、条数、监控间隔、排除词、平台、地区）。
//! AI 规划结果与之合并时，规则抽到的数值优先（模型偶尔会改写数字）。AI 不可用时作为兜底计划。

use crate::platforms;
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleHints {
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    pub currency: Option<String>,
    pub limit: Option<u32>,
    pub monitor_minutes: Option<u32>,
    pub target_price: Option<f64>,
    pub exclude: Vec<String>,
    pub platforms: Vec<String>,
    pub region: Option<String>,
    pub condition: Option<String>,
    pub sort: Option<String>,
    pub goal: Option<String>,
    /// 去掉功能词后的剩余文本（兜底关键词）
    pub remainder: String,
}

static CUR_WORDS: &[(&str, &str)] = &[
    ("欧元", "EUR"), ("欧", "EUR"), ("€", "EUR"), ("eur", "EUR"), ("美元", "USD"), ("美金", "USD"), ("刀", "USD"),
    ("us$", "USD"), ("usd", "USD"), ("$", "USD"), ("英镑", "GBP"), ("镑", "GBP"), ("£", "GBP"), ("gbp", "GBP"),
    ("日元", "JPY"), ("円", "JPY"), ("jpy", "JPY"), ("港币", "HKD"), ("hkd", "HKD"), ("人民币", "CNY"), ("元", "CNY"),
    ("块钱", "CNY"), ("块", "CNY"), ("rmb", "CNY"), ("cny", "CNY"), ("¥", "CNY"), ("￥", "CNY"),
];

pub fn cn_number(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    if t == "半" {
        return Some(0.5);
    }
    let digit = |c: char| match c {
        '零' | '〇' => Some(0.0),
        '一' => Some(1.0),
        '二' | '两' => Some(2.0),
        '三' => Some(3.0),
        '四' => Some(4.0),
        '五' => Some(5.0),
        '六' => Some(6.0),
        '七' => Some(7.0),
        '八' => Some(8.0),
        '九' => Some(9.0),
        _ => None,
    };
    let (mut total, mut cur) = (0.0, 0.0);
    for ch in t.chars() {
        if let Some(d) = digit(ch) {
            cur = d;
        } else {
            let unit = match ch {
                '十' => 10.0,
                '百' => 100.0,
                '千' => 1000.0,
                '万' => 10000.0,
                _ => return None,
            };
            if unit == 10000.0 {
                total = (total + cur) * unit;
            } else {
                total += (if cur == 0.0 { 1.0 } else { cur }) * unit;
            }
            cur = 0.0;
        }
    }
    Some(total + cur)
}

fn unit_mul(u: &str) -> f64 {
    match u.to_lowercase().as_str() {
        "k" | "千" => 1000.0,
        "w" | "万" => 10000.0,
        _ => 1.0,
    }
}

fn detect_currency(s: &str) -> Option<String> {
    let low = s.to_lowercase();
    CUR_WORDS.iter().find(|(w, _)| low.contains(w)).map(|(_, c)| c.to_string())
}

const NUM: &str = r"(\d+(?:\.\d+)?)\s*(k|K|w|W|千|万)?";
const CUR: &str = r"\s*(欧元|欧|€|eur|EUR|美元|美金|刀|usd|USD|\$|英镑|镑|£|gbp|GBP|日元|円|jpy|JPY|港币|人民币|元|块钱|块|rmb|RMB|¥|￥)?";

static RE_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:预算|价格|价位|在)?\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:-|~|～|到|至|—)\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}")).unwrap()
});
static RE_MAX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:不超过|不高于|低于|小于|少于|最多|封顶|under|below|less than|max|<=|≤|<)\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}")).unwrap()
});
static RE_MAX2: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:预算|价格)?\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:以内|以下|之内|内|封顶)")).unwrap()
});
static RE_MIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:高于|大于|超过|至少|不低于|over|above|at least|>=|≥|>)\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}|(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:以上|起)")).unwrap()
});
static RE_ABOUT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:预算|价格|大概|大约|价位)\s*(?:在)?\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:左右|上下)?|(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:左右|上下)")).unwrap()
});
static RE_TARGET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:降到|跌到|低于|降至|到)\s*(?:€|\$|£|¥|￥)?{NUM}{CUR}\s*(?:时|的时候)?\s*(?:提醒|通知|告诉)")).unwrap()
});
static RE_MONITOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"每(?:隔)?\s*(\d+|[一二两三四五六七八九十半]+)?\s*(?:个)?\s*(分钟|分|小时|钟头|天|日)").unwrap()
});
static RE_WATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:持续|实时|定时|自动)?(?:监控|盯着|盯一下|盯住|盯价|跟踪|追踪)|降价(?:了)?(?:提醒|通知|告诉)我?|有新的?.{0,6}?(?:提醒|通知)我?").unwrap()
});
static RE_LIMIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:前|top|TOP|Top)\s*(\d+|[一二两三四五六七八九十百]+)\s*(?:条|个|款|名)?|(\d+|[一二两三四五六七八九十百]+)\s*(?:条|个|款)(?:结果|商品|推荐)").unwrap()
});
static RE_EXCLUDE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:排除|不要|不看|去掉|过滤掉?|剔除|屏蔽|别要|不含|不包括|\bwithout\b|\bno\b)\s*([^，。,；;！!？?\s]{1,24})").unwrap()
});

/// 价格数字只有在出现货币/预算语境时才算价格（避免把"3天""256G"当价格）。
fn price_ctx(m: &regex::Captures, whole: &str) -> bool {
    let s = m.get(0).map(|x| x.as_str()).unwrap_or("");
    let tail: String = whole[m.get(0).unwrap().end()..].chars().take(2).collect();
    let bad_tail = ["天", "日", "小时", "分钟", "周", "月", "年", "G", "g", "T", "寸", "mm", "W", "w", "人", "代", "岁", "个", "条", "次"];
    if bad_tail.iter().any(|b| tail.starts_with(b)) && detect_currency(s).is_none() {
        return false;
    }
    true
}

pub fn parse(instruction: &str) -> RuleHints {
    let text = instruction.trim();
    let low = text.to_lowercase();
    let mut h = RuleHints::default();
    let mut used: Vec<(usize, usize)> = vec![];
    let take = |m: &regex::Match, used: &mut Vec<(usize, usize)>| used.push((m.start(), m.end()));

    // 目标价提醒（先于价格区间，避免"降到1500提醒我"被当成预算）
    if let Some(c) = RE_TARGET.captures(text) {
        if let Some(v) = c.get(1).and_then(|m| m.as_str().parse::<f64>().ok()) {
            h.target_price = Some(v * unit_mul(c.get(2).map(|m| m.as_str()).unwrap_or("")));
            if let Some(cur) = c.get(3) {
                h.currency = detect_currency(cur.as_str());
            }
            take(&c.get(0).unwrap(), &mut used);
            h.monitor_minutes.get_or_insert(360);
        }
    }

    // 价格区间
    let overlaps = |m: &regex::Match, used: &Vec<(usize, usize)>| used.iter().any(|(s, e)| m.start() < *e && *s < m.end());
    if let Some(c) = RE_RANGE.captures(text).filter(|c| !overlaps(&c.get(0).unwrap(), &used) && price_ctx(c, text)) {
        let u1 = c.get(2).or(c.get(5)).map(|m| m.as_str()).unwrap_or("");
        let u2 = c.get(5).map(|m| m.as_str()).unwrap_or(u1);
        let a = c[1].parse::<f64>().unwrap_or(0.0) * unit_mul(u1);
        let b = c[4].parse::<f64>().unwrap_or(0.0) * unit_mul(u2);
        let has_ctx = c.get(3).is_some() || c.get(6).is_some() || c[0].contains("预算") || c[0].contains("价");
        if has_ctx && a > 0.0 && b > 0.0 {
            h.price_min = Some(a.min(b));
            h.price_max = Some(a.max(b));
            if let Some(cur) = c.get(3).or(c.get(6)) {
                h.currency = detect_currency(cur.as_str());
            }
            take(&c.get(0).unwrap(), &mut used);
        }
    }
    if h.price_max.is_none() {
        for re in [&*RE_MAX, &*RE_MAX2] {
            if let Some(c) = re.captures(text).filter(|c| !overlaps(&c.get(0).unwrap(), &used) && price_ctx(c, text)) {
                h.price_max = c[1].parse::<f64>().ok().map(|v| v * unit_mul(c.get(2).map(|m| m.as_str()).unwrap_or("")));
                if let Some(cur) = c.get(3) {
                    h.currency = detect_currency(cur.as_str());
                }
                take(&c.get(0).unwrap(), &mut used);
                break;
            }
        }
    }
    if h.price_min.is_none() {
        if let Some(c) = RE_MIN.captures(text).filter(|c| !overlaps(&c.get(0).unwrap(), &used) && price_ctx(c, text)) {
            let (n, u, cur) = if c.get(1).is_some() { (1, 2, 3) } else { (4, 5, 6) };
            let has_cur = c.get(cur).is_some();
            if has_cur || c[0].contains("至少") || c[0].contains("以上") || c[0].contains("起") {
                h.price_min = c[n].parse::<f64>().ok().map(|v| v * unit_mul(c.get(u).map(|m| m.as_str()).unwrap_or("")));
                if let Some(cm) = c.get(cur) {
                    h.currency = detect_currency(cm.as_str());
                }
                take(&c.get(0).unwrap(), &mut used);
            }
        }
    }
    if h.price_min.is_none() && h.price_max.is_none() {
        if let Some(c) = RE_ABOUT.captures(text).filter(|c| !overlaps(&c.get(0).unwrap(), &used) && price_ctx(c, text)) {
            let (n, u, cur) = if c.get(1).is_some() { (1, 2, 3) } else { (4, 5, 6) };
            if let Some(v) = c[n].parse::<f64>().ok().map(|v| v * unit_mul(c.get(u).map(|m| m.as_str()).unwrap_or(""))) {
                h.price_min = Some((v * 0.75 * 100.0).round() / 100.0);
                h.price_max = Some((v * 1.25 * 100.0).round() / 100.0);
                if let Some(cm) = c.get(cur) {
                    h.currency = detect_currency(cm.as_str());
                }
                take(&c.get(0).unwrap(), &mut used);
            }
        }
    }

    // 监控
    if let Some(c) = RE_MONITOR.captures(text) {
        let n = c.get(1).and_then(|m| cn_number(m.as_str())).unwrap_or(1.0);
        let mins = match &c[2] {
            "分钟" | "分" => n,
            "小时" | "钟头" => n * 60.0,
            _ => n * 1440.0,
        };
        h.monitor_minutes = Some((mins as u32).clamp(30, 10080));
        take(&c.get(0).unwrap(), &mut used);
    }
    if let Some(m) = RE_WATCH.find(text) {
        h.monitor_minutes.get_or_insert(360);
        used.push((m.start(), m.end()));
    }

    // 条数
    if let Some(c) = RE_LIMIT.captures(text) {
        if let Some(n) = c.get(1).or(c.get(2)).and_then(|m| cn_number(m.as_str())) {
            h.limit = Some((n as u32).clamp(1, 200));
            take(&c.get(0).unwrap(), &mut used);
        }
    }

    // 排除词与成色
    for c in RE_EXCLUDE.captures_iter(text) {
        let m = c.get(0).unwrap();
        used.push((m.start(), m.end()));
        for part in c[1].split(['、', '/', '和', '及', '与', '或', ',']) {
            let part = part.trim().trim_end_matches(['的', '类']).trim();
            if !part.is_empty() && part.chars().count() <= 12 && !h.exclude.iter().any(|x| x == part) {
                h.exclude.push(part.to_string());
            }
        }
    }
    let excl_used = h.exclude.iter().any(|x| x.contains("二手") || x.contains("翻新") || x == "used");
    if excl_used || low.contains("全新") || low.contains("brand new") {
        h.condition = Some("new".into());
    } else if low.contains("二手") || low.contains("闲置") || low.contains("used") || low.contains("gebraucht") {
        h.condition = Some("used".into());
    }

    // 平台
    let mut aliases = platforms::aliases();
    aliases.sort_by_key(|(a, _)| std::cmp::Reverse(a.chars().count()));
    let mut consumed = low.clone();
    for (alias, id) in aliases {
        if consumed.contains(alias) {
            consumed = consumed.replace(alias, " ");
            if !h.platforms.iter().any(|x| x == id) {
                h.platforms.push(id.to_string());
            }
        }
    }
    if consumed.contains("亚马逊") || consumed.contains("amazon") {
        let id = if low.contains("德国") || low.contains("欧") { "amazon_de" } else { "amazon_com" };
        if !h.platforms.iter().any(|x| x == id) {
            h.platforms.push(id.into());
        }
    }
    if consumed.contains("ebay") {
        let id = if low.contains("德国") { "ebay_de" } else { "ebay_com" };
        if !h.platforms.iter().any(|x| x == id) {
            h.platforms.push(id.into());
        }
    }
    if ["国际", "海外", "国外", "外网", "international"].iter().any(|w| low.contains(w)) {
        h.region = Some("intl".into());
    } else if ["国内", "中国大陆"].iter().any(|w| low.contains(w)) {
        h.region = Some("cn".into());
    }
    if ["全平台", "所有平台", "全网", "各平台"].iter().any(|w| low.contains(w)) {
        h.platforms.clear();
    }

    // 排序与目标
    h.sort = if ["最便宜", "价格最低", "从低到高", "低价优先", "cheapest"].iter().any(|w| low.contains(w)) {
        Some("price_asc".into())
    } else if ["最贵", "从高到低"].iter().any(|w| low.contains(w)) {
        Some("price_desc".into())
    } else if ["销量", "卖得最好", "爆款", "畅销"].iter().any(|w| low.contains(w)) {
        Some("sales".into())
    } else if ["好评", "评分最高", "口碑最好"].iter().any(|w| low.contains(w)) {
        Some("rating".into())
    } else if ["最新", "新上架", "刚发布"].iter().any(|w| low.contains(w)) {
        Some("recent".into())
    } else {
        None
    };
    h.goal = if ["礼物", "送给", "送人", "送他", "送她", "gift"].iter().any(|w| low.contains(w)) {
        Some("gift".into())
    } else if ["货源", "批发", "进货", "厂家", "供应商", "wholesale"].iter().any(|w| low.contains(w)) {
        Some("source".into())
    } else if h.monitor_minutes.is_some() {
        Some("track".into())
    } else if ["比价", "对比", "哪里买", "哪家便宜", "最便宜", "compare"].iter().any(|w| low.contains(w)) {
        Some("compare".into())
    } else if ["分析", "趋势", "行情", "调研", "市场"].iter().any(|w| low.contains(w)) {
        Some("research".into())
    } else {
        None
    };

    // 剩余文本作为兜底关键词
    let mut chars: Vec<char> = text.chars().collect();
    let byte_to_char = |b: usize| text[..b].chars().count();
    for (s, e) in &used {
        for c in chars.iter_mut().take(byte_to_char(*e)).skip(byte_to_char(*s)) {
            *c = ' ';
        }
    }
    let mut rest: String = chars.into_iter().collect();
    let fillers = [
        "帮我", "帮忙", "请", "给我", "我想", "我要", "想要", "想买", "要买", "打算买", "找一下", "找找", "找", "搜一下", "搜索", "搜", "看看",
        "看一下", "查一下", "查", "一下", "推荐", "哪里买", "哪家", "比价", "对比", "便宜", "最便宜", "的", "在", "上", "里", "和", "跟",
        "预算", "价格", "全新", "二手", "国内", "国际", "海外", "平台", "全平台", "所有", "淘宝", "天猫", "京东", "拼多多", "闲鱼", "小红书",
        "抖音", "微博", "亚马逊", "德国", "美国", "英国", "日本", "法国", "please", "find", "search", "for", "me", "on", "the", "cheapest",
        "buy", "want", "to", "a",
    ];
    for (alias, _) in platforms::aliases() {
        rest = rest.replace(alias, " ");
    }
    let mut words: Vec<String> = vec![];
    for chunk in rest.split(|c: char| c.is_whitespace() || "，。,;；!！?？:：、/()（）【】[]\"'“”".contains(c)) {
        let mut w = chunk.to_string();
        for f in fillers.iter().filter(|f| !f.is_ascii()) {
            if w.chars().count() > f.chars().count() {
                if let Some(s) = w.strip_prefix(f) {
                    w = s.to_string();
                }
                if let Some(s) = w.strip_suffix(f) {
                    w = s.to_string();
                }
            }
        }
        let w = w.trim().to_string();
        if !w.is_empty() && !fillers.contains(&w.as_str()) && !(w.chars().count() == 1 && !w.is_ascii()) {
            words.push(w);
        }
    }
    h.remainder = words.join(" ");
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_with_currency() {
        let h = parse("帮我在亚马逊德国和 eBay 找 Sony WH-1000XM5，200 欧以内，不要二手");
        assert_eq!(h.price_max, Some(200.0));
        assert_eq!(h.currency.as_deref(), Some("EUR"));
        assert_eq!(h.condition.as_deref(), Some("new"));
        assert!(h.platforms.contains(&"amazon_de".to_string()));
        assert!(h.platforms.iter().any(|p| p.starts_with("ebay")));
        assert!(h.exclude.contains(&"二手".to_string()));
        assert!(h.remainder.contains("Sony WH-1000XM5"), "remainder: {}", h.remainder);
    }

    #[test]
    fn range_and_units() {
        let h = parse("淘宝京东搜蓝牙耳机，价格50-300元，按销量");
        assert_eq!((h.price_min, h.price_max), (Some(50.0), Some(300.0)));
        assert_eq!(h.sort.as_deref(), Some("sales"));
        assert_eq!(h.platforms, vec!["taobao".to_string(), "jd".to_string()]);
        let h = parse("预算3k到5k的微单相机");
        assert_eq!((h.price_min, h.price_max), (Some(3000.0), Some(5000.0)));
    }

    #[test]
    fn numbers_not_prices() {
        let h = parse("最近3天的256G手机");
        assert_eq!(h.price_max, None);
        assert_eq!(h.price_min, None);
    }

    #[test]
    fn monitor_and_target() {
        let h = parse("盯一下 Switch 2 的价格，降到 1500 元提醒我");
        assert_eq!(h.target_price, Some(1500.0));
        assert!(h.monitor_minutes.is_some());
        assert_eq!(h.goal.as_deref(), Some("track"));
        let h = parse("每2小时监控一次 iPhone 16");
        assert_eq!(h.monitor_minutes, Some(120));
    }

    #[test]
    fn english_words_not_mangled() {
        let h = parse("lenovo laptop for a camera canon");
        assert!(h.exclude.is_empty(), "{:?}", h.exclude);
        assert!(h.remainder.contains("lenovo") && h.remainder.contains("camera") && h.remainder.contains("canon"), "{}", h.remainder);
        assert!(!h.remainder.split(' ').any(|w| w == "for" || w == "a"));
        assert_eq!(parse("find headphones, no refurbished").exclude, vec!["refurbished".to_string()]);
    }

    #[test]
    fn gift_goal_and_cn_numbers() {
        assert_eq!(parse("给爸爸挑个生日礼物，五百元以内").goal.as_deref(), Some("gift"));
        assert_eq!(cn_number("二十"), Some(20.0));
        assert_eq!(cn_number("两千"), Some(2000.0));
        assert_eq!(cn_number("十五"), Some(15.0));
        assert_eq!(cn_number("3"), Some(3.0));
    }
}
