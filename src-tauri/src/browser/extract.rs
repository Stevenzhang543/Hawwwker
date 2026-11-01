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
((pattern) => {
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
    tab.eval(&format!("({EXTRACT_JS})({pattern})"), Duration::from_secs(25)).await
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
                        Outcome::Error(format!("{} 要求安全验证：可在设置中开启「搜索时允许弹窗」后重试", p.name))
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
