//! 评论检索（v26.03）：打开排名靠前商品的详情页，读取评分 / 评论数 / 好评率 / 评论原文 / 店铺评分，
//! 规则聚合为 ReviewInsight（AI 分析在 ai::analysis 中进一步补充），并缓存 24 小时。

use crate::browser::extract;
use crate::browser::manager::Browser;
use crate::db::{now_iso, parse_iso, Db};
use crate::models::ReviewInsight;
use crate::platforms::Platform;
use crate::search::parse::parse_number;
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// 从详情页读到的原始评论与店铺信息。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewRaw {
    pub rating: Option<f64>,
    pub count: Option<u64>,
    pub good_rate: Option<f64>,
    pub texts: Vec<String>,
    pub shop_name: Option<String>,
    pub shop_scores: BTreeMap<String, f64>,
    pub shop_years: Option<f64>,
    pub shop_followers: Option<u64>,
    pub shop_badges: Vec<String>,
    pub seller_text: Option<String>,
    pub login_wall: bool,
    pub captcha: bool,
    pub fetched_at: String,
}

impl ReviewRaw {
    pub fn has_shop(&self) -> bool {
        self.shop_name.is_some() || !self.shop_scores.is_empty() || self.shop_years.is_some()
    }
    pub fn has_product(&self) -> bool {
        self.rating.is_some() || self.count.is_some() || self.good_rate.is_some() || !self.texts.is_empty()
    }
}

static RATING_TXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([0-5][.,]\d{1,2})\s*(?:out of 5|von 5|sur 5|/\s*5\b|颗星|分（满分5|stars?\b|Sterne)").unwrap());
static COUNT_TXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d[\d.,]*\s*(?:万|k)?)\s*\+?\s*(?:条评价|个评价|条评论|global ratings|ratings|Sternebewertungen|globale Bewertungen|Bewertungen|reviews|Rezensionen|件の評価|件のレビュー|évaluations)").unwrap()
});
/// "累计评价 2万+" / "商品评价(2万+)"：非"累计"写法必须带括号，避免把"商品评价 9.6"（店铺评分）当成条数
static COUNT_CN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:累计评价\s*[:：]?\s*|(?:商品评价|宝贝评价|评价)\s*[（(]\s*)(\d[\d.]*\s*万?)\s*\+?").unwrap());
static GOOD_RATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:好评率|好评度)\s*[:：]?\s*(\d{2,3}(?:\.\d+)?)\s*%|(\d{2,3}(?:\.\d+)?)\s*%\s*好评").unwrap());
static POSITIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\d{2,3}(?:[.,]\d)?)\s*%\s*(?:positive feedback|positive Bewertungen|positive Bewertung|positives Feedback|positive)").unwrap());
static DSR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(宝贝描述|描述相符|商品描述|描述|服务态度|卖家服务|售后服务|服务|物流服务|物流速度|物流履约|发货速度|物流|商品质量|商品评价)\s*[:：]?\s*(\d{1,2}\.\d{1,2})").unwrap()
});
static YEARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:开店|店龄)\s*(\d{1,2})\s*年|(\d{1,2})\s*年老店|(\d{1,2})\s*年店").unwrap());
static SINCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:member since|mitglied seit|aktiv seit|on etsy since|入驻时间)\s*[:：]?\s*(?:[A-Za-zäöü.]+\s+)?(\d{4})").unwrap());
static FOLLOWERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d[\d.,]*\s*(?:万|k)?)\s*\+?\s*(?:粉丝|人关注|followers|Follower)|(?:关注人数|粉丝数?|followers)\s*[:：]?\s*(\d[\d.,]*\s*(?:万|k)?)").unwrap()
});

fn count_of(raw: &str) -> Option<u64> {
    let t = raw.trim().trim_end_matches('+').trim();
    let (num, mul) = if let Some(n) = t.strip_suffix('万') {
        (n.trim(), 10000.0)
    } else if let Some(n) = t.strip_suffix(['k', 'K']) {
        (n.trim(), 1000.0)
    } else {
        (t, 1.0)
    };
    // 计数没有小数：1.234 / 1,234 都是千分位，除非带 万/k
    let v = if mul > 1.0 { parse_number(num, "en")? } else { num.replace(['.', ',', ' ', '\u{a0}'], "").parse::<f64>().ok()? };
    Some((v * mul).round() as u64)
}

fn rating_of(t: &str) -> Option<f64> {
    RATING_TXT.captures(t).and_then(|c| c[1].replace(',', ".").parse::<f64>().ok()).filter(|r| *r > 0.0 && *r <= 5.0)
}

fn norm_dsr(k: &str) -> &'static str {
    match k {
        "宝贝描述" | "描述相符" | "商品描述" | "描述" => "描述",
        "服务态度" | "卖家服务" | "售后服务" | "服务" => "服务",
        "物流服务" | "物流速度" | "物流履约" | "发货速度" | "物流" => "物流",
        "商品质量" => "质量",
        _ => "评价",
    }
}

/// 解析 read_detail 的结果。
pub fn parse_page(v: &Value) -> ReviewRaw {
    let s = |k: &str| v[k].as_str().unwrap_or("").trim().to_string();
    let body = s("body");
    let shop_blob = format!("{} {}", s("shopScores"), s("seller"));
    let mut raw = ReviewRaw { login_wall: v["loginWall"].as_bool().unwrap_or(false), captcha: v["captcha"].as_bool().unwrap_or(false), fetched_at: now_iso(), ..Default::default() };
    raw.rating = rating_of(&s("rating"))
        .or_else(|| v["ldRating"].as_f64().filter(|r| *r > 0.0 && *r <= 5.0).map(|r| (r * 10.0).round() / 10.0))
        .or_else(|| rating_of(&body));
    raw.count = COUNT_TXT
        .captures(&s("count"))
        .and_then(|c| count_of(&c[1]))
        .or_else(|| COUNT_CN.captures(&s("count")).and_then(|c| count_of(&c[1])))
        .or_else(|| count_of(s("count").trim_matches(|c: char| c == '(' || c == ')' || c == '（' || c == '）')))
        .or_else(|| v["ldCount"].as_f64().map(|c| c as u64))
        .or_else(|| COUNT_TXT.captures(&body).and_then(|c| count_of(&c[1])))
        .or_else(|| COUNT_CN.captures(&body).and_then(|c| count_of(&c[1])));
    let gr = s("goodRate");
    static PCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{2,3}(?:\.\d+)?)\s*%").unwrap());
    raw.good_rate = PCT
        .captures(&gr)
        .and_then(|c| c[1].parse::<f64>().ok())
        .or_else(|| GOOD_RATE.captures(&body).and_then(|c| c.get(1).or(c.get(2))).and_then(|m| m.as_str().parse::<f64>().ok()))
        .filter(|g| *g > 0.0 && *g <= 100.0);
    raw.texts = v["reviews"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|t| t.chars().take(300).collect::<String>()).take(20).collect())
        .unwrap_or_default();
    raw.shop_name = Some(s("shopName")).filter(|n| !n.is_empty() && n.chars().count() <= 40);
    for scope in [&shop_blob, &body] {
        for c in DSR.captures_iter(scope) {
            let k = norm_dsr(&c[1]);
            if let Ok(mut x) = c[2].parse::<f64>() {
                if x > 5.0 && x <= 10.0 {
                    x /= 2.0; // 京东店铺评分为 10 分制
                }
                if x > 0.0 && x <= 5.0 {
                    raw.shop_scores.entry(k.to_string()).or_insert((x * 100.0).round() / 100.0);
                }
            }
        }
        if raw.shop_scores.len() >= 2 {
            break;
        }
    }
    if let Some(p) = POSITIVE.captures(&shop_blob).or_else(|| POSITIVE.captures(&body)).and_then(|c| c[1].replace(',', ".").parse::<f64>().ok()) {
        raw.shop_scores.insert("好评率".into(), p);
    }
    raw.shop_years = YEARS
        .captures(&shop_blob)
        .or_else(|| YEARS.captures(&body))
        .and_then(|c| c.get(1).or(c.get(2)).or(c.get(3)).and_then(|m| m.as_str().parse::<f64>().ok()))
        .or_else(|| {
            SINCE.captures(&shop_blob).or_else(|| SINCE.captures(&body)).and_then(|c| c[1].parse::<i32>().ok()).map(|y| {
                let now: i32 = chrono::Utc::now().format("%Y").to_string().parse().unwrap_or(2026);
                (now - y).max(0) as f64
            })
        });
    raw.shop_followers = FOLLOWERS.captures(&shop_blob).or_else(|| FOLLOWERS.captures(&body)).and_then(|c| c.get(1).or(c.get(2)).and_then(|m| count_of(m.as_str())));
    raw.shop_badges = v["badges"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    raw.seller_text = Some(s("seller")).filter(|t| !t.is_empty()).map(|t| t.chars().take(300).collect());
    raw
}

/// 评论中的风险信号：类别 → 关键词。
const RED: &[(&str, &[&str])] = &[
    ("假货 / 非正品", &["假货", "假的", "不是正品", "仿品", "高仿", "山寨", "fake", "counterfeit", "not genuine", "not original", "fälschung", "nicht original", "偽物"]),
    ("二手 / 翻新", &["二手", "翻新", "被用过", "使用痕迹", "拆过", "used item", "refurbished", "gebraucht", "benutzt"]),
    ("质量问题", &["质量差", "做工差", "坏了", "故障", "不能用", "没用几天", "掉色", "开线", "异味", "broke", "broken", "defective", "stopped working", "poor quality", "kaputt", "defekt", "schlechte qualität"]),
    ("售后差", &["客服差", "不退", "退货难", "不发货", "发错货", "态度差", "no refund", "terrible service", "kundenservice", "rückerstattung"]),
    ("与描述不符", &["与描述不符", "货不对板", "图片不符", "not as described", "nicht wie beschrieben"]),
];

/// 规则聚合：评分 / 好评率 → 分数；关键词 → 风险信号；代表性评论。
pub fn rule_insight(raw: &ReviewRaw) -> ReviewInsight {
    let mut red = vec![];
    let n = raw.texts.len().max(1) as f64;
    for (label, words) in RED {
        let hits = raw.texts.iter().filter(|t| {
            let l = t.to_lowercase();
            words.iter().any(|w| l.contains(&w.to_lowercase()))
        });
        let k = hits.count();
        if k >= 2 || (k == 1 && raw.texts.len() < 10) || k as f64 / n >= 0.15 {
            red.push(format!("{label}（{k} 条评论提到）"));
        }
    }
    let base = match (raw.rating, raw.good_rate) {
        (Some(r), Some(g)) => Some((r * 20.0 + g) / 2.0),
        (Some(r), None) => Some(r * 20.0),
        (None, Some(g)) => Some(g),
        (None, None) if !raw.texts.is_empty() => Some(72.0),
        _ => None,
    };
    let score = base.map(|b| {
        let mut s = b - (red.len().min(3) as f64) * 8.0;
        if raw.count.is_some_and(|c| c < 10) {
            s = 60.0 + (s - 60.0) * 0.5; // 评论太少，向中性收缩
        }
        s.round().clamp(0.0, 100.0) as u32
    });
    let source = match (raw.has_product(), raw.has_shop()) {
        (true, true) => "product+shop",
        (true, false) => "product",
        (false, true) => "shop",
        _ => "none",
    };
    let shop_note = (!raw.shop_scores.is_empty()).then(|| raw.shop_scores.iter().map(|(k, v)| if k == "好评率" { format!("{k} {v:.1}%") } else { format!("{k} {v:.1}") }).collect::<Vec<_>>().join(" · "));
    let unavailable = if raw.has_product() {
        None
    } else if raw.login_wall {
        Some("需要登录才能查看评论".to_string())
    } else if raw.captcha {
        Some("页面要求安全验证，未读取评论".to_string())
    } else {
        Some("页面上没有找到评论".to_string())
    };
    ReviewInsight {
        rating: raw.rating,
        count: raw.count,
        good_rate: raw.good_rate,
        sampled: raw.texts.len() as u32,
        pros: vec![],
        cons: vec![],
        red_flags: red,
        match_note: None,
        score,
        source: source.into(),
        snippets: raw.texts.iter().take(5).map(|t| t.chars().take(120).collect()).collect(),
        shop_note,
        unavailable_reason: unavailable,
        ai_checked: false,
        fetched_at: Some(raw.fetched_at.clone()),
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Stored {
    raw: ReviewRaw,
    insight: Option<ReviewInsight>,
}

/// 24 小时内读过的评论直接复用。
pub fn cached(db: &Db, listing_id: &str, hours: i64) -> Option<ReviewRaw> {
    let (json, at): (Option<String>, Option<String>) = db
        .conn()
        .query_row("SELECT review_json, review_at FROM listings WHERE id=?1", [listing_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .ok()
        .flatten()?;
    let at = parse_iso(&at?)?;
    if chrono::Utc::now() - at > chrono::Duration::hours(hours) {
        return None;
    }
    let st: Stored = serde_json::from_str(&json?).ok()?;
    (st.raw.has_product() || st.raw.has_shop()).then_some(st.raw)
}

/// 保存原始评论（更新 review_at）与分析结果。
pub fn store(db: &Db, listing_id: &str, raw: &ReviewRaw, insight: &ReviewInsight) {
    let json = serde_json::to_string(&Stored { raw: raw.clone(), insight: Some(insight.clone()) }).unwrap_or_default();
    let _ = db.conn().execute("UPDATE listings SET review_json=?2, review_at=?3 WHERE id=?1", params![listing_id, json, raw.fetched_at]);
}

/// 读取保存的分析结果（历史回放 / 商品详情）。
pub fn stored_insight(json: Option<String>) -> Option<ReviewInsight> {
    serde_json::from_str::<Stored>(&json?).ok()?.insight
}

/// 并发读取多个详情页（受浏览器并发槽位限制，另加本阶段上限）；总时长受 budget 限制，超时的条目报告原因。
pub async fn fetch_many(
    browser: &Arc<Browser>, jobs: Vec<(String, Platform, String)>, per_page: Duration, budget: Duration, concurrency: usize, cancel: &CancellationToken,
    on_done: &(dyn Fn(usize, usize) + Send + Sync),
) -> Vec<(String, Result<ReviewRaw, String>)> {
    use futures_util::stream::{self, StreamExt};
    let total = jobs.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let started = std::time::Instant::now();
    let fut = stream::iter(jobs.into_iter().map(|(id, p, url)| {
        let done = &done;
        async move {
            if cancel.is_cancelled() {
                return (id, Err("已取消".to_string()));
            }
            let left = budget.saturating_sub(started.elapsed());
            if left < Duration::from_secs(4) {
                return (id, Err("评论检索时间已用完，跳过".to_string()));
            }
            let r = match tokio::time::timeout(per_page.min(left), extract::read_detail(browser, &p, &url, cancel)).await {
                Ok(Ok(v)) => Ok(parse_page(&v)),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("详情页加载超时".to_string()),
            };
            let n = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            on_done(n, total);
            (id, r)
        }
    }))
    .buffer_unordered(concurrency.max(1))
    .collect::<Vec<_>>();
    fut.await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_amazon_like_page() {
        let v = json!({
            "rating": "4,6 von 5", "count": "12.345 Sternebewertungen", "goodRate": "", "ldRating": null, "ldCount": null,
            "reviews": ["Super Klang, Geräuschunterdrückung top", "Nach 2 Wochen kaputt, defekt", "Leider kaputt gegangen"],
            "shopName": "", "shopScores": "", "seller": "Verkauf und Versand durch Amazon", "badges": [], "body": "", "loginWall": false, "captcha": false
        });
        let raw = parse_page(&v);
        assert_eq!(raw.rating, Some(4.6));
        assert_eq!(raw.count, Some(12345));
        assert_eq!(raw.texts.len(), 3);
        let ins = rule_insight(&raw);
        assert!(ins.red_flags.iter().any(|f| f.starts_with("质量问题")), "{:?}", ins.red_flags);
        assert_eq!(ins.source, "product");
        assert!(ins.score.unwrap() < 92);
    }

    #[test]
    fn parse_jd_like_page_with_shop() {
        let v = json!({
            "rating": "", "count": "", "goodRate": "好评度 98%", "reviews": [],
            "shopName": "索尼京东自营旗舰店", "shopScores": "商品评价 9.6 物流履约 9.5 售后服务 9.4", "seller": "", "badges": ["自营"],
            "body": "商品评价(2万+) 开店 8 年 关注人数 12万", "loginWall": false, "captcha": false
        });
        let raw = parse_page(&v);
        assert_eq!(raw.good_rate, Some(98.0));
        assert_eq!(raw.count, Some(20000));
        assert_eq!(raw.shop_scores.get("物流"), Some(&4.75));
        assert_eq!(raw.shop_years, Some(8.0));
        assert_eq!(raw.shop_followers, Some(120000));
        let ins = rule_insight(&raw);
        assert_eq!(ins.source, "product+shop");
        assert_eq!(ins.score, Some(98));
        assert!(ins.shop_note.unwrap().contains("服务"));
    }

    #[test]
    fn unavailable_reasons() {
        let raw = ReviewRaw { login_wall: true, ..Default::default() };
        assert_eq!(rule_insight(&raw).unavailable_reason.as_deref(), Some("需要登录才能查看评论"));
        assert_eq!(rule_insight(&raw).score, None);
        let ebay = parse_page(&json!({"shopScores": "99,6 % positive Bewertungen Mitglied seit Mär 2012", "reviews": []}));
        assert_eq!(ebay.shop_scores.get("好评率"), Some(&99.6));
        assert!(ebay.shop_years.unwrap() >= 13.0);
    }
}
