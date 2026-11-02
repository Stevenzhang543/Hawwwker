//! 平台注册表：家族 → 版本（国内版 / 国际版），以及每个平台的搜索地址、结果链接规则、登录识别规则。
//! 国际站点规则依据各站公开网页结构编写，标为 beta 的可在「平台目录」直接改搜索模板。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{CustomPlatformInput, PlatformInfo, PlatformPatch};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    pub id: String,
    pub family: String,
    pub family_name: String,
    pub name: String,
    pub edition: String,
    pub region: String,
    pub country: String,
    pub kind: String,
    pub currency: String,
    pub locale: String,
    pub color: String,
    pub glyph: String,
    pub home: String,
    pub search: String,
    pub default_search: String,
    /// 详情链接正则（JS 语法），第 1 组为商品 ID；为空则只用卡片兜底识别
    pub link: String,
    /// v26.03 起每个平台都是独立账号（account_group = 平台 id），平台之间不再联动登录 / 退出。
    pub account_group: String,
    pub login: String,
    pub beta: bool,
    pub enabled: bool,
    pub custom: bool,
    pub note: String,
    pub cookie_domains: Vec<String>,
    /// 只有登录完成后才会出现的"强"Cookie。游客 Cookie、退出后仍保留的"记住昵称"类 Cookie（tracknick / lgc）、
    /// 游客也有但值不同的 Cookie（1688 `__cn_logon__=false`）都不列入。
    pub auth_cookies: Vec<String>,
    /// v26.03 账号页：未登录时会跳转到登录页。用于主动验证登录（隐藏标签打开，只读页面）。
    #[serde(default)]
    pub probe_url: String,
    /// 页面文字出现即视为登录墙（只在没识别到结果时判断）
    pub wall_text: Vec<String>,
    /// URL 包含即视为登录页
    pub wall_url: Vec<String>,
    pub keepalive: String,
    /// 只有商品 ID、没有链接时的详情地址模板（{id}）
    pub detail: String,
    /// 排序参数：(sort, 追加到搜索地址的片段)
    pub sort: Vec<(String, String)>,
}

impl Platform {
    pub fn info(&self) -> PlatformInfo {
        PlatformInfo {
            id: self.id.clone(),
            family: self.family.clone(),
            family_name: self.family_name.clone(),
            name: self.name.clone(),
            edition: self.edition.clone(),
            region: self.region.clone(),
            country: self.country.clone(),
            kind: self.kind.clone(),
            currency: self.currency.clone(),
            locale: self.locale.clone(),
            color: self.color.clone(),
            glyph: self.glyph.clone(),
            home: self.home.clone(),
            search_url: self.search.clone(),
            default_search_url: self.default_search.clone(),
            account_group: self.account_group.clone(),
            login: self.login.clone(),
            beta: self.beta,
            enabled: self.enabled,
            custom: self.custom,
            note: self.note.clone(),
        }
    }

    pub fn needs_login(&self) -> bool {
        self.login == "required"
    }

    /// 生成搜索地址。占位符：`{q}` 百分号编码；`{q+}` 空格为 +；`{q-}` 空格为 -（路径式）。
    pub fn search_url(&self, q: &str, sort: &str) -> String {
        let q = q.trim();
        let enc = urlencoding::encode(q).into_owned();
        let plus = enc.replace("%20", "+");
        let dash = urlencoding::encode(&q.split_whitespace().collect::<Vec<_>>().join("-")).into_owned();
        let mut url = self.search.replace("{q+}", &plus).replace("{q-}", &dash).replace("{q}", &enc);
        if let Some((_, suffix)) = self.sort.iter().find(|(k, _)| k == sort) {
            url.push_str(suffix);
        }
        url
    }

    pub fn detail_url(&self, id: &str) -> Option<String> {
        if self.detail.is_empty() || id.is_empty() || id.starts_with('t') && !id.chars().skip(1).all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(self.detail.replace("{id}", id))
    }

    pub fn host_matches(&self, url: &str) -> bool {
        let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").to_lowercase();
        self.cookie_domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }
}

#[allow(clippy::too_many_arguments)]
fn p(
    id: &str, family: &str, family_name: &str, name: &str, edition: &str, region: &str, country: &str, kind: &str,
    currency: &str, locale: &str, color: &str, glyph: &str, home: &str, search: &str, link: &str, group: &str,
    login: &str, beta: bool, enabled: bool, note: &str,
) -> Platform {
    Platform {
        id: id.into(),
        family: family.into(),
        family_name: family_name.into(),
        name: name.into(),
        edition: edition.into(),
        region: region.into(),
        country: country.into(),
        kind: kind.into(),
        currency: currency.into(),
        locale: locale.into(),
        color: color.into(),
        glyph: glyph.into(),
        home: home.into(),
        search: search.into(),
        default_search: search.into(),
        link: link.into(),
        // v26.03：平台之间不再共享登录，每个平台都是独立账号（保留参数只为兼容注册表写法）
        account_group: { let _ = group; id.into() },
        login: login.into(),
        beta,
        enabled,
        custom: false,
        note: note.into(),
        cookie_domains: vec![],
        auth_cookies: vec![],
        wall_text: vec![],
        wall_url: vec!["/login".into(), "passport.".into(), "/signin".into(), "/ap/signin".into()],
        keepalive: String::new(),
        detail: String::new(),
        sort: vec![],
        probe_url: String::new(),
    }
}

trait With {
    fn cookies(self, domains: &[&str], auth: &[&str]) -> Self;
    fn wall(self, text: &[&str]) -> Self;
    fn keep(self, url: &str) -> Self;
    fn detail(self, tpl: &str) -> Self;
    fn sorts(self, s: &[(&str, &str)]) -> Self;
    fn probe(self, url: &str) -> Self;
}

impl With for Platform {
    fn cookies(mut self, domains: &[&str], auth: &[&str]) -> Self {
        self.cookie_domains = domains.iter().map(|s| s.to_string()).collect();
        self.auth_cookies = auth.iter().map(|s| s.to_string()).collect();
        self
    }
    fn wall(mut self, text: &[&str]) -> Self {
        self.wall_text = text.iter().map(|s| s.to_string()).collect();
        self
    }
    fn keep(mut self, url: &str) -> Self {
        self.keepalive = url.into();
        self
    }
    fn detail(mut self, tpl: &str) -> Self {
        self.detail = tpl.into();
        self
    }
    fn sorts(mut self, s: &[(&str, &str)]) -> Self {
        self.sort = s.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        self
    }
    fn probe(mut self, url: &str) -> Self {
        self.probe_url = url.into();
        self
    }
}

const ALI_WALL: &[&str] = &["密码登录", "短信登录", "扫码登录", "亲，请登录", "请登录后"];

pub static BUILTIN: LazyLock<Vec<Platform>> = LazyLock::new(|| {
    vec![
        /* ---------------- 国内版 ---------------- */
        p("taobao", "taobao", "淘宝", "淘宝/天猫", "国内版", "cn", "CN", "marketplace", "CNY", "zh-CN", "#FF5000", "淘",
          "https://www.taobao.com/", "https://s.taobao.com/search?q={q}",
          r"(?:item\.taobao\.com/item\.htm|detail\.tmall\.com/item\.htm|a\.m\.taobao\.com/i)[^\s\x22']*?[?&]id=(\d+)",
          "taobao", "required", false, true, "")
            .cookies(&["taobao.com", "tmall.com"], &["unb", "cookie17"]).wall(ALI_WALL).keep("https://www.taobao.com/")
            .probe("https://i.taobao.com/my_taobao.htm")
            .detail("https://item.taobao.com/item.htm?id={id}")
            .sorts(&[("sales", "&sort=sale-desc"), ("price_asc", "&sort=price-asc"), ("price_desc", "&sort=price-desc")]),
        p("jd", "jd", "京东", "京东", "国内版", "cn", "CN", "retail", "CNY", "zh-CN", "#E1251B", "京",
          "https://www.jd.com/", "https://search.jd.com/Search?keyword={q}&enc=utf-8", r"item\.jd\.com/(\d+)\.html",
          "jd", "required", false, true, "")
            .cookies(&["jd.com"], &["thor", "pt_key"]).wall(&["欢迎登录", "扫码登录", "账户登录"]).keep("https://www.jd.com/")
            .probe("https://home.jd.com/")
            .detail("https://item.jd.com/{id}.html")
            .sorts(&[("sales", "&psort=3"), ("price_asc", "&psort=2"), ("price_desc", "&psort=1")]),
        p("pinduoduo", "pinduoduo", "拼多多", "拼多多", "国内版", "cn", "CN", "marketplace", "CNY", "zh-CN", "#E02E24", "拼",
          "https://mobile.yangkeduo.com/", "https://mobile.yangkeduo.com/search_result.html?search_key={q}",
          r"goods\d?\.html\?(?:[^\x22'#]*&)?goods_id=(\d+)", "pinduoduo", "required", false, true, "移动网页版，需手机号登录")
            .cookies(&["yangkeduo.com", "pinduoduo.com"], &["PDDAccessToken", "pdd_user_id"])
            .wall(&["手机登录", "登录查看", "请使用手机号登录"]).keep("https://mobile.yangkeduo.com/")
            .probe("https://mobile.yangkeduo.com/personal.html")
            .detail("https://mobile.yangkeduo.com/goods.html?goods_id={id}"),
        p("xianyu", "xianyu", "闲鱼", "闲鱼", "国内版", "cn", "CN", "c2c", "CNY", "zh-CN", "#FFB800", "闲",
          "https://www.goofish.com/", "https://www.goofish.com/search?q={q}",
          r"(?:goofish\.com/item\?(?:[^\x22'#]*&)?id=|2\.taobao\.com/item\.htm\?(?:[^\x22'#]*&)?id=)(\d+)",
          "xianyu", "required", false, true, "二手与转卖；独立登录（不再与淘宝联动）")
            .cookies(&["goofish.com"], &["unb", "cookie17"]).wall(&["密码登录", "短信登录", "扫码登录", "登录后可以更懂你", "请登录"])
            .keep("https://www.goofish.com/").probe("https://www.goofish.com/personal")
            .detail("https://www.goofish.com/item?id={id}"),
        p("alibaba1688", "1688", "1688", "1688", "国内版", "cn", "CN", "wholesale", "CNY", "zh-CN", "#FF6A00", "批",
          "https://www.1688.com/", "https://s.1688.com/selloffer/offer_search.htm?keywords={q}&charset=utf8",
          r"detail\.1688\.com/offer/(\d+)\.html", "alibaba1688", "required", false, false, "货源与批发价；独立登录")
            .cookies(&["1688.com"], &["__cn_logon_id__", "unb"]).wall(ALI_WALL).keep("https://www.1688.com/")
            .probe("https://work.1688.com/")
            .detail("https://detail.1688.com/offer/{id}.html"),
        p("xiaohongshu", "xiaohongshu", "小红书", "小红书", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#FF2442", "红",
          "https://www.xiaohongshu.com/explore",
          "https://www.xiaohongshu.com/search_result?keyword={q}&source=web_search_result_notes",
          r"xiaohongshu\.com/(?:explore|search_result|discovery/item)/([0-9a-f]{24})", "xiaohongshu", "required", false, true,
          "种草笔记与口碑；游客也有 web_session，以 Cookie 变化 + 主动验证为准")
            .cookies(&["xiaohongshu.com"], &["web_session"]).wall(&["登录后查看", "手机号登录", "登录探索更多", "登录后推荐更懂你的笔记", "马上登录即可"])
            .keep("https://www.xiaohongshu.com/explore").probe("https://www.xiaohongshu.com/explore"),
        p("douyin", "douyin", "抖音", "抖音", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#161823", "抖",
          "https://www.douyin.com/", "https://www.douyin.com/search/{q}?type=general", r"douyin\.com/(?:video|note)/(\d{10,})",
          "douyin", "required", false, true, "")
            .cookies(&["douyin.com"], &["sessionid", "sessionid_ss", "sid_guard", "uid_tt"]).wall(&["扫码登录", "验证码登录", "登录后即可"])
            .keep("https://www.douyin.com/").probe("https://www.douyin.com/user/self"),
        p("weibo", "weibo", "微博", "微博", "国内版", "cn", "CN", "social", "CNY", "zh-CN", "#E6162D", "博",
          "https://weibo.com/", "https://s.weibo.com/weibo?q={q}", r"weibo\.com/\d+/([A-Za-z0-9]{8,})", "weibo", "required",
          false, false, "公开讨论与求推荐")
            .cookies(&["weibo.com", "weibo.cn"], &["SSOLoginState", "ALF"]).wall(&["立即登录", "扫码登录", "登录后查看"]).keep("https://weibo.com/")
            .probe("https://account.weibo.com/set/index"),
        /* ---------------- 国际版（与国内版同家族） ---------------- */
        p("rednote", "xiaohongshu", "小红书", "RedNote", "国际版", "intl", "SG", "social", "USD", "en-US", "#FF2442", "R",
          "https://www.rednote.com/explore", "https://www.rednote.com/search_result?keyword={q}&source=web_search_result_notes",
          r"(?:rednote|xiaohongshu)\.com/(?:explore|search_result|discovery/item)/([0-9a-f]{24})", "rednote", "recommended",
          true, false, "小红书国际版（rednote.com）")
            .cookies(&["rednote.com"], &["web_session"]).wall(&["Log in", "Sign up", "登录"]).keep("https://www.rednote.com/explore")
            .probe("https://www.rednote.com/explore"),
        p("tiktok", "douyin", "抖音", "TikTok", "国际版", "intl", "US", "social", "USD", "en-US", "#000000", "T",
          "https://www.tiktok.com/", "https://www.tiktok.com/search?q={q}", r"tiktok\.com/@[^/\x22']+/video/(\d{10,})",
          "tiktok", "recommended", true, false, "抖音国际版")
            .cookies(&["tiktok.com"], &["sessionid", "sid_tt", "sid_guard"]).keep("https://www.tiktok.com/")
            .probe("https://www.tiktok.com/setting"),
        p("temu", "pinduoduo", "拼多多", "Temu", "国际版", "intl", "US", "marketplace", "USD", "en-US", "#FB7701", "T",
          "https://www.temu.com/", "https://www.temu.com/search_result.html?search_key={q}",
          r"temu\.com/[^\x22'?#]*?-g-(\d{6,})\.html", "temu", "optional", true, true, "拼多多国际版；币种以页面显示为准")
            .cookies(&["temu.com"], &[]),
        p("aliexpress", "taobao", "淘宝", "AliExpress", "国际版", "intl", "CN", "marketplace", "USD", "en-US", "#E62E04", "A",
          "https://www.aliexpress.com/", "https://www.aliexpress.com/wholesale?SearchText={q+}",
          r"aliexpress\.[a-z.]+/item/(\d{6,})\.html", "aliexpress", "optional", true, true, "阿里巴巴国际零售；币种以页面显示为准")
            .cookies(&["aliexpress.com", "aliexpress.us"], &[]).detail("https://www.aliexpress.com/item/{id}.html"),
        p("joybuy", "jd", "京东", "Joybuy", "国际版", "intl", "DE", "retail", "EUR", "de-DE", "#E1251B", "J",
          "https://www.joybuy.com/", "https://www.joybuy.com/search?keyword={q}",
          r"joybuy\.[a-z.]+/(?:[^\x22'?#]*/)?(?:dp|item|product|p)/[^\x22'?#]*?(\d{6,})", "joybuy", "optional", true, false,
          "京东欧洲站（2025-26 上线），地址如有变化请在平台目录修改")
            .cookies(&["joybuy.com", "joybuy.de"], &[]),
        p("alibaba", "1688", "1688", "Alibaba.com", "国际版", "intl", "CN", "wholesale", "USD", "en-US", "#FF6A00", "A",
          "https://www.alibaba.com/", "https://www.alibaba.com/trade/search?SearchText={q+}",
          r"alibaba\.com/product-detail/[^\x22'?#]*?_(\d{6,})\.html", "alibaba_intl", "optional", true, false, "国际批发与工厂")
            .cookies(&["alibaba.com"], &[]),
        /* ---------------- 国际平台 ---------------- */
        p("amazon_com", "amazon", "Amazon", "Amazon", "美国站", "intl", "US", "retail", "USD", "en-US", "#FF9900", "a",
          "https://www.amazon.com/", "https://www.amazon.com/s?k={q+}",
          r"amazon\.com/(?:[^\x22'?#]*/)?(?:dp|gp/product)/([A-Z0-9]{10})", "amazon_com", "optional", false, true, "")
            .cookies(&["amazon.com"], &["at-main", "sess-at-main"]).probe("https://www.amazon.com/gp/css/homepage.html").keep("https://www.amazon.com/")
            .detail("https://www.amazon.com/dp/{id}")
            .sorts(&[("price_asc", "&s=price-asc-rank"), ("price_desc", "&s=price-desc-rank"), ("rating", "&s=review-rank"), ("recent", "&s=date-desc-rank")]),
        p("amazon_de", "amazon", "Amazon", "Amazon", "德国站", "intl", "DE", "retail", "EUR", "de-DE", "#FF9900", "a",
          "https://www.amazon.de/", "https://www.amazon.de/s?k={q+}",
          r"amazon\.de/(?:[^\x22'?#]*/)?(?:dp|gp/product)/([A-Z0-9]{10})", "amazon_de", "optional", false, true, "")
            .cookies(&["amazon.de"], &["at-acbde", "sess-at-acbde"]).probe("https://www.amazon.de/gp/css/homepage.html").keep("https://www.amazon.de/")
            .detail("https://www.amazon.de/dp/{id}")
            .sorts(&[("price_asc", "&s=price-asc-rank"), ("price_desc", "&s=price-desc-rank"), ("rating", "&s=review-rank"), ("recent", "&s=date-desc-rank")]),
        p("amazon_co_uk", "amazon", "Amazon", "Amazon", "英国站", "intl", "GB", "retail", "GBP", "en-GB", "#FF9900", "a",
          "https://www.amazon.co.uk/", "https://www.amazon.co.uk/s?k={q+}",
          r"amazon\.co\.uk/(?:[^\x22'?#]*/)?(?:dp|gp/product)/([A-Z0-9]{10})", "amazon_co_uk", "optional", false, false, "")
            .cookies(&["amazon.co.uk"], &["at-acbuk", "sess-at-acbuk"]).probe("https://www.amazon.co.uk/gp/css/homepage.html").detail("https://www.amazon.co.uk/dp/{id}")
            .sorts(&[("price_asc", "&s=price-asc-rank"), ("price_desc", "&s=price-desc-rank")]),
        p("amazon_co_jp", "amazon", "Amazon", "Amazon", "日本站", "intl", "JP", "retail", "JPY", "ja-JP", "#FF9900", "a",
          "https://www.amazon.co.jp/", "https://www.amazon.co.jp/s?k={q+}",
          r"amazon\.co\.jp/(?:[^\x22'?#]*/)?(?:dp|gp/product)/([A-Z0-9]{10})", "amazon_co_jp", "optional", false, false, "")
            .cookies(&["amazon.co.jp"], &["at-acbjp", "sess-at-acbjp"]).probe("https://www.amazon.co.jp/gp/css/homepage.html").detail("https://www.amazon.co.jp/dp/{id}")
            .sorts(&[("price_asc", "&s=price-asc-rank"), ("price_desc", "&s=price-desc-rank")]),
        p("amazon_fr", "amazon", "Amazon", "Amazon", "法国站", "intl", "FR", "retail", "EUR", "fr-FR", "#FF9900", "a",
          "https://www.amazon.fr/", "https://www.amazon.fr/s?k={q+}",
          r"amazon\.fr/(?:[^\x22'?#]*/)?(?:dp|gp/product)/([A-Z0-9]{10})", "amazon_fr", "optional", false, false, "")
            .cookies(&["amazon.fr"], &["at-acbfr", "sess-at-acbfr"]).probe("https://www.amazon.fr/gp/css/homepage.html").detail("https://www.amazon.fr/dp/{id}")
            .sorts(&[("price_asc", "&s=price-asc-rank"), ("price_desc", "&s=price-desc-rank")]),
        p("ebay_com", "ebay", "eBay", "eBay", "美国站", "intl", "US", "marketplace", "USD", "en-US", "#0064D2", "e",
          "https://www.ebay.com/", "https://www.ebay.com/sch/i.html?_nkw={q+}",
          r"ebay\.com/itm/(?:[^\x22'?#/]*/)?(\d{9,})", "ebay_com", "optional", false, true, "")
            .cookies(&["ebay.com"], &[]).detail("https://www.ebay.com/itm/{id}")
            .sorts(&[("price_asc", "&_sop=15"), ("price_desc", "&_sop=16"), ("recent", "&_sop=10")]),
        p("ebay_de", "ebay", "eBay", "eBay", "德国站", "intl", "DE", "marketplace", "EUR", "de-DE", "#0064D2", "e",
          "https://www.ebay.de/", "https://www.ebay.de/sch/i.html?_nkw={q+}",
          r"ebay\.de/itm/(?:[^\x22'?#/]*/)?(\d{9,})", "ebay_de", "optional", false, true, "")
            .cookies(&["ebay.de"], &[]).detail("https://www.ebay.de/itm/{id}")
            .sorts(&[("price_asc", "&_sop=15"), ("price_desc", "&_sop=16"), ("recent", "&_sop=10")]),
        p("ebay_co_uk", "ebay", "eBay", "eBay", "英国站", "intl", "GB", "marketplace", "GBP", "en-GB", "#0064D2", "e",
          "https://www.ebay.co.uk/", "https://www.ebay.co.uk/sch/i.html?_nkw={q+}",
          r"ebay\.co\.uk/itm/(?:[^\x22'?#/]*/)?(\d{9,})", "ebay_co_uk", "optional", false, false, "")
            .cookies(&["ebay.co.uk"], &[]).detail("https://www.ebay.co.uk/itm/{id}")
            .sorts(&[("price_asc", "&_sop=15"), ("price_desc", "&_sop=16")]),
        p("idealo", "idealo", "idealo", "idealo", "德国比价", "intl", "DE", "compare", "EUR", "de-DE", "#0A3761", "i",
          "https://www.idealo.de/", "https://www.idealo.de/preisvergleich/MainSearchProductCategory.html?q={q+}",
          r"idealo\.de/preisvergleich/OffersOfProduct/(\d+)", "idealo", "optional", true, true, "德国比价网站，聚合多家商店报价")
            .cookies(&["idealo.de"], &[]),
        p("kleinanzeigen", "kleinanzeigen", "Kleinanzeigen", "Kleinanzeigen", "德国二手", "intl", "DE", "c2c", "EUR", "de-DE",
          "#1D4B00", "K", "https://www.kleinanzeigen.de/", "https://www.kleinanzeigen.de/s-suchanfrage.html?keywords={q+}",
          r"kleinanzeigen\.de/s-anzeige/[^\x22'?#]*?/(\d{6,})-", "kleinanzeigen", "optional", true, false, "德国本地二手（闲鱼类）")
            .cookies(&["kleinanzeigen.de"], &[]),
        p("otto", "otto", "OTTO", "OTTO", "德国站", "intl", "DE", "retail", "EUR", "de-DE", "#F00020", "O",
          "https://www.otto.de/", "https://www.otto.de/suche/{q}/", r"otto\.de/p/([^\x22'?#/]+)", "otto", "optional", true,
          false, "").cookies(&["otto.de"], &[]),
        p("etsy", "etsy", "Etsy", "Etsy", "国际站", "intl", "US", "marketplace", "USD", "en-US", "#F1641E", "E",
          "https://www.etsy.com/", "https://www.etsy.com/search?q={q+}", r"etsy\.com/(?:[a-z-]+/)?listing/(\d+)", "etsy",
          "optional", true, false, "手工与原创商品").cookies(&["etsy.com"], &[]),
        p("walmart", "walmart", "Walmart", "Walmart", "美国站", "intl", "US", "retail", "USD", "en-US", "#0071DC", "W",
          "https://www.walmart.com/", "https://www.walmart.com/search?q={q+}", r"walmart\.com/ip/(?:[^\x22'?#]*/)?(\d{5,})",
          "walmart", "optional", true, false, "").cookies(&["walmart.com"], &[]).detail("https://www.walmart.com/ip/{id}"),
        p("bestbuy", "bestbuy", "Best Buy", "Best Buy", "美国站", "intl", "US", "retail", "USD", "en-US", "#0046BE", "B",
          "https://www.bestbuy.com/", "https://www.bestbuy.com/site/searchpage.jsp?st={q+}",
          r"bestbuy\.com/site/[^\x22'?#]*?/(\d{7})\.p", "bestbuy", "optional", true, false, "").cookies(&["bestbuy.com"], &[]),
        p("shein", "shein", "SHEIN", "SHEIN", "国际站", "intl", "US", "marketplace", "USD", "en-US", "#222222", "S",
          "https://www.shein.com/", "https://www.shein.com/pdsearch/{q-}/", r"shein\.com/[^\x22'?#]*?-p-(\d+)(?:-cat-\d+)?\.html",
          "shein", "optional", true, false, "").cookies(&["shein.com"], &[]),
        p("mercari_jp", "mercari", "Mercari", "メルカリ", "日本二手", "intl", "JP", "c2c", "JPY", "ja-JP", "#FF0211", "M",
          "https://jp.mercari.com/", "https://jp.mercari.com/search?keyword={q}", r"mercari\.com/item/(m\d+)", "mercari",
          "optional", true, false, "").cookies(&["mercari.com"], &[]).detail("https://jp.mercari.com/item/{id}"),
        p("rakuten", "rakuten", "Rakuten", "楽天市場", "日本站", "intl", "JP", "marketplace", "JPY", "ja-JP", "#BF0000", "楽",
          "https://www.rakuten.co.jp/", "https://search.rakuten.co.jp/search/mall/{q}/",
          r"item\.rakuten\.co\.jp/([^\x22'?#]+?/[^\x22'?#/]+)/?", "rakuten", "optional", true, false, "")
            .cookies(&["rakuten.co.jp"], &[]),
        p("reddit", "reddit", "Reddit", "Reddit", "国际社区", "intl", "US", "social", "USD", "en-US", "#FF4500", "r",
          "https://www.reddit.com/", "https://www.reddit.com/search/?q={q}", r"reddit\.com/r/[^/\x22']+/comments/([a-z0-9]+)",
          "reddit", "optional", false, false, "海外口碑与讨论（无价格）").cookies(&["reddit.com"], &[]),
    ]
});

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Override {
    enabled: Option<bool>,
    search_url: Option<String>,
    custom: Option<Platform>,
}

/// 读取全部平台（内置 + 覆盖 + 自定义）。
pub fn all(db: &Db) -> Vec<Platform> {
    let overrides: Vec<(String, Override)> = {
        let c = db.conn();
        let mut stmt = match c.prepare("SELECT key, value FROM platform_overrides") {
            Ok(s) => s,
            Err(_) => return BUILTIN.clone(),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map(|rows| {
                rows.filter_map(|r| r.ok())
                    .filter_map(|(id, v)| serde_json::from_str::<Override>(&v).ok().map(|o| (id, o)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut out: Vec<Platform> = BUILTIN.clone();
    for p in out.iter_mut() {
        if let Some((_, o)) = overrides.iter().find(|(id, _)| id == &p.id) {
            if let Some(e) = o.enabled {
                p.enabled = e;
            }
            if let Some(s) = o.search_url.as_ref().filter(|s| !s.trim().is_empty()) {
                p.search = s.clone();
            }
        }
    }
    for (_, o) in overrides.iter() {
        if let Some(mut c) = o.custom.clone() {
            if let Some(e) = o.enabled {
                c.enabled = e;
            }
            out.push(c);
        }
    }
    out
}

pub fn get(db: &Db, id: &str) -> Option<Platform> {
    all(db).into_iter().find(|p| p.id == id)
}

pub fn infos(db: &Db) -> Vec<PlatformInfo> {
    all(db).iter().map(Platform::info).collect()
}

fn load_override(db: &Db, id: &str) -> Override {
    db.get_json::<Override>("platform_overrides", id).unwrap_or_default()
}

pub fn update(db: &Db, id: &str, patch: PlatformPatch) -> AppResult<PlatformInfo> {
    let mut o = load_override(db, id);
    if get(db, id).is_none() {
        return Err(AppError::not_found("平台"));
    }
    if let Some(e) = patch.enabled {
        o.enabled = Some(e);
    }
    if let Some(s) = patch.search_url {
        let s = s.trim().to_string();
        if !s.is_empty() {
            validate_template(&s)?;
        }
        if let Some(c) = o.custom.as_mut() {
            if !s.is_empty() {
                c.search = s.clone();
            }
        } else {
            o.search_url = if s.is_empty() { None } else { Some(s) };
        }
    }
    db.put_json("platform_overrides", id, &o)?;
    get(db, id).map(|p| p.info()).ok_or_else(|| AppError::not_found("平台"))
}

fn validate_template(s: &str) -> AppResult<()> {
    if !(s.starts_with("https://") || s.starts_with("http://")) {
        return Err(AppError::invalid("搜索地址必须以 http:// 或 https:// 开头"));
    }
    if !(s.contains("{q}") || s.contains("{q+}") || s.contains("{q-}")) {
        return Err(AppError::invalid("搜索地址需要包含关键词占位符 {q}"));
    }
    Ok(())
}

pub fn add_custom(db: &Db, input: CustomPlatformInput) -> AppResult<PlatformInfo> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("请填写平台名称"));
    }
    validate_template(input.search_url.trim())?;
    if let Some(re) = input.link_pattern.as_ref().filter(|s| !s.trim().is_empty()) {
        regex::Regex::new(re).map_err(|e| AppError::invalid(format!("商品链接正则无效：{e}")))?;
    }
    let host = input.search_url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("").to_lowercase();
    let base_domain = host.trim_start_matches("www.").to_string();
    let slug: String = base_domain.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let id = format!("custom_{}", if slug.is_empty() { uuid::Uuid::new_v4().simple().to_string()[..8].to_string() } else { slug });
    if get(db, &id).is_some() {
        return Err(AppError::invalid("这个网站已经添加过了"));
    }
    let glyph: String = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let mut plat = p(
        &id, &id, name, name, "自定义", if input.region == "cn" { "cn" } else { "intl" }, "",
        input.kind.as_deref().unwrap_or("retail"), &input.currency.to_uppercase(), &input.locale, "#6E7BFF", &glyph,
        input.home.as_deref().unwrap_or(&format!("https://{host}/")), input.search_url.trim(),
        input.link_pattern.as_deref().unwrap_or(""), &id, "optional", true, true, "自定义平台",
    );
    plat.custom = true;
    plat.cookie_domains = vec![base_domain];
    let o = Override { enabled: Some(true), search_url: None, custom: Some(plat) };
    db.put_json("platform_overrides", &id, &o)?;
    get(db, &id).map(|p| p.info()).ok_or_else(|| AppError::internal("保存失败"))
}

pub fn remove_custom(db: &Db, id: &str) -> AppResult<()> {
    if !id.starts_with("custom_") {
        return Err(AppError::invalid("内置平台不能删除，可以在平台目录中停用"));
    }
    db.conn().execute("DELETE FROM platform_overrides WHERE key=?1", [id])?;
    Ok(())
}

/// 指令中的平台别名（规则预解析用）。
pub fn aliases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("淘宝", "taobao"), ("天猫", "taobao"), ("京东", "jd"), ("拼多多", "pinduoduo"), ("pdd", "pinduoduo"),
        ("闲鱼", "xianyu"), ("1688", "alibaba1688"), ("阿里巴巴", "alibaba1688"), ("小红书", "xiaohongshu"),
        ("红书", "xiaohongshu"), ("抖音", "douyin"), ("微博", "weibo"), ("rednote", "rednote"), ("tiktok", "tiktok"),
        ("temu", "temu"), ("aliexpress", "aliexpress"), ("速卖通", "aliexpress"), ("joybuy", "joybuy"),
        ("alibaba.com", "alibaba"), ("亚马逊美国", "amazon_com"), ("amazon.com", "amazon_com"), ("美亚", "amazon_com"),
        ("亚马逊德国", "amazon_de"), ("德亚", "amazon_de"), ("amazon.de", "amazon_de"), ("亚马逊英国", "amazon_co_uk"),
        ("英亚", "amazon_co_uk"), ("amazon.co.uk", "amazon_co_uk"), ("亚马逊日本", "amazon_co_jp"), ("日亚", "amazon_co_jp"),
        ("amazon.co.jp", "amazon_co_jp"), ("亚马逊法国", "amazon_fr"), ("法亚", "amazon_fr"), ("ebay德国", "ebay_de"),
        ("ebay.de", "ebay_de"), ("ebay英国", "ebay_co_uk"), ("ebay美国", "ebay_com"), ("ebay.com", "ebay_com"),
        ("idealo", "idealo"), ("kleinanzeigen", "kleinanzeigen"), ("otto", "otto"), ("etsy", "etsy"),
        ("walmart", "walmart"), ("沃尔玛", "walmart"), ("best buy", "bestbuy"), ("bestbuy", "bestbuy"), ("shein", "shein"),
        ("希音", "shein"), ("mercari", "mercari_jp"), ("煤炉", "mercari_jp"), ("乐天", "rakuten"), ("rakuten", "rakuten"),
        ("reddit", "reddit"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_link_patterns_compile_and_ids_unique() {
        let mut ids = std::collections::HashSet::new();
        for p in BUILTIN.iter() {
            assert!(ids.insert(p.id.clone()), "duplicate id {}", p.id);
            if !p.link.is_empty() {
                // JS 正则里的 \x22 在 Rust regex 中同样合法
                regex::Regex::new(&p.link).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            }
            assert!(p.search.contains("{q"), "{} search template", p.id);
        }
    }

    #[test]
    fn every_platform_is_its_own_account() {
        for p in BUILTIN.iter() {
            assert_eq!(p.account_group, p.id, "{} must not share login with other platforms", p.id);
            if p.login == "required" {
                assert!(!p.auth_cookies.is_empty(), "{} needs strong auth cookies", p.id);
                assert!(!p.probe_url.is_empty(), "{} needs a probe url", p.id);
            }
        }
        // 闲鱼与淘宝不再共用 Cookie 域名
        let tb = BUILTIN.iter().find(|p| p.id == "taobao").unwrap();
        let xy = BUILTIN.iter().find(|p| p.id == "xianyu").unwrap();
        assert!(xy.cookie_domains.iter().all(|d| !tb.cookie_domains.contains(d)));
    }

    #[test]
    fn search_url_encoding() {
        let amazon = BUILTIN.iter().find(|p| p.id == "amazon_de").unwrap();
        assert_eq!(amazon.search_url("Sony WH-1000XM5", "price_asc"), "https://www.amazon.de/s?k=Sony+WH-1000XM5&s=price-asc-rank");
        let tb = BUILTIN.iter().find(|p| p.id == "taobao").unwrap();
        assert_eq!(tb.search_url("咖啡机", "relevance"), "https://s.taobao.com/search?q=%E5%92%96%E5%95%A1%E6%9C%BA");
        let shein = BUILTIN.iter().find(|p| p.id == "shein").unwrap();
        assert_eq!(shein.search_url("summer dress", ""), "https://www.shein.com/pdsearch/summer-dress/");
    }

    #[test]
    fn link_patterns_match_samples() {
        let cases = [
            ("amazon_de", "https://www.amazon.de/Sony-WH-1000XM5-Kopfh%C3%B6rer/dp/B09Y2MYL5C/ref=sr_1_1", "B09Y2MYL5C"),
            ("ebay_de", "https://www.ebay.de/itm/256123456789?hash=item", "256123456789"),
            ("taobao", "https://item.taobao.com/item.htm?spm=a21n57&id=712345678901&ns=1", "712345678901"),
            ("jd", "https://item.jd.com/100012345678.html", "100012345678"),
            ("temu", "https://www.temu.com/de/bluetooth-kopfhoerer-g-601099512345678.html", "601099512345678"),
            ("aliexpress", "https://de.aliexpress.com/item/1005006123456789.html?spm=x", "1005006123456789"),
            ("kleinanzeigen", "https://www.kleinanzeigen.de/s-anzeige/sony-wh-1000xm5/2876543210-172-3331", "2876543210"),
            ("xiaohongshu", "https://www.xiaohongshu.com/explore/64a1b2c3d4e5f6a7b8c9d0e1?xsec=1", "64a1b2c3d4e5f6a7b8c9d0e1"),
        ];
        for (id, url, want) in cases {
            let p = BUILTIN.iter().find(|p| p.id == id).unwrap();
            let re = regex::Regex::new(&p.link).unwrap();
            let got = re.captures(url).and_then(|c| c.get(1)).map(|m| m.as_str().to_string());
            assert_eq!(got.as_deref(), Some(want), "{id}");
        }
    }

    #[test]
    fn custom_platform_roundtrip() {
        let db = Db::memory().unwrap();
        let info = add_custom(&db, CustomPlatformInput {
            name: "Geizhals".into(), region: "intl".into(), currency: "eur".into(), locale: "de-AT".into(),
            search_url: "https://geizhals.at/?fs={q+}".into(), link_pattern: None, home: None, kind: None,
        }).unwrap();
        assert_eq!(info.id, "custom_geizhals_at");
        assert!(info.custom && info.enabled);
        update(&db, &info.id, PlatformPatch { enabled: Some(false), search_url: None }).unwrap();
        assert!(!get(&db, &info.id).unwrap().enabled);
        remove_custom(&db, &info.id).unwrap();
        assert!(get(&db, &info.id).is_none());
        assert!(update(&db, "taobao", PlatformPatch { enabled: None, search_url: Some("ftp://x".into()) }).is_err());
    }
}
