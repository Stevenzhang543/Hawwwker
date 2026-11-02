//! 在用户已登录的浏览器里执行平台搜索：打开搜索页 → 等待结果 / 识别登录墙与验证 → 自适应滚动 → 读取卡片。
//! 以及商品详情页读价（盯价用）。

use super::manager::{Browser, Tab, TabGuard};
use std::sync::Arc;
use crate::platforms::Platform;
use crate::search::parse::{parse_card, Candidate, RawCard};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// 结果卡片抽取脚本：按"详情链接"定位卡片；无链接时按"商品图 + 价格"兜底。
/// 每张卡片返回若干"文字块"及其字号、字重、是否链接文字、是否删除线，供 Rust 判断标题与价格。
pub const EXTRACT_JS: &str = r#"
((pattern, hint) => {
  let re = null; try { re = pattern ? new RegExp(pattern) : null; } catch (e) { re = null; }
  const INLINE = new Set(['A','SPAN','B','I','EM','STRONG','SMALL','SUP','SUB','FONT','MARK','S','DEL','INS','U','BDI','LABEL','ABBR','TIME','DATA','BR','WBR','IMG','SVG','PICTURE']);
  const seen = new Set(); const out = [];
  const lineOf = (el, card) => {
    let fs = 0, fw = 0;
    for (const n of [el, ...el.querySelectorAll('*')].slice(0, 12)) {
      const cs = getComputedStyle(n); const f = parseFloat(cs.fontSize) || 0;
      if (f > fs) { fs = f; fw = parseFloat(cs.fontWeight) || 400; }
    }
    let s = false; let a = false;
    for (let p = el; p && p !== card.parentElement; p = p.parentElement) {
      const tag = p.tagName;
      if (tag === 'S' || tag === 'DEL' || tag === 'STRIKE') s = true;
      else if (!s && getComputedStyle(p).textDecorationLine.includes('line-through')) s = true;
      if (tag === 'A') a = true;
    }
    return { t: (el.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 220), fs, fw, a, s };
  };
  const blocks = (card) => {
    const res = []; const taken = new Set();
    for (const el of [card, ...card.querySelectorAll('*')]) {
      if (res.length >= 40) break;
      if (taken.has(el) || !el.innerText || !el.innerText.trim()) continue;
      let inlineOnly = true;
      for (const c of el.children) {
        const d = getComputedStyle(c).display;
        if (!(INLINE.has(c.tagName) && (d.startsWith('inline') || d === 'contents' || d === 'none'))) { inlineOnly = false; break; }
      }
      if (!inlineOnly) continue;
      for (const d of el.querySelectorAll('*')) taken.add(d);
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const ln = lineOf(el, card); if (ln.t) res.push(ln);
    }
    return res;
  };
  const titleOf = (a, card) => {
    const t = (a.getAttribute('title') || a.getAttribute('aria-label') || '').trim();
    if (t.length >= 6) return t;
    const img = card.querySelector('img[alt]'); const alt = img ? (img.getAttribute('alt') || '').trim() : '';
    return alt.length >= 8 ? alt : '';
  };
  const imgOf = (card) => { const img = card.querySelector('img'); return img ? (img.currentSrc || img.src || img.getAttribute('data-src') || '') : ''; };
  const page = () => ({ url: location.href, title: document.title, body: (document.body ? document.body.innerText : '').slice(0, 3000), ready: document.readyState });
  // v26.03 ① 平台提示选择器：结构化读取标题 / 价格 / 店铺 / 销量 / 徽标
  const qs = (el, sel) => { if (!sel) return null; try { return el.querySelector(sel); } catch (e) { return null; } };
  const qsa = (el, sel) => { if (!sel) return []; try { return [...el.querySelectorAll(sel)]; } catch (e) { return []; } };
  const tx = (el) => {
    if (!el) return '';
    let t = (el.innerText || '').replace(/\s+/g, ' ').trim();
    if (!t) t = (el.textContent || '').replace(/\s+/g, ' ').trim();
    if (!t) t = (el.getAttribute('aria-label') || el.getAttribute('title') || el.getAttribute('alt') || '').trim();
    return t.slice(0, 220);
  };
  if (hint && hint.card) {
    const hinted = []; const hseen = new Set();
    for (const c of qsa(document, hint.card)) {
      if (hinted.length >= 80) break;
      if (!(c.innerText || '').trim()) continue;
      if (hinted.some((o) => o.el.contains(c))) continue;
      const a = qs(c, hint.link || 'a[href]') || (c.tagName === 'A' ? c : c.closest('a[href]'));
      const href = a ? a.href : '';
      let id = hint.idAttr ? (c.getAttribute(hint.idAttr) || '') : '';
      if (!id && re && href) { const m = href.match(re); if (m) id = m[1] || ''; }
      const key = id || href || (c.innerText || '').slice(0, 80);
      if (hseen.has(key)) continue; hseen.add(key);
      const h = { title: tx(qs(c, hint.title)), price: tx(qs(c, hint.price)), orig: tx(qs(c, hint.orig)), shop: tx(qs(c, hint.shop)),
        sales: tx(qs(c, hint.sales)), rating: tx(qs(c, hint.rating)), reviews: tx(qs(c, hint.reviews)), location: tx(qs(c, hint.location)),
        ship: tx(qs(c, hint.ship)), badges: qsa(c, hint.badges).map(tx).filter((t) => t && t.length <= 18).slice(0, 8), currency: '' };
      if (!h.title && !h.price) continue;
      hinted.push({ el: c, item: { id, href, img: imgOf(c), titleAttr: a ? titleOf(a, c) : '', nolink: !a, lines: blocks(c), h } });
    }
    if (hinted.length >= 3) return { items: hinted.map((x) => x.item), mode: 'hint', ...page() };
  }
  // ② JSON-LD ItemList（部分站点在页面里给出结构化结果）
  const ld = [];
  for (const sc of document.querySelectorAll('script[type="application/ld+json"]')) {
    try {
      const d = JSON.parse(sc.textContent); const arr = Array.isArray(d) ? d : (d['@graph'] || [d]);
      for (const x of arr) {
        if (![].concat(x['@type'] || []).includes('ItemList')) continue;
        for (const el of (x.itemListElement || [])) {
          const it = el.item || el; if (!it || !it.name) continue;
          let o = it.offers; if (Array.isArray(o)) o = o[0]; o = o || {};
          const url = it.url || el.url || ''; const price = o.price ?? o.lowPrice;
          let id = ''; if (re && url) { const m = String(url).match(re); if (m) id = m[1] || ''; }
          const ar = it.aggregateRating || {};
          ld.push({ id, href: url, img: [].concat(it.image || [])[0] || '', titleAttr: it.name, nolink: !url, lines: [],
            h: { title: String(it.name), price: price != null ? String(price) : '', orig: '', shop: (it.brand && it.brand.name) ? '' : '', sales: '',
                 rating: ar.ratingValue != null ? String(ar.ratingValue) : '', reviews: ar.reviewCount != null ? String(ar.reviewCount) : (ar.ratingCount != null ? String(ar.ratingCount) : ''),
                 location: '', ship: '', badges: [], currency: o.priceCurrency || '' } });
        }
      }
    } catch (e) {}
  }
  if (re) {
    for (const a of document.querySelectorAll('a[href]')) {
      const href = a.href || ''; const m = href.match(re); if (!m) continue;
      const id = m[1] || href; if (seen.has(id)) continue;
      let card = a;
      for (let i = 0; i < 9 && card.parentElement; i++) {
        const p = card.parentElement; const ids = new Set();
        for (const b of p.querySelectorAll('a[href]')) { const mm = (b.href || '').match(re); if (mm) ids.add(mm[1] || b.href); if (ids.size > 1) break; }
        if (ids.size > 1 || (p.innerText || '').length > 900) break;
        card = p;
      }
      if ((card.innerText || '').trim().length < 4) continue;
      seen.add(id);
      out.push({ id, href, img: imgOf(card), titleAttr: titleOf(a, card), nolink: false, lines: blocks(card) });
      if (out.length >= 80) break;
    }
  }
  let mode = 'link';
  if (out.length < 3 && ld.length >= 3) return { items: ld.slice(0, 80), mode: 'ld', ...page() };
  if (out.length === 0) {
    mode = 'card';
    const priceRe = /[¥￥$€£]\s*\d|\d+(?:[.,]\d+)?\s*(?:元|€|円)|已拼|人付款|已售|sold|verkauft/i;
    const big = (i) => (i.naturalWidth || i.width || 0) >= 60 || i.getBoundingClientRect().width >= 60;
    const cands = new Set();
    for (const img of document.querySelectorAll('img')) {
      if (!big(img)) continue;
      let el = img;
      for (let k = 0; k < 8 && el.parentElement; k++) { el = el.parentElement; const t = el.innerText || ''; if (t.length >= 8 && priceRe.test(t)) break; }
      const t = (el.innerText || '').trim();
      if (t.length < 8 || t.length > 900 || !priceRe.test(t)) continue;
      if ([...el.querySelectorAll('img')].filter(big).length > 3) continue;
      cands.add(el);
    }
    const cards = [...cands].filter((c) => ![...cands].some((o) => o !== c && c.contains(o)));
    const idRe = /(?:goods_?id|item_?id|offer_?id|sku_?id|product_?id|data-id)[^0-9a-z]{0,6}([0-9]{6,}|[A-Z0-9]{10})/i;
    for (const c of cards.slice(0, 80)) {
      const a = c.closest('a[href]') || c.querySelector('a[href]');
      let id = '';
      for (const n of [c, ...c.querySelectorAll('*')].slice(0, 80)) {
        for (const at of (n.getAttributeNames ? n.getAttributeNames() : [])) {
          const v = n.getAttribute(at) || ''; const m = idRe.exec(at + '=' + v) || (/^data-/.test(at) && /^(\d{8,})$/.exec(v));
          if (m) { id = m[1]; break; }
        }
        if (id) break;
      }
      if (!id && a && re) { const m = (a.href || '').match(re); if (m) id = m[1]; }
      const key = id || ('h' + (c.innerText || '').split('\n').slice(0, 3).join('|').slice(0, 80));
      if (seen.has(key)) continue; seen.add(key);
      out.push({ id: id, href: a ? a.href : '', img: imgOf(c), titleAttr: a ? titleOf(a, c) : '', nolink: !a, lines: blocks(c) });
    }
  }
  return { items: out, mode, url: location.href, title: document.title,
           body: (document.body ? document.body.innerText : '').slice(0, 3000), ready: document.readyState };
})
"#;

/// 商品详情页读价：JSON-LD → meta → itemprop → 最大字号价格文字；附带主体文字供 AI 兜底。
pub const PROBE_JS: &str = r#"
(() => {
  const out = { title: '', price: null, currency: null, available: null, image: '', bigPrices: [], text: '', url: location.href, body: '' };
  const num = (v) => { const x = parseFloat(String(v ?? '').replace(/[^0-9.,]/g, '').replace(/,(\d{1,2})$/, '.$1').replace(/,/g, '')); return isNaN(x) ? null : x; };
  for (const s of document.querySelectorAll('script[type="application/ld+json"]')) {
    try {
      const d = JSON.parse(s.textContent); const arr = Array.isArray(d) ? d : (d['@graph'] || [d]);
      for (const x of arr) {
        const t = [].concat(x['@type'] || []);
        if (!t.includes('Product') && !t.includes('ProductGroup')) continue;
        out.title = out.title || x.name || '';
        let o = x.offers; if (Array.isArray(o)) o = o[0];
        if (o) {
          const ps = o.priceSpecification || {};
          out.price = out.price ?? num(o['@type'] === 'AggregateOffer' ? (o.lowPrice ?? o.price) : (o.price ?? ps.price));
          out.currency = out.currency || o.priceCurrency || ps.priceCurrency || null;
          if (o.availability) out.available = !/OutOfStock|SoldOut|Discontinued/i.test(o.availability);
        }
        out.image = out.image || [].concat(x.image || [])[0] || '';
      }
    } catch (e) {}
  }
  const meta = (n) => { const m = document.querySelector(`meta[property="${n}"],meta[name="${n}"],meta[itemprop="${n}"]`); return m ? m.getAttribute('content') : null; };
  if (out.price == null) { out.price = num(meta('product:price:amount') || meta('og:price:amount')); out.currency = out.currency || meta('product:price:currency') || meta('og:price:currency'); }
  if (out.price == null) { const ip = document.querySelector('[itemprop="price"]'); if (ip) { out.price = num(ip.getAttribute('content') || ip.textContent); const ic = document.querySelector('[itemprop="priceCurrency"]'); if (ic) out.currency = out.currency || ic.getAttribute('content'); } }
  out.title = out.title || meta('og:title') || ((document.querySelector('h1') || {}).innerText || '').trim() || document.title;
  out.image = out.image || meta('og:image') || '';
  const re = /^(?:US\s?\$|[¥￥$€£])\s?\d[\d.,\s]*$|^\d[\d.,\s]*\s?(?:€|元|円)$/;
  const c = [];
  for (const el of document.querySelectorAll('body *')) {
    if (el.children.length > 4) continue;
    const t = (el.innerText || '').replace(/\s+/g, ' ').trim();
    if (!t || t.length > 20 || !re.test(t)) continue;
    const r = el.getBoundingClientRect(); if (r.width === 0 || r.top > 1500 || r.top < -50) continue;
    const cs = getComputedStyle(el); if (cs.textDecorationLine.includes('line-through') || el.closest('s,del,strike')) continue;
    c.push({ t, fs: parseFloat(cs.fontSize) || 0, top: r.top });
    if (c.length > 300) break;
  }
  c.sort((a, b) => b.fs - a.fs || a.top - b.top);
  out.bigPrices = c.slice(0, 5).map((x) => x.t);
  out.text = ((document.querySelector('main,#dp,#ppd,#content,article,[class*="detail" i]') || document.body || {}).innerText || '').slice(0, 3000);
  out.body = (document.body ? document.body.innerText : '').slice(0, 1500);
  return out;
})()
"#;

pub const CAPTCHA_HINTS: &[&str] = &[
    "验证码", "滑块", "拖动滑块", "安全验证", "请完成验证", "访问过于频繁", "captcha", "are you a robot", "robot check",
    "enter the characters you see", "unusual traffic", "sicherheitsüberprüfung", "bestätigen sie, dass sie kein roboter",
    "punish", "nocaptcha", "verify you are human", "人机验证",
];

pub enum Outcome {
    Ok(Vec<Candidate>),
    LoginRequired(String),
    Error(String),
    Cancelled,
}

pub struct SearchOpts {
    pub target: usize,
    pub max_rounds: u32,
    /// 撞到登录/验证时最多等待用户处理的秒数；0 = 不弹窗
    pub wait_user_secs: u64,
}

/// 平台搜索过程中的状态通知：(state, message)
pub type Notify<'a> = &'a (dyn Fn(&str, &str) + Send + Sync);

fn detect_wall(p: &Platform, data: &Value) -> (bool, bool) {
    let url = data.get("url").and_then(|u| u.as_str()).unwrap_or("").to_lowercase();
    let blob = format!(
        "{} {} {}",
        url,
        data.get("title").and_then(|u| u.as_str()).unwrap_or(""),
        data.get("body").and_then(|u| u.as_str()).unwrap_or("")
    )
    .to_lowercase();
    let login = p.wall_url.iter().any(|h| url.contains(&h.to_lowercase())) || p.wall_text.iter().any(|h| blob.contains(&h.to_lowercase()));
    let captcha = CAPTCHA_HINTS.iter().any(|h| blob.contains(h));
    (login, captcha)
}

async fn extract(tab: &Tab, p: &Platform) -> Result<Value, String> {
    let pattern = serde_json::to_string(&p.link).unwrap_or_else(|_| "\"\"".into());
    let hint = hint_for(p).map(|h| h.json()).unwrap_or(Value::Null).to_string();
    tab.eval(&format!("({EXTRACT_JS})({pattern}, {hint})"), Duration::from_secs(25)).await
}

/* ---------------- v26.03 平台提示选择器 ---------------- */

/// 搜索结果卡片的平台提示选择器（CSS）。命中 ≥ 3 张卡片时使用结构化字段，否则退回通用识别。
/// 选择器依据各平台公开页面结构编写，站点改版后自动回退，不会导致搜索失败。
#[derive(Debug, Clone, Copy, Default)]
pub struct Hint {
    pub card: &'static str,
    pub id_attr: &'static str,
    pub link: &'static str,
    pub title: &'static str,
    pub price: &'static str,
    pub orig: &'static str,
    pub shop: &'static str,
    pub sales: &'static str,
    pub rating: &'static str,
    pub reviews: &'static str,
    pub location: &'static str,
    pub ship: &'static str,
    pub badges: &'static str,
}

impl Hint {
    pub fn json(&self) -> Value {
        serde_json::json!({
            "card": self.card, "idAttr": self.id_attr, "link": self.link, "title": self.title, "price": self.price, "orig": self.orig,
            "shop": self.shop, "sales": self.sales, "rating": self.rating, "reviews": self.reviews, "location": self.location,
            "ship": self.ship, "badges": self.badges,
        })
    }
}

const H_TAOBAO: Hint = Hint {
    card: "a[class*=\"doubleCardWrapper\"], div[class*=\"Card--doubleCardWrapper\"], a[class*=\"Card--doubleCardWrapper\"], [class*=\"doubleCardWrapperAdapt\"]",
    id_attr: "",
    link: "",
    title: "[class*=\"Title--title\"], [class*=\"title--\"]",
    price: "[class*=\"Price--priceWrapper\"], [class*=\"priceWrapper--\"]",
    orig: "",
    shop: "[class*=\"ShopInfo--shopName\"], [class*=\"shopNameText--\"], [class*=\"shopName--\"]",
    sales: "[class*=\"Price--realSales\"], [class*=\"realSales--\"]",
    rating: "",
    reviews: "",
    location: "[class*=\"Price--procity\"], [class*=\"procity--\"]",
    ship: "",
    badges: "[class*=\"SalesPoint--\"] span, [class*=\"ShopInfo--shopTag\"], [class*=\"iconText\"], [class*=\"subIconWrapper\"] span",
};
const H_JD: Hint = Hint {
    card: "#J_goodsList li.gl-item, li.gl-item, div[data-sku][class*=\"goodsCard\"], div[data-sku][class*=\"_wrapper_\"]",
    id_attr: "data-sku",
    link: "",
    title: ".p-name em, .p-name a, [class*=\"_text_\"][title], [class*=\"goods_title\"]",
    price: ".p-price i, .p-price strong, [class*=\"_price_\"]",
    orig: "",
    shop: ".p-shop a, .p-shop .curr-shop, a[href*=\"mall.jd.com\"]",
    sales: "",
    rating: "",
    reviews: ".p-commit strong a, .p-commit strong, [class*=\"_commit_\"]",
    location: "",
    ship: "",
    badges: ".p-icons i, .p-icons img[alt], [class*=\"_imgTag_\"], [class*=\"_tag_\"]",
};
const H_XIANYU: Hint = Hint {
    card: "a[class*=\"feeds-item-wrap\"], div[class*=\"feeds-item-wrap\"]",
    title: "[class*=\"main-title\"], [class*=\"row1-wrap-title\"]",
    price: "[class*=\"price-wrap\"], [class*=\"price--\"]",
    location: "[class*=\"seller-text\"], [class*=\"row4-wrap-seller\"]",
    badges: "[class*=\"cpv--\"], [class*=\"tag--\"]",
    ..EMPTY
};
const H_1688: Hint = Hint {
    card: ".sm-offer-item, .search-offer-item, [class*=\"offer-list-row\"] > div, .space-offer-card-box",
    title: ".title, [class*=\"offer-title\"], .mojar-element-title",
    price: ".price, [class*=\"offer-price\"], .mojar-element-price",
    shop: ".company-name, [class*=\"company-name\"], .desc-text",
    sales: ".sale, [class*=\"sale\"], .sale-amount",
    badges: "[class*=\"tag\"] span, .icon-text",
    ..EMPTY
};
const H_AMAZON: Hint = Hint {
    card: "div[data-component-type=\"s-search-result\"]",
    id_attr: "data-asin",
    link: "h2 a, a.a-link-normal.s-no-outline, a.a-link-normal[href*=\"/dp/\"]",
    title: "h2 a span, h2 span, [data-cy=\"title-recipe\"] h2",
    price: ".a-price:not(.a-text-price) .a-offscreen",
    orig: ".a-price.a-text-price .a-offscreen",
    shop: "",
    sales: "[data-cy=\"reviews-block\"] + div span.a-color-secondary, span.a-size-base.a-color-secondary",
    rating: "span.a-icon-alt, [aria-label*=\"out of 5\"], [aria-label*=\"von 5\"]",
    reviews: "a[href*=\"customerReviews\"] span.s-underline-text, a[href*=\"customerReviews\"] span, span[aria-label$=\"ratings\"], span[aria-label*=\"Bewertungen\"]",
    location: "",
    ship: "[data-cy=\"delivery-recipe\"], .s-align-children-center",
    badges: ".a-badge-text, [data-component-type=\"s-status-badge-component\"] span, .puis-sponsored-label-text, i.a-icon-prime[aria-label]",
};
const H_EBAY: Hint = Hint {
    card: "li.s-item, li.s-card, ul.srp-results > li[data-listingid]",
    id_attr: "data-listingid",
    link: "a.s-item__link, a.su-link, a[href*=\"/itm/\"]",
    title: ".s-item__title, .s-card__title",
    price: ".s-item__price, .s-card__price",
    orig: ".s-item__trending-price .STRIKETHROUGH, .s-card__price--previous",
    shop: ".s-item__seller-info-text, .s-card__seller-info",
    sales: ".s-item__quantitySold, .s-item__hotness, .s-card__sold",
    rating: ".x-star-rating .clipped, .s-item__reviews .clipped",
    reviews: ".s-item__reviews-count span",
    location: ".s-item__location, .s-item__itemLocation",
    ship: ".s-item__shipping, .s-item__logisticsCost, .s-item__freeXDays",
    badges: ".s-item__subtitle, .SECONDARY_INFO, .s-item__etrs-text, .s-card__subtitle",
};
const H_IDEALO: Hint = Hint {
    card: "[class*=\"sr-resultList__item\"], [data-testid=\"resultItem\"], .offerList-item",
    title: "[class*=\"sr-productSummary__title\"], [class*=\"productSummary__title\"], .offerList-item-description-title",
    price: "[class*=\"sr-detailedPriceInfo__price\"], [class*=\"detailedPriceInfo__price\"], .offerList-item-priceMin",
    rating: "[class*=\"sr-productSummary__rating\"]",
    ..EMPTY
};
const H_ALIEXPRESS: Hint = Hint {
    card: "div[class*=\"search-item-card-wrapper\"], a[class*=\"search-card-item\"], [class*=\"list--gallery\"] > div",
    title: "h3, [class*=\"multi--titleText\"], [class*=\"title--\"]",
    price: "[class*=\"multi--price-sale\"], [class*=\"price-sale\"], [class*=\"price--current\"]",
    orig: "[class*=\"price-original\"], [class*=\"price--original\"]",
    shop: "[class*=\"cards--store\"], [class*=\"store--\"]",
    sales: "[class*=\"trade--\"], [class*=\"multi--trade\"]",
    rating: "[class*=\"evaluation\"], [class*=\"multi--starList\"]",
    ship: "[class*=\"shipping\"], [class*=\"tag--\"]",
    ..EMPTY
};
const H_WALMART: Hint = Hint {
    card: "div[data-item-id]",
    id_attr: "data-item-id",
    title: "[data-automation-id=\"product-title\"]",
    price: "[data-automation-id=\"product-price\"] .f2, [data-automation-id=\"product-price\"] span.w_iUH7, [data-automation-id=\"product-price\"]",
    rating: "[data-testid=\"product-ratings\"], span.w_iUH7",
    reviews: "[data-testid=\"product-reviews\"]",
    ship: "[data-automation-id=\"fulfillment-badge\"]",
    badges: "[data-testid=\"badgeTagComponent\"] span",
    ..EMPTY
};
const H_BESTBUY: Hint = Hint {
    card: "li.sku-item, .sku-item",
    id_attr: "data-sku-id",
    title: ".sku-title a, h4.sku-title, .sku-header a",
    price: ".priceView-customer-price span:first-child, [data-testid=\"customer-price\"] span",
    rating: ".c-ratings-reviews p.visually-hidden, .ratings-reviews .visually-hidden",
    reviews: ".c-total-reviews, .c-reviews",
    ..EMPTY
};
const H_RAKUTEN: Hint = Hint {
    card: ".searchresultitem, [class*=\"searchresultitem\"], div[class*=\"dui-card searchresultitem\"]",
    title: ".title a, h2 a, [class*=\"title-link\"]",
    price: ".important, [class*=\"price--\"], .price",
    shop: ".merchant a, [class*=\"merchant\"]",
    reviews: ".legend, .review a",
    ship: ".dui-tag, [class*=\"shipping\"]",
    badges: ".dui-tag",
    ..EMPTY
};
const H_MERCARI: Hint = Hint {
    card: "li[data-testid=\"item-cell\"]",
    title: "[data-testid=\"thumbnail-item-name\"], span[class*=\"itemName\"]",
    price: "[class*=\"number__\"], [class*=\"price\"]",
    ..EMPTY
};
const H_KLEINANZEIGEN: Hint = Hint {
    card: "article.aditem, li.ad-listitem article",
    id_attr: "data-adid",
    title: "h2 a, .text-module-begin a, a.ellipsis",
    price: ".aditem-main--middle--price-shipping--price, [class*=\"price-shipping--price\"]",
    location: ".aditem-main--top--left",
    ship: ".aditem-main--middle--price-shipping--shipping",
    badges: ".simpletag, .text-module-end span",
    ..EMPTY
};
const H_ETSY: Hint = Hint {
    card: ".v2-listing-card, div[data-listing-id]",
    id_attr: "data-listing-id",
    title: ".v2-listing-card__title, h3",
    price: ".lc-price .currency-value, .n-listing-card__price .currency-value, p.lc-price",
    shop: ".v2-listing-card__shop span, [class*=\"shop-name\"]",
    rating: "input[name=\"initial-rating\"], [aria-label*=\"out of 5\"]",
    ..EMPTY
};
const H_OTTO: Hint = Hint {
    card: "article.product, .find_tile",
    title: ".find_tile__name, [class*=\"tile__name\"]",
    price: ".find_tile__retailPrice, [class*=\"retailPrice\"], [class*=\"tile__price\"]",
    ..EMPTY
};
const H_ALIBABA: Hint = Hint {
    card: ".fy23-search-card, [class*=\"search-card-e\"], .organic-list > div",
    title: "h2, [class*=\"search-card-e-title\"]",
    price: "[class*=\"search-card-e-price\"], [class*=\"price\"]",
    shop: "[class*=\"search-card-e-company\"], [class*=\"supplier\"]",
    badges: "[class*=\"verified\"], [class*=\"gold\"]",
    ..EMPTY
};
const EMPTY: Hint = Hint {
    card: "", id_attr: "", link: "", title: "", price: "", orig: "", shop: "", sales: "", rating: "", reviews: "", location: "", ship: "", badges: "",
};

/// 平台 → 提示选择器（按 id / 家族）。拼多多、Temu、Shein、社区类平台结构变化快，只用通用识别。
pub fn hint_for(p: &Platform) -> Option<Hint> {
    let id = p.id.as_str();
    Some(match id {
        "taobao" => H_TAOBAO,
        "jd" => H_JD,
        "xianyu" => H_XIANYU,
        "alibaba1688" => H_1688,
        "aliexpress" => H_ALIEXPRESS,
        "walmart" => H_WALMART,
        "bestbuy" => H_BESTBUY,
        "rakuten" => H_RAKUTEN,
        "mercari_jp" => H_MERCARI,
        "kleinanzeigen" => H_KLEINANZEIGEN,
        "etsy" => H_ETSY,
        "otto" => H_OTTO,
        "idealo" => H_IDEALO,
        "alibaba" => H_ALIBABA,
        _ if id.starts_with("amazon") || p.family == "amazon" => H_AMAZON,
        _ if id.starts_with("ebay") || p.family == "ebay" => H_EBAY,
        _ => return None,
    })
}

fn item_count(v: &Value) -> usize {
    v.get("items").and_then(|i| i.as_array()).map(|a| a.len()).unwrap_or(0)
}

/// 单个搜索词：返回候选与"是否弹出过窗口"。
async fn search_once(
    browser: &Arc<Browser>, p: &Platform, q: &str, sort: &str, opts: &SearchOpts, notify: Notify<'_>, cancel: &CancellationToken,
) -> Result<Vec<Candidate>, Outcome> {
    let url = p.search_url(q, sort);
    let tab = browser.new_tab(&url, true).await.map_err(Outcome::Error)?;
    let mut guard = TabGuard::new(browser, &tab.id);
    let mut shown = false;
    let result: Result<Vec<Candidate>, Outcome> = async {
        let mut deadline = Instant::now() + Duration::from_secs(25);
        let mut asked = false;
        let mut data: Value;
        loop {
            if cancel.is_cancelled() {
                return Err(Outcome::Cancelled);
            }
            tokio::time::sleep(Duration::from_millis(1300)).await;
            data = extract(&tab, p).await.unwrap_or(Value::Null);
            if item_count(&data) > 0 {
                break;
            }
            let href = data.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if href.starts_with("chrome-error://") || href.starts_with("edge-error://") {
                return Err(Outcome::Error(format!("无法打开{}（网络不可达或被代理拦截）", p.name)));
            }
            let (login, captcha) = detect_wall(p, &data);
            if (login || captcha) && !asked {
                if opts.wait_user_secs == 0 {
                    return Err(if login {
                        Outcome::LoginRequired(format!("{} 需要登录：到「平台 · 账号与登录」登录一次即可", p.name))
                    } else {
                        Outcome::Error(format!("{} 需要验证（滑块 / 安全验证）：在设置中开启「搜索时允许弹窗」，或在「平台 · 账号与登录」打开该平台手动完成一次验证后重试", p.name))
                    });
                }
                asked = true;
                deadline = Instant::now() + Duration::from_secs(opts.wait_user_secs);
                browser.show(&tab.id).await;
                shown = true;
                notify("waiting_user", &format!("{} 需要{}，请在弹出的浏览器窗口中完成，完成后自动继续", p.name, if login { "重新登录" } else { "安全验证" }));
            }
            if Instant::now() > deadline {
                if asked {
                    return Err(if login { Outcome::LoginRequired(format!("{} 等待登录超时", p.name)) } else { Outcome::Error(format!("{} 等待验证超时", p.name)) });
                }
                break; // 页面正常但没有结果
            }
        }
        if asked {
            notify("searching", &format!("{} 已继续搜索", p.name));
        }
        // 自适应滚动：够数或连续两屏无新增即停
        let mut last = item_count(&data);
        let mut stale = 0;
        for _ in 0..opts.max_rounds {
            if last >= opts.target || cancel.is_cancelled() {
                break;
            }
            let _ = tab.eval("window.scrollBy(0, Math.round(window.innerHeight * 0.9)); true", Duration::from_secs(5)).await;
            tokio::time::sleep(Duration::from_millis(1300 + (rand_ms() % 1300))).await;
            if let Ok(d) = extract(&tab, p).await {
                let now = item_count(&d);
                stale = if now <= last { stale + 1 } else { 0 };
                if now >= last {
                    data = d;
                }
                last = last.max(now);
            }
            if stale >= 2 {
                break;
            }
        }
        let raws: Vec<RawCard> = data
            .get("items")
            .and_then(|i| serde_json::from_value::<Vec<RawCard>>(i.clone()).ok())
            .unwrap_or_default();
        let cands: Vec<Candidate> = raws.iter().filter_map(|r| parse_card(p, r, q)).collect();
        if cands.is_empty() {
            return Err(Outcome::Error(format!("没有在{}识别到「{q}」的结果（页面可能改版或确实没有相关商品）", p.name)));
        }
        Ok(cands)
    }
    .await;
    if shown {
        browser.hide(&tab.id).await;
    }
    browser.close_tab(tab).await;
    guard.disarm();
    result
}

fn rand_ms() -> u64 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
    (n as u64).wrapping_mul(2654435761) >> 7
}

/// 一个平台的完整搜索（多个搜索词，失败换标签重试一次）。
pub async fn search_platform(
    browser: &Arc<Browser>, p: &Platform, queries: &[String], sort: &str, opts: SearchOpts, notify: Notify<'_>, cancel: &CancellationToken,
) -> Outcome {
    let _slot = tokio::select! {
        s = browser.slot() => s,
        _ = cancel.cancelled() => return Outcome::Cancelled,
    };
    notify("searching", &format!("正在打开 {}", p.name));
    let mut found: Vec<Candidate> = vec![];
    let mut errors: Vec<String> = vec![];
    for (qi, q) in queries.iter().enumerate() {
        if found.len() >= opts.target * 3 / 2 {
            break;
        }
        for attempt in 0..2 {
            match search_once(browser, p, q, sort, &opts, notify, cancel).await {
                Ok(mut c) => {
                    found.append(&mut c);
                    break;
                }
                Err(Outcome::Cancelled) => return Outcome::Cancelled,
                Err(Outcome::LoginRequired(m)) => return Outcome::LoginRequired(m),
                Err(Outcome::Error(e)) => {
                    let retryable = e.contains("超时") || e.contains("CDP") || e.contains("断开");
                    if attempt == 1 || !retryable {
                        errors.push(e);
                        break;
                    }
                }
                Err(Outcome::Ok(_)) => break,
            }
        }
        if queries.len() > 1 {
            notify("searching", &format!("{}：第 {}/{} 个搜索词，累计 {} 条", p.name, qi + 1, queries.len(), found.len()));
        }
    }
    if found.is_empty() {
        return Outcome::Error(errors.join("；").chars().take(300).collect());
    }
    Outcome::Ok(found)
}

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub title: String,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub available: Option<bool>,
    pub image: String,
    pub big_prices: Vec<String>,
    pub text: String,
    pub login_wall: bool,
}

/// 打开商品详情页读取价格（后台标签，节奏受站点间隔控制）。
pub async fn probe(browser: &Arc<Browser>, p: Option<&Platform>, url: &str) -> Result<Probe, String> {
    let _slot = browser.slot().await;
    let tab = browser.new_tab(url, true).await?;
    let mut guard = TabGuard::new(browser, &tab.id);
    let res = async {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let _ = tab.eval("window.scrollBy(0, 400); true", Duration::from_secs(5)).await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let v = tab.eval(PROBE_JS, Duration::from_secs(20)).await?;
        let mut pr = Probe {
            title: v["title"].as_str().unwrap_or("").trim().chars().take(300).collect(),
            price: v["price"].as_f64().filter(|x| *x > 0.0),
            currency: v["currency"].as_str().map(|s| s.to_uppercase()).filter(|s| s.len() == 3),
            available: v["available"].as_bool(),
            image: v["image"].as_str().unwrap_or("").to_string(),
            big_prices: v["bigPrices"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
            text: v["text"].as_str().unwrap_or("").to_string(),
            login_wall: false,
        };
        if let Some(p) = p {
            let (login, _) = detect_wall(p, &serde_json::json!({"url": v["url"], "title": v["title"], "body": v["body"]}));
            pr.login_wall = login && pr.price.is_none();
        }
        Ok::<Probe, String>(pr)
    }
    .await;
    browser.close_tab(tab).await;
    guard.disarm();
    res
}

/* ---------------- v26.03 评论检索：商品详情页 ---------------- */

/// 找到"商品评价 / Customer reviews"标签并切换过去（只点页内标签，不跟随跳转链接）。
pub const REVIEW_TAB_JS: &str = r#"
(() => {
  const re = /^(商品评价|宝贝评价|累计评价|用户评价|买家评价|评价|评论|Customer reviews|Reviews|Ratings & Reviews|Kundenrezensionen|Bewertungen|Rezensionen|レビュー)\s*[（(]?\s*[\d万+.,kK]*\s*[)）]?$/i;
  for (const el of document.querySelectorAll('a,li,span,button,[role="tab"],h2,h3')) {
    const t = (el.innerText || '').trim(); if (!t || t.length > 24 || !re.test(t)) continue;
    const r = el.getBoundingClientRect(); if (r.width === 0 && r.height === 0) continue;
    el.scrollIntoView({ block: 'center' });
    const a = el.closest('a[href]'); const href = a ? (a.getAttribute('href') || '') : '';
    const safe = !a || href.startsWith('#') || href.startsWith('javascript');
    if (safe && el.tagName !== 'H2' && el.tagName !== 'H3') { try { el.click(); } catch (e) {} }
    return t;
  }
  return '';
})()
"#;

/// 读取评分、评论数、好评率、评论文字、店铺信息（平台提示 + JSON-LD + 通用规则），以及页面文字供 Rust 解析。
pub const REVIEWS_JS: &str = r#"
((hint) => {
  const qs = (el, sel) => { if (!sel) return null; try { return el.querySelector(sel); } catch (e) { return null; } };
  const qsa = (el, sel) => { if (!sel) return []; try { return [...el.querySelectorAll(sel)]; } catch (e) { return []; } };
  const tx = (el) => { if (!el) return ''; let t = (el.innerText || '').replace(/\s+/g, ' ').trim(); if (!t) t = (el.textContent || '').replace(/\s+/g, ' ').trim(); if (!t) t = (el.getAttribute('aria-label') || el.getAttribute('title') || '').trim(); return t.slice(0, 400); };
  const out = { rating: '', count: '', goodRate: '', reviews: [], ldRating: null, ldCount: null, shopName: '', shopScores: '', seller: '', badges: [], url: location.href, title: document.title, body: '' };
  const push = (t) => {
    t = (t || '').replace(/\s+/g, ' ').trim();
    if (t.length < 6 || t.length > 400) return;
    if (/^(write a review|bewertung schreiben|写评价|查看全部|查看更多|more|mehr|helpful|hilfreich|report|melden|有用|举报)/i.test(t)) return;
    if (out.reviews.some((o) => o.includes(t) || t.includes(o))) return;
    out.reviews.push(t);
  };
  for (const sc of document.querySelectorAll('script[type="application/ld+json"]')) {
    try {
      const d = JSON.parse(sc.textContent); const arr = Array.isArray(d) ? d : (d['@graph'] || [d]);
      for (const x of arr) {
        const t = [].concat(x['@type'] || []); if (!t.includes('Product') && !t.includes('ProductGroup')) continue;
        const ar = x.aggregateRating || {};
        if (ar.ratingValue != null) { const best = parseFloat(ar.bestRating || 5) || 5; out.ldRating = parseFloat(ar.ratingValue) / best * 5; }
        if (ar.reviewCount != null || ar.ratingCount != null) out.ldCount = parseFloat(ar.reviewCount ?? ar.ratingCount);
        for (const r of [].concat(x.review || []).slice(0, 20)) push(r.reviewBody || r.description || '');
        if (x.brand && x.offers && !out.seller) { const o = [].concat(x.offers)[0] || {}; if (o.seller && o.seller.name) out.seller = 'Sold by ' + o.seller.name; }
      }
    } catch (e) {}
  }
  if (hint) {
    out.rating = tx(qs(document, hint.rating)); out.count = tx(qs(document, hint.count)); out.goodRate = tx(qs(document, hint.goodRate));
    out.shopName = tx(qs(document, hint.shopName));
    out.shopScores = qsa(document, hint.shopScores).map(tx).join(' ').slice(0, 600);
    out.seller = (out.seller + ' ' + qsa(document, hint.seller).map(tx).join(' ')).trim().slice(0, 400);
    for (const el of qsa(document, hint.reviews)) { push(tx(el)); if (out.reviews.length >= 30) break; }
    out.badges = qsa(document, hint.badges).map(tx).filter((t) => t && t.length <= 18).slice(0, 8);
  }
  if (out.reviews.length < 3) {
    const re = /(comment|review|rezension|bewertung|evaluat|feedback|rate-?(content|txt|text)|ratecontent)/i;
    for (const el of document.querySelectorAll('[class],[id],[data-hook]')) {
      const key = (typeof el.className === 'string' ? el.className : '') + ' ' + (el.id || '') + ' ' + (el.getAttribute('data-hook') || '');
      if (!re.test(key) || el.children.length > 8) continue;
      const r = el.getBoundingClientRect(); if (r.width === 0 && r.height === 0) continue;
      push(tx(el));
      if (out.reviews.length >= 30) break;
    }
  }
  out.body = (document.body ? document.body.innerText : '').slice(0, 8000);
  return out;
})
"#;

/// 详情页的评论 / 店铺提示选择器。
#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewHint {
    pub reviews: &'static str,
    pub rating: &'static str,
    pub count: &'static str,
    pub good_rate: &'static str,
    pub shop_name: &'static str,
    pub shop_scores: &'static str,
    pub seller: &'static str,
    pub badges: &'static str,
}

impl ReviewHint {
    pub fn json(&self) -> Value {
        serde_json::json!({
            "reviews": self.reviews, "rating": self.rating, "count": self.count, "goodRate": self.good_rate,
            "shopName": self.shop_name, "shopScores": self.shop_scores, "seller": self.seller, "badges": self.badges,
        })
    }
}

pub fn review_hint_for(p: &Platform) -> Option<ReviewHint> {
    let id = p.id.as_str();
    Some(match id {
        "jd" => ReviewHint {
            reviews: ".comment-item .comment-con, [class*=\"comment-item\"] [class*=\"comment-con\"], [class*=\"_comment_content\"]",
            rating: "",
            count: "#comment-count .count, .J-comm-count, [class*=\"comment-count\"]",
            good_rate: ".percent-con, .comment-percent strong, [class*=\"good-rate\"]",
            shop_name: ".J-hove-wrap .name a, .shopName strong span, .popbox-inner .mt h3 a, [class*=\"shop-name\"]",
            shop_scores: ".score-parts, .score-body, [class*=\"shop-score\"], .J-shop-score",
            seller: ".J-hove-wrap, .contact",
            badges: ".u-jd, .goodshop, .ziying, [class*=\"self-tag\"]",
        },
        "taobao" => ReviewHint {
            reviews: "[class*=\"Comment--content\"], [class*=\"comment--content\"], .tm-rate-fulltxt, [class*=\"Review--content\"]",
            rating: "",
            count: "[class*=\"Tabs--title\"]",
            good_rate: "",
            shop_name: "[class*=\"ShopHeader--title\"], [class*=\"ShopHeaderNew--shopName\"], [class*=\"shopName--\"], .shop-name-link, .slogo-shopname",
            shop_scores: "[class*=\"ShopHeader--\"] [class*=\"score\"], [class*=\"StoreComprehensiveRating\"], .shop-rate, .tb-shop-rate",
            seller: "",
            badges: "[class*=\"ShopHeader--tag\"], [class*=\"shopTag\"]",
        },
        "xianyu" => ReviewHint {
            shop_name: "[class*=\"item-user-info-nick\"], [class*=\"nick--\"]",
            shop_scores: "[class*=\"item-user-info-label\"], [class*=\"credit\"]",
            badges: "[class*=\"label--\"], [class*=\"tag--\"]",
            ..Default::default()
        },
        "alibaba1688" => ReviewHint {
            reviews: "[class*=\"evaluate-item\"] [class*=\"content\"]",
            shop_name: ".company-name, [class*=\"company-name\"]",
            shop_scores: "[class*=\"score\"], .supplier-info",
            ..Default::default()
        },
        _ if id.starts_with("amazon") || p.family == "amazon" => ReviewHint {
            reviews: "[data-hook=\"review-body\"] span, [data-hook=\"review-collapsed\"] span",
            rating: "[data-hook=\"rating-out-of-text\"], #acrPopover span.a-icon-alt, #averageCustomerReviews span.a-icon-alt",
            count: "#acrCustomerReviewText, [data-hook=\"total-review-count\"]",
            good_rate: "",
            shop_name: "#sellerProfileTriggerId, #merchantInfoFeature_feature_div .offer-display-feature-text-message, [offer-display-feature-name=\"desktop-merchant-info\"] .offer-display-feature-text-message",
            shop_scores: "",
            seller: "#merchant-info, #tabular-buybox, #merchantInfoFeature_feature_div, #fulfillerInfoFeature_feature_div",
            badges: "#acBadge_feature_div .ac-badge-text-primary, .a-badge-text",
        },
        _ if id.starts_with("ebay") || p.family == "ebay" => ReviewHint {
            reviews: ".fdbk-container__details__comment span, .ebay-review-section .review-item-content, [class*=\"review-item-content\"]",
            rating: ".ebay-review-start-rating, .x-star-rating .clipped",
            count: ".ebay-reviews-count",
            good_rate: "",
            shop_name: ".x-sellercard-atf__info__about-seller a span, .ux-seller-section__item--seller a span, [data-testid=\"str-title\"] a",
            shop_scores: ".x-sellercard-atf__data-item, .ux-seller-section__item, .x-sellercard-atf__about-seller",
            seller: ".x-sellercard-atf__about-seller, .ux-seller-section",
            badges: ".x-sellercard-atf__data-item--top-rated, .ux-seller-section__item--top-rated",
        },
        _ => return None,
    })
}

/// 打开商品详情页读取评论与店铺信息（后台标签；遇到登录 / 验证不等待，直接报告原因）。
/// 返回脚本原始结果，并附加 loginWall / captcha 标记；文字解析在 search::reviews 中完成。
pub async fn read_detail(browser: &Arc<Browser>, p: &Platform, url: &str, cancel: &CancellationToken) -> Result<Value, String> {
    let _slot = tokio::select! {
        s = browser.slot() => s,
        _ = cancel.cancelled() => return Err("已取消".into()),
    };
    let tab = browser.new_tab(url, true).await?;
    let mut guard = TabGuard::new(browser, &tab.id);
    let hint = review_hint_for(p).map(|h| h.json()).unwrap_or(Value::Null).to_string();
    let res = async {
        let pause = |ms: u64| async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(ms)) => Ok(()),
                _ = cancel.cancelled() => Err("已取消".to_string()),
            }
        };
        pause(2200).await?;
        let first = tab
            .eval("({url: location.href, title: document.title, body: document.body ? document.body.innerText.slice(0, 3000) : ''})", Duration::from_secs(10))
            .await
            .unwrap_or(Value::Null);
        let (login, captcha) = detect_wall(p, &first);
        if captcha || (login && first["body"].as_str().unwrap_or("").chars().count() < 1500) {
            return Ok(serde_json::json!({"loginWall": login, "captcha": captcha, "url": first["url"], "title": first["title"], "reviews": [], "body": ""}));
        }
        let _ = tab.eval(REVIEW_TAB_JS, Duration::from_secs(8)).await;
        pause(1400).await?;
        for _ in 0..3 {
            let _ = tab.eval("window.scrollBy(0, Math.round(window.innerHeight * 0.9)); true", Duration::from_secs(5)).await;
            pause(800 + rand_ms() % 500).await?;
        }
        let mut v = tab.eval(&format!("({REVIEWS_JS})({hint})"), Duration::from_secs(20)).await?;
        let none = v["reviews"].as_array().is_none_or(|a| a.is_empty()) && v["rating"].as_str().unwrap_or("").is_empty() && v["ldRating"].is_null();
        let (login, captcha) = detect_wall(p, &v);
        v["loginWall"] = Value::Bool(login && none);
        v["captcha"] = Value::Bool(captcha && none);
        Ok::<Value, String>(v)
    }
    .await;
    browser.close_tab(tab).await;
    guard.disarm();
    res
}
