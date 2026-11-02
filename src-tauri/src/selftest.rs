//! 平台自检：对选定平台做一次真实的小搜索，报告能否识别结果、价格是否解析成功。
//! 既用于「平台目录 · 检测」按钮，也用于命令行 `hawwwker --selftest-sites <报告.md> [平台…]`（发布前在真实网络上验证国际站规则）。

use crate::browser::extract::{search_platform, Outcome, SearchOpts};
use crate::browser::manager::Browser;
use crate::models::{SelfTestResult, SelfTestSample};
use crate::platforms::Platform;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// 按站点语言选一个通用、各平台都有结果的搜索词。
pub fn default_query(p: &Platform) -> &'static str {
    match p.locale.split('-').next().unwrap_or("en") {
        "zh" => "蓝牙耳机",
        "de" => "Kopfhörer",
        "fr" => "casque audio",
        "ja" => "イヤホン",
        _ => "headphones",
    }
}

pub async fn run_one(browser: &Arc<Browser>, p: &Platform, query: Option<&str>) -> SelfTestResult {
    let q = query.filter(|q| !q.trim().is_empty()).unwrap_or(default_query(p)).to_string();
    let started = Instant::now();
    let cancel = CancellationToken::new();
    let notify = |_: &str, _: &str| {};
    let opts = SearchOpts { target: 12, max_rounds: 1, wait_user_secs: 0 };
    let fut = search_platform(browser, p, std::slice::from_ref(&q), "relevance", opts, &notify, &cancel);
    let outcome = match tokio::time::timeout(Duration::from_secs(90), fut).await {
        Ok(o) => o,
        Err(_) => Outcome::Error("90 秒内没有完成".into()),
    };
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Outcome::Ok(c) => {
            let with_price = c.iter().filter(|x| x.price.is_some()).count() as u32;
            SelfTestResult {
                platform_id: p.id.clone(),
                ok: with_price > 0,
                count: c.len() as u32,
                with_price,
                samples: c
                    .iter()
                    .take(3)
                    .map(|x| SelfTestSample { title: x.title.chars().take(80).collect(), price: x.price, currency: x.currency.clone(), landed: x.landed(), url: x.url.clone() })
                    .collect(),
                ms,
                login_required: false,
                message: if with_price == 0 { Some("识别到商品但没有解析出价格".into()) } else { None },
            }
        }
        Outcome::LoginRequired(m) => SelfTestResult { platform_id: p.id.clone(), ok: false, count: 0, with_price: 0, samples: vec![], ms, login_required: true, message: Some(m) },
        Outcome::Error(e) => SelfTestResult { platform_id: p.id.clone(), ok: false, count: 0, with_price: 0, samples: vec![], ms, login_required: false, message: Some(e) },
        Outcome::Cancelled => SelfTestResult { platform_id: p.id.clone(), ok: false, count: 0, with_price: 0, samples: vec![], ms, login_required: false, message: Some("已取消".into()) },
    }
}

pub fn report_markdown(results: &[SelfTestResult], platforms: &[Platform]) -> String {
    let mut out = String::from("# Hawwwker 平台自检报告\n\n");
    out.push_str(&format!("时间：{}  \n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z")));
    let ok = results.iter().filter(|r| r.ok).count();
    out.push_str(&format!("通过：{ok} / {}\n\n", results.len()));
    out.push_str("| 平台 | 结果 | 识别条数 | 含价格 | 用时 | 示例 / 说明 |\n|---|---|---:|---:|---:|---|\n");
    for r in results {
        let name = platforms.iter().find(|p| p.id == r.platform_id).map(|p| format!("{} {}", p.name, p.edition)).unwrap_or_else(|| r.platform_id.clone());
        let verdict = if r.ok { "✅ 通过" } else if r.login_required { "🔐 需要登录" } else { "⚠️ 未通过" };
        let detail = match r.samples.first() {
            Some(s) => format!("{} — {} {}", s.title.replace('|', "/"), s.currency, s.price.map(|p| format!("{p:.2}")).unwrap_or("?".into())),
            None => r.message.clone().unwrap_or_default().replace('|', "/"),
        };
        out.push_str(&format!("| {name} | {verdict} | {} | {} | {:.1}s | {detail} |\n", r.count, r.with_price, r.ms as f64 / 1000.0));
    }
    out
}
