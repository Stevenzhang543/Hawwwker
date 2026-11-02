//! 店铺类型识别与可信度（v26.03）。
//!
//! 依据：店铺名（旗舰店 / 专卖店 / 专营店 / 工作室…）、卡片徽标（天猫、京东自营、Prime、Privat…）、
//! 详情页卖家信息（"Ships from and sold by Amazon" / "Verkauf durch Amazon"）、平台类型（闲鱼、Kleinanzeigen 默认个人卖家）
//! 以及店铺评分、开店年限、粉丝数。只做规则判断，结果可解释（notes）。

use crate::models::ShopInfo;
use crate::platforms::Platform;
use std::collections::BTreeMap;

/// 分类所需的输入（卡片 + 可选的详情页信息）。
#[derive(Debug, Clone, Default)]
pub struct ShopInput<'a> {
    pub shop: Option<&'a str>,
    pub badges: &'a [String],
    pub title: &'a str,
    pub condition: &'a str,
    /// 详情页上的卖家 / 发货说明文字（例如 "Verkauf und Versand durch Amazon"）
    pub seller_text: Option<&'a str>,
    pub rating: Option<f64>,
    pub scores: Option<&'a BTreeMap<String, f64>>,
    pub years: Option<f64>,
    pub followers: Option<u64>,
    pub sales: Option<i64>,
}

pub fn label_of(kind: &str) -> &'static str {
    match kind {
        "flagship" => "旗舰店",
        "official" => "官方店",
        "self_operated" => "平台自营",
        "authorized" => "专卖店",
        "distributor" => "专营店",
        "enterprise" => "企业店",
        "individual" => "个人卖家",
        "studio" => "小工作室",
        "overseas" => "海外代购",
        "secondhand" => "二手商家",
        "marketplace" => "第三方卖家",
        _ => "店铺未知",
    }
}

/// 店铺类型的基础可信度（0–100）。
pub fn base_credibility(kind: &str) -> i32 {
    match kind {
        "self_operated" => 92,
        "flagship" => 90,
        "official" => 88,
        "authorized" => 78,
        "enterprise" => 66,
        "distributor" => 62,
        "marketplace" => 55,
        "studio" => 55,
        "unknown" => 50,
        "overseas" => 45,
        "secondhand" => 45,
        "individual" => 40,
        _ => 50,
    }
}

fn has(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// 识别店铺类型（kind）与一句依据。
pub fn kind_of(p: &Platform, i: &ShopInput) -> (String, String) {
    let shop = i.shop.unwrap_or("").trim();
    let shop_l = shop.to_lowercase();
    let badges_l: Vec<String> = i.badges.iter().map(|b| b.to_lowercase()).collect();
    let badge = |needles: &[&str]| badges_l.iter().any(|b| needles.iter().any(|n| b.contains(n)));
    let seller = i.seller_text.unwrap_or("").to_lowercase();
    let title_l = i.title.to_lowercase();
    let k = |kind: &str, why: String| (kind.to_string(), why);

    // ① 平台自营
    let amazon_self = has(&seller, &["sold by amazon", "verkauf durch amazon", "verkauf und versand durch amazon", "versand und verkauf durch amazon", "ships from and sold by amazon", "vendu par amazon", "amazon.co.jp が販売", "販売元: amazon"])
        || shop_l == "amazon" || shop_l.starts_with("amazon.");
    if amazon_self && p.family.starts_with("amazon") {
        return k("self_operated", "由 Amazon 自营销售与发货".into());
    }
    if badge(&["京东自营", "自营"]) || has(shop, &["京东自营", "自营旗舰店", "自营店"]) || has(&shop_l, &["京东超市", "天猫超市"]) {
        return k("self_operated", format!("{} 平台自营", p.name));
    }
    if p.id == "bestbuy" && shop.is_empty() || has(&seller, &["sold and shipped by best buy", "sold by walmart.com", "sold and shipped by walmart"]) {
        return k("self_operated", format!("{} 自营", p.name));
    }
    // ② 二手 / 回收（店铺名）
    // "优品"只在不是旗舰 / 官方店时才算二手（例如"名创优品官方旗舰店"是品牌官方店）
    let is_brand_store = has(shop, &["旗舰店", "官方店", "官方直营", "专卖店"]);
    if has(shop, &["二手", "回收", "寄卖"]) || (has(shop, &["优品"]) && !is_brand_store) || has(&shop_l, &["warehouse", "renewed", "refurbished", "second hand", "secondhand"]) {
        return k("secondhand", format!("店铺「{shop}」经营二手 / 翻新"));
    }
    // ③ 旗舰店 / 官方店
    if shop.contains("旗舰店") {
        return k("flagship", format!("店铺名为「{shop}」"));
    }
    if has(shop, &["官方店", "官方直营", "官方商城", "品牌直营"]) || has(&shop_l, &["official store", "offizieller shop", "official shop"]) || badge(&["官方旗舰", "official store", "offizieller shop", "品牌直营"]) {
        return k("official", format!("官方店铺「{shop}」"));
    }
    // ④ 专卖 / 专营 / 授权
    if shop.contains("专卖店") || badge(&["品牌授权"]) || shop.contains("授权店") {
        return k("authorized", format!("品牌专卖 / 授权店「{shop}」"));
    }
    if shop.contains("专营店") {
        return k("distributor", format!("多品牌专营店「{shop}」"));
    }
    // ⑤ 海外 / 代购
    if has(shop, &["海外", "全球购", "代购", "跨境", "免税", "进口"]) || badge(&["全球购", "海外直邮", "天猫国际", "京东国际", "跨境"]) || has(&title_l, &["代购", "海外直邮"]) {
        return k("overseas", "海外 / 跨境 / 代购渠道".into());
    }
    // ⑥ 小工作室 / 手作
    if has(shop, &["工作室", "手作", "手工", "原创设计", "设计师"]) || has(&shop_l, &["studio", "atelier", "handmade", "workshop"]) || p.id == "etsy" {
        return k("studio", if shop.is_empty() { format!("{} 多为独立手作店铺", p.name) } else { format!("店铺「{shop}」为工作室 / 手作") });
    }
    // ⑦ 个人卖家
    let private_badge = badge(&["privat", "private seller", "个人闲置", "privater"]);
    let business_badge = badge(&["gewerblich", "business seller", "企业"]);
    if private_badge && !business_badge {
        return k("individual", "卖家标注为个人 / 私人".into());
    }
    if p.kind == "c2c" && !business_badge && !has(shop, &["公司", "店", "商行", "GmbH", "Shop"]) {
        return k("individual", format!("{} 以个人卖家为主", p.name));
    }
    if i.condition == "used" && p.family.starts_with("ebay") && !business_badge && shop.is_empty() {
        return k("individual", "二手商品、卖家信息不足，按个人卖家对待".into());
    }
    // ⑧ 企业店
    if has(shop, &["企业店", "有限公司", "公司", "商行", "工厂", "厂家", "实业"]) || has(&shop_l, &["gmbh", " ltd", "co.,", " inc", " llc", "s.r.l", "corp"]) || badge(&["企业", "源头工厂", "超级工厂", "实力商家", "诚信通"]) || business_badge {
        return k("enterprise", if shop.is_empty() { "企业卖家".into() } else { format!("企业卖家「{shop}」") });
    }
    // ⑨ 天猫店铺（未写明类型）
    if badge(&["天猫"]) {
        return k("authorized", "天猫店铺（品牌授权入驻）".into());
    }
    // ⑩ 第三方卖家
    if !shop.is_empty() {
        return k("marketplace", format!("第三方店铺「{shop}」"));
    }
    if p.family.starts_with("amazon") && has(&seller, &["sold by", "verkauf durch", "verkäufer", "vendu par"]) {
        return k("marketplace", "Amazon 第三方卖家".into());
    }
    if ["marketplace", "wholesale"].contains(&p.kind.as_str()) && p.region == "intl" && ["temu", "aliexpress", "shein", "alibaba"].contains(&p.family.as_str()) {
        return k("marketplace", format!("{} 平台第三方商家", p.name));
    }
    k("unknown", "卡片上没有店铺信息".into())
}

/// 完整的店铺信息：类型 + 可信度 + 依据。
pub fn classify(p: &Platform, i: &ShopInput) -> ShopInfo {
    let (kind, why) = kind_of(p, i);
    let mut notes = vec![why];
    let mut cred = base_credibility(&kind);
    // 店铺评分（0–5）
    let rating = i.rating.or_else(|| {
        let s = i.scores?;
        let v: Vec<f64> = s.iter().filter(|(k, _)| *k != "好评率").map(|(_, v)| *v).filter(|v| *v > 0.0 && *v <= 5.0).collect();
        (!v.is_empty()).then(|| (v.iter().sum::<f64>() / v.len() as f64 * 100.0).round() / 100.0)
    });
    if let Some(r) = rating {
        if r >= 4.8 {
            cred += 6;
            notes.push(format!("店铺评分 {r:.1}"));
        } else if r >= 4.6 {
            cred += 2;
        } else if r < 4.4 {
            cred -= 10;
            notes.push(format!("店铺评分偏低（{r:.1}）"));
        }
    }
    if let Some(good) = i.scores.and_then(|s| s.get("好评率")).copied() {
        if good >= 99.0 {
            cred += 5;
            notes.push(format!("好评率 {good:.1}%"));
        } else if good < 95.0 {
            cred -= 8;
            notes.push(format!("好评率仅 {good:.1}%"));
        }
    }
    if let Some(y) = i.years {
        if y >= 5.0 {
            cred += 5;
            notes.push(format!("开店 {} 年", y.round() as i64));
        } else if y < 1.0 {
            cred -= 6;
            notes.push("新开店铺（不足 1 年）".into());
        }
    }
    if i.followers.is_some_and(|f| f >= 100_000) {
        cred += 3;
    }
    if i.sales.is_some_and(|s| s >= 10_000) {
        cred += 3;
        notes.push("销量很高".into());
    }
    let credibility = cred.clamp(5, 98) as u32;
    ShopInfo {
        name: i.shop.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        label: label_of(&kind).to_string(),
        kind,
        rating,
        scores: i.scores.cloned().unwrap_or_default(),
        years: i.years,
        followers: i.followers,
        credibility,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::BUILTIN;

    fn plat(id: &str) -> Platform {
        BUILTIN.iter().find(|p| p.id == id).unwrap().clone()
    }
    fn kind(id: &str, shop: Option<&str>, badges: &[&str], seller: Option<&str>) -> String {
        let b: Vec<String> = badges.iter().map(|s| s.to_string()).collect();
        kind_of(&plat(id), &ShopInput { shop, badges: &b, title: "x", condition: "new", seller_text: seller, ..Default::default() }).0
    }

    #[test]
    fn brand_names_containing_youpin_are_not_secondhand() {
        assert_eq!(kind("taobao", Some("名创优品官方旗舰店"), &[], None), "flagship");
        assert_eq!(kind("taobao", Some("数码优品回收店"), &[], None), "secondhand");
    }

    #[test]
    fn shop_kinds() {
        assert_eq!(kind("taobao", Some("索尼官方旗舰店"), &[], None), "flagship");
        assert_eq!(kind("jd", Some("索尼京东自营官方旗舰店"), &["自营"], None), "self_operated");
        assert_eq!(kind("jd", Some("某某数码专营店"), &[], None), "distributor");
        assert_eq!(kind("taobao", Some("索尼专卖店"), &["天猫"], None), "authorized");
        assert_eq!(kind("taobao", Some("小林手作工作室"), &[], None), "studio");
        assert_eq!(kind("taobao", Some("日本代购小铺"), &[], None), "overseas");
        assert_eq!(kind("taobao", Some("深圳市某某科技有限公司"), &[], None), "enterprise");
        assert_eq!(kind("taobao", Some("阿花的小店"), &[], None), "marketplace");
        assert_eq!(kind("xianyu", None, &[], None), "individual");
        assert_eq!(kind("ebay_de", Some("tech-shop24"), &["Privat"], None), "individual");
        assert_eq!(kind("amazon_de", None, &[], Some("Verkauf und Versand durch Amazon")), "self_operated");
        assert_eq!(kind("amazon_de", None, &[], Some("Verkauf durch: TechStore GmbH")), "marketplace");
        assert_eq!(kind("taobao", Some("二手数码回收"), &[], None), "secondhand");
        assert_eq!(kind("taobao", None, &[], None), "unknown");
    }

    #[test]
    fn credibility_moves_with_signals() {
        let p = plat("taobao");
        let mut scores = BTreeMap::new();
        scores.insert("描述".to_string(), 4.9);
        scores.insert("服务".to_string(), 4.9);
        let good = classify(&p, &ShopInput { shop: Some("索尼官方旗舰店"), badges: &[], title: "x", condition: "new", scores: Some(&scores), years: Some(8.0), ..Default::default() });
        assert_eq!(good.label, "旗舰店");
        assert!(good.credibility >= 95, "{good:?}");
        let bad = classify(&p, &ShopInput { shop: Some("阿花的小店"), badges: &[], title: "x", condition: "new", rating: Some(4.2), years: Some(0.5), ..Default::default() });
        assert!(bad.credibility < 45, "{bad:?}");
        assert!(bad.notes.iter().any(|n| n.contains("新开")));
    }
}
