//! 卡片解析：标题 / 价格（按字号与币种）/ 原价 / 销量 / 评分 / 店铺 / 成色。支持中文、英文、德文、日文常见写法。

use crate::platforms::Platform;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// 函数内使用的正则：首次使用时编译一次（避免逐卡片重复编译）。
macro_rules! re {
    ($s:expr) => {{
        static R: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| Regex::new($s).unwrap());
        &*R
    }};
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RawLine {
    #[serde(default)]
    pub t: String,
    /// 字号（px）
    #[serde(default)]
    pub fs: f32,
    /// 字重
    #[serde(default)]
    pub fw: f32,
    /// 是否链接文字
    #[serde(default)]
    pub a: bool,
    /// 是否删除线（原价）
    #[serde(default)]
    pub s: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RawCard {
    pub id: String,
    pub href: String,
    pub img: String,
    pub title_attr: String,
    pub nolink: bool,
    pub lines: Vec<RawLine>,
    /// v26.03：平台提示选择器 / JSON-LD 读到的结构化字段（优先于文字块推断）
    pub h: Option<HintFields>,
}

/// v26.03：按平台选择器直接读到的字段（全是原始文字，由 Rust 解析）。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HintFields {
    pub title: String,
    pub price: String,
    pub orig: String,
    pub shop: String,
    pub sales: String,
    pub rating: String,
    pub reviews: String,
    pub location: String,
    pub ship: String,
    pub badges: Vec<String>,
    /// json-ld 来源时价格已是纯数字 + 币种
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Candidate {
    pub platform_id: String,
    pub external_id: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub price: Option<f64>,
    pub currency: String,
    pub original_price: Option<f64>,
    pub sales: Option<i64>,
    pub rating: Option<f64>,
    pub reviews: Option<i64>,
    pub shop: Option<String>,
    pub condition: String,
    pub is_ad: bool,
    /// 卡片原始文字（交给 AI 抽取时使用）
    pub text: String,
    /// 0~1：规则解析的把握
    pub confidence: f32,
    pub query: String,
    /// 运费（0 = 包邮/免运费；None = 未知），与 price 同币种
    pub shipping: Option<f64>,
    /// 券后 / 满减后 / 优惠后价格
    pub coupon_price: Option<f64>,
    /// v26.03：卡片上的店铺 / 商品徽标（天猫、自营、旗舰店、Prime、Privat、Top Rated…）
    #[serde(default)]
    pub badges: Vec<String>,
    /// v26.03：发货地
    #[serde(default)]
    pub location: Option<String>,
}

impl Candidate {
    /// 到手价 = （券后价或标价）+ 运费
    pub fn landed(&self) -> Option<f64> {
        let base = self.coupon_price.or(self.price)?;
        Some(((base + self.shipping.unwrap_or(0.0)) * 100.0).round() / 100.0)
    }
}

/// 数字解析：根据 locale 判断 `.`/`,` 是千分位还是小数点；兼容 `1.299,00` `1,299.00` `1 299,00` `12.5万`。
pub fn parse_number(raw: &str, locale: &str) -> Option<f64> {
    let s: String = raw.trim().chars().filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'')).collect();
    if s.is_empty() {
        return None;
    }
    let (body, mult) = if let Some(b) = s.strip_suffix('万') {
        (b.to_string(), 10000.0)
    } else if let Some(b) = s.strip_suffix(['k', 'K']) {
        (b.to_string(), 1000.0)
    } else {
        (s.clone(), 1.0)
    };
    let comma_decimal = locale.starts_with("de") || locale.starts_with("fr") || locale.starts_with("es") || locale.starts_with("it") || locale.starts_with("nl");
    let last_dot = body.rfind('.');
    let last_comma = body.rfind(',');
    let normalized = match (last_dot, last_comma) {
        (Some(d), Some(c)) => {
            if c > d {
                body.replace('.', "").replace(',', ".")
            } else {
                body.replace(',', "")
            }
        }
        (None, Some(c)) => {
            let decimals = body.len() - c - 1;
            if comma_decimal && decimals <= 2 || decimals == 1 || decimals == 2 && body.matches(',').count() == 1 && comma_decimal {
                body.replace(',', ".")
            } else if decimals == 3 || !comma_decimal {
                body.replace(',', "")
            } else {
                body.replace(',', ".")
            }
        }
        (Some(d), None) => {
            let decimals = body.len() - d - 1;
            if body.matches('.').count() > 1 || (comma_decimal && decimals == 3) {
                body.replace('.', "")
            } else {
                body.clone()
            }
        }
        (None, None) => body.clone(),
    };
    normalized.parse::<f64>().ok().map(|v| (v * mult * 100.0).round() / 100.0)
}

/// 价格匹配：符号在前或在后。
static PRICE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(US\s?\$|C\$|A\$|HK\$|S\$|\$|€|£|¥|￥|EUR|USD|GBP|JPY|CNY|RMB)\s?(\d{1,3}(?:[.,\s\u{a0}]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?)|(\d{1,3}(?:[.,\s\u{a0}]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?)\s?(€|EUR|元|円|£|\$|USD|GBP|JPY|kr)").unwrap()
});
static COUPON_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:券|元券|减|off|Rabatt|coupon|Coupon|/\s*(?:Monat|month|月|kg|l|100\s?g|Stück)|(?i:shipping|delivery|Versand|Lieferung|livraison|送料|运费|邮费))").unwrap()
});
static COUPON_BEFORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:满|减|券|省|Spare|Save|save|spare|coupon|Coupon|Rabatt|Versand|shipping|Lieferung|运费)\s*$").unwrap());

fn currency_code(sym: &str, platform: &Platform) -> String {
    let u = sym.to_uppercase().replace(' ', "");
    match u.as_str() {
        "€" | "EUR" => "EUR".into(),
        "£" | "GBP" => "GBP".into(),
        "$" => if platform.currency == "CNY" { "USD".into() } else if ["USD", "CAD", "AUD", "HKD", "SGD"].contains(&platform.currency.as_str()) { platform.currency.clone() } else { "USD".into() },
        "US$" | "USD" => "USD".into(),
        "C$" => "CAD".into(),
        "A$" => "AUD".into(),
        "HK$" => "HKD".into(),
        "S$" => "SGD".into(),
        "円" | "JPY" => "JPY".into(),
        "元" | "CNY" | "RMB" => "CNY".into(),
        "¥" | "￥" => if platform.currency == "JPY" { "JPY".into() } else { "CNY".into() },
        "KR" => if platform.locale.starts_with("da") || platform.locale.starts_with("no") { "DKK".into() } else { "SEK".into() },
        _ => platform.currency.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceHit {
    pub value: f64,
    pub currency: String,
}

/// 在一行文字里找价格（跳过券、满减、运费、月供）。
pub fn prices_in(text: &str, platform: &Platform) -> Vec<PriceHit> {
    let mut out = vec![];
    for c in PRICE_RE.captures_iter(text) {
        let m = c.get(0).unwrap();
        let before = &text[..m.start()];
        let after = &text[m.end()..];
        if COUPON_AFTER.is_match(after) || COUPON_BEFORE.is_match(before) {
            continue;
        }
        let (sym, num) = if let Some(n) = c.get(2) { (c.get(1).unwrap().as_str(), n.as_str()) } else { (c.get(4).unwrap().as_str(), c.get(3).unwrap().as_str()) };
        // 没有币种符号的纯 "kr"/"$" 歧义情况交给平台默认
        if let Some(v) = parse_number(num, &platform.locale) {
            if v > 0.0 && v < 10_000_000.0 {
                out.push(PriceHit { value: v, currency: currency_code(sym, platform) });
            }
        }
    }
    out
}

static SALES_CN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+(?:\.\d+)?)\s*(万|w|W)?\+?\s*(?:人付款|人收货|人已买|人想要|件已售|已售|销量|人拼|笔成交|人购买)|(?:已拼|已售|已抢|月销|总销量|累计销量|销量)\s*(\d+(?:\.\d+)?)\s*(万|w|W)?\+?").unwrap()
});
static SALES_EN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*(k|K|万)?\+?\s*(?:sold|bought in past month|verkauft|Mal (?:im letzten Monat )?gekauft|件購入|個販売)").unwrap()
});
static RATING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([0-5][.,]\d)\s*(?:out of 5|von 5|sur 5|/\s*5|颗星|星|stars?|Sterne|Sternen|★)").unwrap()
});
static REVIEWS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d{1,3}(?:[.,]\d{3})*|\d+(?:\.\d+)?\s*(?:万|k|K)?)\+?\s*(?:ratings?|reviews?|Bewertungen|Rezensionen|avis|条评价|评价|件のレビュー|件)").unwrap()
});
static SHOP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(旗舰店|专营店|专卖店|官方店|自营|小店|店铺|有限公司|商行|工厂|厂)$|(?i)(?:^Visit the (.+) Store$|^Besuche den (.+)-Store$|^von (.+)$|^Brand: (.+)$|^Marke: (.+)$)").unwrap()
});
static JUNK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:满\d+.*减.*|\d+元券|券后.*|包邮|百亿补贴|官方补贴|退货包运费|极速退款|广告|赞|收藏|评论|转发|Sponsored|Gesponsert|Anzeige|Werbung|Ad|Free (?:shipping|delivery).*|Kostenlose Lieferung.*|GRATIS.*|Lieferung.*|Delivery.*|Add to cart|In den Einkaufswagen|加入购物车|找同款|看相似|更多|Mehr|Options?:.*|\+\s*\d+\s*(?:colors?|Farben).*|[\d:./\- ]+|\d+天前|\d+小时前|\d+分钟前|昨天.*|刚刚)$").unwrap()
});

fn is_junk(t: &str, platform: &Platform) -> bool {
    let n = t.chars().count();
    n < 3 || JUNK_RE.is_match(t) || (!prices_in(t, platform).is_empty() && n <= 22) || SALES_CN.is_match(t) && n <= 16
        || SALES_EN.is_match(t) && n <= 40 || RATING_RE.is_match(t) && n <= 30 || REVIEWS_RE.is_match(t) && n <= 20
}

fn count(num: &str, unit: Option<&str>, locale: &str) -> Option<i64> {
    let base = parse_number(num, locale)?;
    let mul = match unit.map(|u| u.to_lowercase()) {
        Some(u) if u == "万" || u == "w" => 10000.0,
        Some(u) if u == "k" => 1000.0,
        _ => 1.0,
    };
    Some((base * mul).round() as i64)
}

/// 合并被拆成多行的价格：`¥` / `189` / `.00` → `¥189.00`
fn merge_lines(lines: &[RawLine]) -> Vec<RawLine> {
    let mut out: Vec<RawLine> = vec![];
    let sym_only = re!(r"^(?:[¥￥$€£]|US\s?\$|HK\$)$");
    let dec_only = re!(r"^[.,]\d{1,2}$");
    let digits_only = re!(r"^\d[\d.,]*$");
    for l in lines {
        let t = l.t.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(prev) = out.last_mut() {
            let pt = prev.t.trim().to_string();
            if sym_only.is_match(&pt) && digits_only.is_match(t) {
                prev.t = format!("{pt}{t}");
                prev.fs = prev.fs.max(l.fs);
                continue;
            }
            if dec_only.is_match(t) && PRICE_RE.is_match(&pt) && !pt.contains(['.', ',']) {
                prev.t = format!("{pt}{t}");
                continue;
            }
            if (t == "€" || t == "円" || t == "元") && digits_only.is_match(&pt) {
                prev.t = format!("{pt} {t}");
                continue;
            }
        }
        out.push(RawLine { t: t.to_string(), ..l.clone() });
    }
    out
}

fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(s.as_bytes());
    format!("t{}", hex::encode(&d[..6]))
}

static FREE_SHIP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)包邮|免运费|运费\s*[:：]?\s*[¥￥]?\s*0(?:\.0+)?(?:元)?(?:\D|$)|free (?:shipping|delivery)|kostenlose[rn]? (?:Versand|Lieferung)|gratis[- ]?(?:versand|lieferung)|versandkostenfrei|livraison gratuite|送料無料").unwrap()
});
static SHIP_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)shipping|delivery|Versand|Lieferung|livraison|送料|运费|邮费").unwrap());
static COUPON_FIXED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:券后价?|到手价|折后价?|活动价|补贴后)\s*[:：]?\s*[¥￥]?\s*(\d+(?:\.\d+)?)").unwrap()
});
static COUPON_PCT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:save|spare|sparen?)\s*(\d{1,2})\s*%\s*(?:with coupon|mit (?:coupon|gutschein))|(\d{1,2})\s*%\s*(?:coupon|gutschein)").unwrap()
});
static COUPON_FULL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"满\s*(\d+)\s*(?:元)?\s*减\s*(\d+)").unwrap());

/// 不做"券/运费"过滤的原始价格抽取（用于识别运费金额本身）。
fn raw_prices(text: &str, platform: &Platform) -> Vec<PriceHit> {
    PRICE_RE
        .captures_iter(text)
        .filter_map(|c| {
            let (sym, num) = if let Some(n) = c.get(2) { (c.get(1)?.as_str(), n.as_str()) } else { (c.get(4)?.as_str(), c.get(3)?.as_str()) };
            parse_number(num, &platform.locale).map(|v| PriceHit { value: v, currency: currency_code(sym, platform) })
        })
        .collect()
}

/// 运费：包邮/免运费 → 0；"+ 4,99 € Versand" / "运费 ¥8" → 金额；未知 → None。
pub fn shipping_of(lines: &[String], platform: &Platform, price: Option<f64>, currency: &str) -> Option<f64> {
    let all = lines.join("\n");
    if FREE_SHIP.is_match(&all) {
        return Some(0.0);
    }
    for l in lines {
        if !SHIP_WORD.is_match(l) {
            continue;
        }
        if let Some(hit) = raw_prices(l, platform).into_iter().find(|h| h.currency == currency) {
            if hit.value > 0.0 && price.is_none_or(|p| hit.value < p) && hit.value < 200.0 {
                return Some(hit.value);
            }
        }
    }
    None
}

/// 券后价：明确的"券后/到手价"金额 > 百分比券 > 满减（满足门槛时）。
pub fn coupon_of(text: &str, price: Option<f64>) -> Option<f64> {
    let p = price?;
    let round = |x: f64| (x * 100.0).round() / 100.0;
    if let Some(v) = COUPON_FIXED.captures(text).and_then(|c| c[1].parse::<f64>().ok()) {
        if v > 0.0 && v < p {
            return Some(round(v));
        }
    }
    if let Some(pct) = COUPON_PCT.captures(text).and_then(|c| c.get(1).or(c.get(2))).and_then(|m| m.as_str().parse::<f64>().ok()) {
        if pct > 0.0 && pct < 90.0 {
            return Some(round(p * (1.0 - pct / 100.0)));
        }
    }
    let mut best: Option<f64> = None;
    for c in COUPON_FULL.captures_iter(text) {
        if let (Ok(th), Ok(minus)) = (c[1].parse::<f64>(), c[2].parse::<f64>()) {
            if p >= th && minus < p {
                best = Some(best.map_or(p - minus, |b: f64| b.min(p - minus)));
            }
        }
    }
    best.map(round)
}

/// 卡片上常见的店铺 / 资质徽标（整行匹配，短文字）。
static BADGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:天猫|天猫超市|天猫国际|淘宝直播|京东自营|自营|京东物流|京东超市|京东国际|旗舰店|官方旗舰店|官方|官方店|官方直营|品牌|品牌直营|品牌授权|企业|企业店|金牌卖家|源头工厂|超级工厂|实力商家|诚信通|深度验商|百亿补贴|品牌黑标|正品保障|假一赔[四十]|退货包运费|验货宝|个人闲置|个人|卖家信用极好|信用极好|芝麻信用[^\s]{0,4}|全球购|海外直邮|跨境|Amazon'?s Choice|Amazon Choice|Bestseller|Best Seller|#1 Best Seller|Top Rated(?: Plus)?|Top-Bewertung|Topbewertet|Prime|Privat(?:er (?:Verkäufer|Nutzer))?|Gewerblich(?:er Nutzer)?|Private seller|Business seller|Brand new|Refurbished|Generalüberholt|Gebraucht|Pre-owned|Used|Climate Pledge Friendly|Choice|Plus|Official Store|Offizieller Shop)$").unwrap()
});

pub fn badges_of(lines: &[String], extra: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in extra.iter().chain(lines.iter()) {
        let t = t.trim();
        if t.is_empty() || t.chars().count() > 18 {
            continue;
        }
        let ok = BADGE_RE.is_match(t) || extra.iter().any(|e| e.trim() == t);
        if ok && !out.iter().any(|o| o == t) {
            out.push(t.to_string());
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

static TITLE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:\s*(?:广告|推广|Sponsored(?: Ad)?|Gesponsert(?:e Anzeige)?|Anzeige|Werbung|Neues Angebot|New Listing|NEU!?|Ad)\s*[-–—:|·]?\s*)+").unwrap()
});

/// 标题清洗：去掉"广告 / Gesponsert / Neues Angebot"等前缀、多余空白；长度上限 300。
pub fn clean_title(t: &str) -> String {
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = TITLE_PREFIX.replace(&t, "").to_string();
    t.trim_matches(|c: char| c == '|' || c == '-' || c == '–' || c.is_whitespace()).chars().take(300).collect()
}

/// 提示字段里的价格：带币种的文字优先；纯数字（如京东 `.p-price i`）用平台币种。
fn hint_price(text: &str, currency: &str, platform: &Platform) -> Option<PriceHit> {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // "¥ 1299 .00" → "¥1299.00"
    let t = re!(r"([¥￥$€£])\s+(\d)").replace_all(&t, "$1$2").to_string();
    let t = re!(r"(\d)\s+([.,]\d{1,2})\b").replace_all(&t, "$1$2").to_string();
    if let Some(h) = prices_in(&t, platform).into_iter().next() {
        return Some(h);
    }
    let num = re!(r"\d[\d.,\s]*").find(&t)?.as_str().trim().to_string();
    let v = parse_number(&num, &platform.locale)?;
    (v > 0.0 && v < 10_000_000.0).then(|| PriceHit { value: v, currency: if currency.len() == 3 { currency.to_uppercase() } else { platform.currency.clone() } })
}

/// 销量文字：月销 1000+ / 已售 5万+ / 10万+人付款 / 1K+ bought in past month / 23 verkauft。
pub fn parse_sales(text: &str, locale: &str) -> Option<i64> {
    SALES_CN
        .captures(text)
        .and_then(|c| if let Some(n) = c.get(1) { count(n.as_str(), c.get(2).map(|m| m.as_str()), "zh") } else { count(&c[3], c.get(4).map(|m| m.as_str()), "zh") })
        .or_else(|| SALES_EN.captures(text).and_then(|c| count(&c[1], c.get(2).map(|m| m.as_str()), locale)))
}

fn parse_reviews_count(text: &str, locale: &str) -> Option<i64> {
    if let Some(c) = REVIEWS_RE.captures(text) {
        let raw = c[1].trim().to_string();
        let (num, unit) = if let Some(n) = raw.strip_suffix('万') { (n.trim().to_string(), Some("万")) } else if let Some(n) = raw.strip_suffix(['k', 'K']) { (n.trim().to_string(), Some("k")) } else { (raw, None) };
        return count(&num, unit, locale);
    }
    // 只有数字：京东 "2万+"、亚马逊 "(1.234)" / "1,234"
    let t = text.trim().trim_matches(|c| c == '(' || c == ')' || c == '+').trim();
    let re = re!(r"^(\d[\d.,]*)\s*(万|[kK])?\+?$");
    re.captures(t).and_then(|c| count(&c[1], c.get(2).map(|m| m.as_str()), locale))
}

fn parse_rating(text: &str) -> Option<f64> {
    RATING_RE
        .captures(text)
        .and_then(|c| c[1].replace(',', ".").parse::<f64>().ok())
        .or_else(|| {
            let t = text.trim();
            re!(r"^([0-5](?:[.,]\d)?)$").captures(t).and_then(|c| c[1].replace(',', ".").parse::<f64>().ok())
        })
        .filter(|r| *r > 0.0 && *r <= 5.0)
}

pub fn parse_card(platform: &Platform, raw: &RawCard, query: &str) -> Option<Candidate> {
    let mut lines = merge_lines(&raw.lines);
    let hint = raw.h.clone().unwrap_or_default();
    if lines.is_empty() {
        // JSON-LD / 提示字段来源但没有文字块时，用字段合成
        for t in [&hint.title, &hint.price, &hint.shop, &hint.sales] {
            if !t.trim().is_empty() {
                lines.push(RawLine { t: t.trim().to_string(), fs: 14.0, fw: 400.0, a: false, s: false });
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let text: String = lines.iter().map(|l| l.t.as_str()).collect::<Vec<_>>().join("\n");

    // 价格：未划线、字号最大的价格；划线价为原价
    let mut best: Option<(f32, PriceHit)> = None;
    let mut original: Option<f64> = None;
    for l in &lines {
        for hit in prices_in(&l.t, platform) {
            if l.s {
                original = original.or(Some(hit.value));
                continue;
            }
            let weight = l.fs.max(1.0) + if l.fw >= 600.0 { 2.0 } else { 0.0 };
            if best.as_ref().is_none_or(|(w, _)| weight > *w + 0.5) {
                best = Some((weight, hit));
            }
        }
    }
    let hinted_price = (!hint.price.trim().is_empty()).then(|| hint_price(&hint.price, &hint.currency, platform)).flatten();
    let (price, currency) = match (&hinted_price, &best) {
        (Some(h), _) => (Some(h.value), h.currency.clone()),
        (None, Some((_, h))) => (Some(h.value), h.currency.clone()),
        (None, None) => (None, platform.currency.clone()),
    };
    if let Some(o) = (!hint.orig.trim().is_empty()).then(|| hint_price(&hint.orig, &hint.currency, platform)).flatten() {
        original = Some(o.value);
    }
    let original_price = original.filter(|o| price.is_none_or(|p| *o > p));
    let mut line_texts: Vec<String> = lines.iter().map(|l| l.t.clone()).collect();
    if !hint.ship.trim().is_empty() {
        line_texts.push(hint.ship.trim().to_string());
    }
    let shipping = shipping_of(&line_texts, platform, price, &currency);
    let coupon_price = coupon_of(&text, price);

    // 标题：title 属性 > 链接文字最长行 > 字号最大的长行 > 最长行
    let ok = |t: &str| !is_junk(t, platform) && t.chars().count() >= 4;
    let attr = raw.title_attr.trim();
    let hinted_title = clean_title(&hint.title);
    let title = if hinted_title.chars().count() >= 6 && !is_junk(&hinted_title, platform) {
        hinted_title
    } else if attr.chars().count() >= 6 && !is_junk(attr, platform) {
        attr.to_string()
    } else {
        let linked = lines.iter().filter(|l| l.a && ok(&l.t)).max_by_key(|l| l.t.chars().count()).map(|l| l.t.clone());
        let big = lines
            .iter()
            .filter(|l| ok(&l.t) && l.t.chars().count() >= 6)
            .max_by(|a, b| (a.fs + a.t.chars().count().min(40) as f32 / 20.0).total_cmp(&(b.fs + b.t.chars().count().min(40) as f32 / 20.0)))
            .map(|l| l.t.clone());
        let longest = lines.iter().filter(|l| ok(&l.t)).max_by_key(|l| l.t.chars().count()).map(|l| l.t.clone());
        linked.filter(|t| t.chars().count() >= 8).or(big).or(longest)?
    };
    let title = clean_title(&title);
    if title.chars().count() < 4 || PLACEHOLDER_TITLE.is_match(&title) {
        return None;
    }

    let sales = (!hint.sales.trim().is_empty())
        .then(|| parse_sales(&hint.sales, &platform.locale).or_else(|| parse_reviews_count(&hint.sales, &platform.locale).filter(|_| hint.sales.contains(['+', '万']))))
        .flatten()
        .or_else(|| parse_sales(&text, &platform.locale));
    let rating = (!hint.rating.trim().is_empty()).then(|| parse_rating(&hint.rating)).flatten().or_else(|| {
        RATING_RE.captures(&text).and_then(|c| c[1].replace(',', ".").parse::<f64>().ok()).filter(|r| *r <= 5.0)
    });
    let reviews = (!hint.reviews.trim().is_empty())
        .then(|| parse_reviews_count(&hint.reviews, &platform.locale))
        .flatten()
        .or_else(|| parse_reviews_count_in(&text, &platform.locale));
    let hinted_shop = hint.shop.trim();
    let shop = if !hinted_shop.is_empty() && hinted_shop.chars().count() <= 40 {
        Some(clean_shop(hinted_shop))
    } else {
        lines.iter().find_map(|l| {
            let t = l.t.trim();
            if t == title || t.chars().count() > 40 {
                return None;
            }
            SHOP_RE.captures(t).map(|c| (2..=6).find_map(|i| c.get(i).map(|m| m.as_str().to_string())).unwrap_or_else(|| t.to_string()))
        })
    };
    let badges = badges_of(&line_texts, &hint.badges);
    let location = Some(hint.location.trim().to_string()).filter(|l| !l.is_empty() && l.chars().count() <= 20);
    let low = text.to_lowercase();
    let condition = if ["二手", "闲置", "gebraucht", "pre-owned", "used", "refurbished", "generalüberholt", "翻新", "中古"].iter().any(|w| low.contains(w)) {
        "used"
    } else if ["全新", "brand new", "neu &", "neu und", "新品"].iter().any(|w| low.contains(w)) || low.lines().any(|l| l.trim() == "neu" || l.trim() == "new") {
        "new"
    } else {
        "unknown"
    };
    let is_ad = lines.iter().any(|l| {
        let t = l.t.trim();
        t == "广告" || t.eq_ignore_ascii_case("sponsored") || t == "Gesponsert" || t == "Werbung" || t == "Ad"
    });

    let mut url = raw.href.clone();
    let id = raw.id.trim().to_string();
    if url.is_empty() {
        if let Some(u) = platform.detail_url(&id) {
            url = u;
        }
    }
    let external_id = if id.is_empty() || id.starts_with('t') && id.len() <= 10 && !id[1..].chars().all(|c| c.is_ascii_digit()) {
        short_hash(&format!("{}|{}", platform.id, title))
    } else {
        id.chars().take(150).collect()
    };
    let mut confidence: f32 = 0.4;
    if price.is_some() {
        confidence += 0.3;
    }
    if !raw.title_attr.is_empty() || lines.iter().any(|l| l.a && l.t == title) {
        confidence += 0.2;
    }
    if !url.is_empty() {
        confidence += 0.1;
    }

    Some(Candidate {
        platform_id: platform.id.clone(),
        external_id,
        title,
        url,
        image: Some(raw.img.clone()).filter(|u| u.starts_with("http")),
        price,
        currency,
        original_price,
        sales,
        rating,
        reviews,
        shop,
        condition: condition.into(),
        is_ad,
        text: text.chars().take(600).collect(),
        confidence: confidence.min(1.0),
        query: query.to_string(),
        shipping,
        coupon_price,
        badges,
        location,
    })
}

static PLACEHOLDER_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:Shop on eBay|Bei eBay einkaufen|Results matching fewer words|Ergebnisse mit weniger Suchbegriffen)$").unwrap()
});

fn parse_reviews_count_in(text: &str, locale: &str) -> Option<i64> {
    REVIEWS_RE.captures(text).and_then(|c| {
        let raw = c[1].trim().to_string();
        let (num, unit) = if let Some(n) = raw.strip_suffix('万') { (n.trim().to_string(), Some("万")) } else if let Some(n) = raw.strip_suffix(['k', 'K']) { (n.trim().to_string(), Some("k")) } else { (raw, None) };
        count(&num, unit, locale)
    })
}

/// 店铺名清洗：去掉"进店 / 进入店铺 / Visit the … Store"包装。
fn clean_shop(t: &str) -> String {
    let t = t.trim();
    if let Some(c) = SHOP_RE.captures(t) {
        if let Some(m) = (2..=6).find_map(|i| c.get(i)) {
            return m.as_str().trim().to_string();
        }
    }
    // eBay："seller-name (1.234) 99,8%" → 卖家名
    let t = re!(r"\s*\(\d[\d.,]*\)\s*(?:\d{1,3}(?:[.,]\d)?\s*%)?\s*$").replace(t, "").to_string();
    t.trim_end_matches("进店").trim_end_matches('>').trim_end_matches("进入店铺").trim().chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::BUILTIN;

    fn plat(id: &str) -> Platform {
        BUILTIN.iter().find(|p| p.id == id).unwrap().clone()
    }
    fn l(t: &str, fs: f32) -> RawLine {
        RawLine { t: t.into(), fs, fw: 400.0, a: false, s: false }
    }

    #[test]
    fn numbers_by_locale() {
        assert_eq!(parse_number("1.299,00", "de-DE"), Some(1299.0));
        assert_eq!(parse_number("1,299.00", "en-US"), Some(1299.0));
        assert_eq!(parse_number("1 299,99", "fr-FR"), Some(1299.99));
        assert_eq!(parse_number("189,95", "de-DE"), Some(189.95));
        assert_eq!(parse_number("1.299", "de-DE"), Some(1299.0));
        assert_eq!(parse_number("12.5万", "zh-CN"), Some(125000.0));
        assert_eq!(parse_number("2,980", "ja-JP"), Some(2980.0));
        assert_eq!(parse_number("19.99", "en-US"), Some(19.99));
    }

    #[test]
    fn currency_by_platform() {
        let jp = plat("amazon_co_jp");
        assert_eq!(prices_in("￥2,980", &jp), vec![PriceHit { value: 2980.0, currency: "JPY".into() }]);
        let tb = plat("taobao");
        assert_eq!(prices_in("¥189.00", &tb), vec![PriceHit { value: 189.0, currency: "CNY".into() }]);
        let de = plat("amazon_de");
        assert_eq!(prices_in("1.299,00 €", &de), vec![PriceHit { value: 1299.0, currency: "EUR".into() }]);
        assert!(prices_in("满300减40", &tb).is_empty());
        assert!(prices_in("¥20券", &tb).is_empty());
    }

    #[test]
    fn taobao_split_price_and_title() {
        let raw = RawCard {
            id: "712345678901".into(),
            href: "https://item.taobao.com/item.htm?id=712345678901".into(),
            lines: vec![
                RawLine { t: "Sony/索尼 WH-1000XM5 头戴式无线降噪耳机 官方正品".into(), fs: 14.0, fw: 400.0, a: true, s: false },
                l("¥", 12.0), l("1899", 20.0), l(".00", 12.0),
                l("2000+人付款", 12.0),
                l("索尼官方旗舰店", 12.0),
                l("满300减40", 12.0),
            ],
            ..Default::default()
        };
        let c = parse_card(&plat("taobao"), &raw, "WH-1000XM5").unwrap();
        assert_eq!(c.title, "Sony/索尼 WH-1000XM5 头戴式无线降噪耳机 官方正品");
        assert_eq!(c.price, Some(1899.0));
        assert_eq!(c.currency, "CNY");
        assert_eq!(c.sales, Some(2000));
        assert_eq!(c.shop.as_deref(), Some("索尼官方旗舰店"));
    }

    #[test]
    fn amazon_de_card() {
        let raw = RawCard {
            id: "B09Y2MYL5C".into(),
            href: "https://www.amazon.de/dp/B09Y2MYL5C".into(),
            lines: vec![
                l("Gesponsert", 12.0),
                RawLine { t: "Sony WH-1000XM5 Kabellose Kopfhörer mit Noise Cancelling, Schwarz".into(), fs: 16.0, fw: 500.0, a: true, s: false },
                l("4,5 von 5 Sternen", 12.0),
                l("12.345", 12.0),
                l("Mehr als 1.000 Mal im letzten Monat gekauft", 12.0),
                RawLine { t: "289,99 €".into(), fs: 24.0, fw: 700.0, a: false, s: false },
                RawLine { t: "UVP: 419,00 €".into(), fs: 12.0, fw: 400.0, a: false, s: true },
                l("Kostenlose Lieferung Mi., 5. Nov.", 12.0),
            ],
            ..Default::default()
        };
        let c = parse_card(&plat("amazon_de"), &raw, "Sony WH-1000XM5").unwrap();
        assert!(c.title.starts_with("Sony WH-1000XM5 Kabellose"));
        assert_eq!(c.price, Some(289.99));
        assert_eq!(c.currency, "EUR");
        assert_eq!(c.original_price, Some(419.0));
        assert_eq!(c.rating, Some(4.5));
        assert!(c.is_ad);
    }

    #[test]
    fn landed_price_parts() {
        let de = plat("amazon_de");
        let raw = RawCard {
            id: "B0X".into(),
            lines: vec![
                RawLine { t: "Kaffeemühle elektrisch".into(), fs: 15.0, fw: 400.0, a: true, s: false },
                RawLine { t: "39,99 €".into(), fs: 22.0, fw: 700.0, a: false, s: false },
                l("+ 4,99 € Versand", 11.0),
                l("Spare 10 % mit Coupon", 11.0),
            ],
            ..Default::default()
        };
        let c = parse_card(&de, &raw, "x").unwrap();
        assert_eq!(c.price, Some(39.99), "shipping line must not become the price");
        assert_eq!(c.shipping, Some(4.99));
        assert_eq!(c.coupon_price, Some(35.99));
        assert_eq!(c.landed(), Some(40.98));
        let tb = plat("taobao");
        let raw = RawCard { id: "1".into(), lines: vec![l("索尼耳机 官方正品", 14.0), l("¥1899", 20.0), l("券后¥1799", 12.0), l("包邮", 11.0)], ..Default::default() };
        let c = parse_card(&tb, &raw, "x").unwrap();
        assert_eq!((c.price, c.coupon_price, c.shipping), (Some(1899.0), Some(1799.0), Some(0.0)));
        assert_eq!(coupon_of("满300减40", Some(329.0)), Some(289.0));
        assert_eq!(coupon_of("满300减40", Some(199.0)), None);
    }

    #[test]
    fn biggest_price_wins_over_unit_price() {
        let raw = RawCard {
            id: "x1".into(),
            lines: vec![
                RawLine { t: "Kaffeebohnen Crema 1kg".into(), fs: 15.0, fw: 400.0, a: true, s: false },
                l("17,99 €/kg", 11.0),
                RawLine { t: "17,99 €".into(), fs: 22.0, fw: 700.0, a: false, s: false },
            ],
            ..Default::default()
        };
        let c = parse_card(&plat("amazon_de"), &raw, "kaffee").unwrap();
        assert_eq!(c.price, Some(17.99));
    }

    #[test]
    fn card_without_link_uses_detail_template_and_hash_ids() {
        let raw = RawCard { id: "".into(), nolink: true, lines: vec![l("拼多多同款蓝牙耳机 超长续航", 14.0), l("¥39.9", 18.0), l("已拼1.2万件", 12.0)], ..Default::default() };
        let c = parse_card(&plat("pinduoduo"), &raw, "蓝牙耳机").unwrap();
        assert!(c.external_id.starts_with('t'));
        assert_eq!(c.sales, Some(12000));
        let raw2 = RawCard { id: "123456789".into(), nolink: true, lines: raw.lines.clone(), ..Default::default() };
        let c2 = parse_card(&plat("pinduoduo"), &raw2, "x").unwrap();
        assert_eq!(c2.url, "https://mobile.yangkeduo.com/goods.html?goods_id=123456789");
    }

    fn hcard(h: HintFields, lines: &[&str]) -> RawCard {
        RawCard { id: "1".into(), href: "https://x/1".into(), lines: lines.iter().map(|t| l(t, 13.0)).collect(), h: Some(h), ..Default::default() }
    }

    #[test]
    fn hinted_fields_win() {
        let jd = plat("jd");
        let c = parse_card(&jd, &hcard(HintFields { title: "广告 索尼 WH-1000XM5 降噪耳机".into(), price: "2299.00".into(), shop: "索尼京东自营官方旗舰店".into(),
            reviews: "2万+".into(), badges: vec!["自营".into(), "放心购".into()], ..Default::default() }, &["索尼 WH-1000XM5 降噪耳机", "¥2299.00", "2万+条评价"]), "q").unwrap();
        assert_eq!(c.title, "索尼 WH-1000XM5 降噪耳机");
        assert_eq!(c.price, Some(2299.0));
        assert_eq!(c.currency, "CNY");
        assert_eq!(c.shop.as_deref(), Some("索尼京东自营官方旗舰店"));
        assert_eq!(c.reviews, Some(20000));
        assert!(c.badges.contains(&"自营".to_string()) && c.badges.contains(&"放心购".to_string()));

        let tb = plat("taobao");
        let c = parse_card(&tb, &hcard(HintFields { title: "Sony/索尼 WH-1000XM5".into(), price: "¥ 1799 .00".into(), sales: "3000+人付款".into(),
            location: "广东 深圳".into(), ..Default::default() }, &["天猫", "Sony/索尼 WH-1000XM5"]), "q").unwrap();
        assert_eq!(c.price, Some(1799.0));
        assert_eq!(c.sales, Some(3000));
        assert_eq!(c.location.as_deref(), Some("广东 深圳"));
        assert!(c.badges.contains(&"天猫".to_string()));

        let de = plat("amazon_de");
        let c = parse_card(&de, &hcard(HintFields { title: "Gesponsert Sony WH-1000XM5 Kopfhörer".into(), price: "279,00 €".into(), orig: "419,00 €".into(),
            rating: "4,6 von 5 Sternen".into(), reviews: "12.345".into(), sales: "1K+ bought in past month".into(), ..Default::default() }, &[]), "q").unwrap();
        assert_eq!(c.title, "Sony WH-1000XM5 Kopfhörer");
        assert_eq!((c.price, c.original_price), (Some(279.0), Some(419.0)));
        assert_eq!(c.rating, Some(4.6));
        assert_eq!(c.reviews, Some(12345));
        assert_eq!(c.sales, Some(1000));
    }

    #[test]
    fn ld_and_placeholders() {
        let eb = plat("ebay_de");
        let ld = RawCard { id: "".into(), href: "https://www.ebay.de/itm/1".into(), h: Some(HintFields { title: "Sony WH-1000XM5".into(), price: "199.5".into(), currency: "EUR".into(), ..Default::default() }), ..Default::default() };
        let c = parse_card(&eb, &ld, "q").unwrap();
        assert_eq!((c.price, c.currency.as_str()), (Some(199.5), "EUR"));
        let ph = hcard(HintFields { title: "Shop on eBay".into(), price: "20,00 EUR".into(), ..Default::default() }, &["Shop on eBay", "20,00 EUR"]);
        assert!(parse_card(&eb, &ph, "q").is_none());
        assert_eq!(clean_title("Neues Angebot  Sony   WH-1000XM5 "), "Sony WH-1000XM5");
        assert_eq!(parse_sales("月销 1000+", "zh-CN"), Some(1000));
        assert_eq!(parse_sales("已售 5万+", "zh-CN"), Some(50000));
        assert_eq!(parse_sales("23 verkauft", "de-DE"), Some(23));
        assert_eq!(parse_sales("2万+条评价", "zh-CN"), None, "评价数不是销量");
        assert_eq!(clean_shop("kopfhoerer-fan (512) 100%"), "kopfhoerer-fan");
    }
}
