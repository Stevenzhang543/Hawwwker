//! 汇率：ECB 参考汇率（frankfurter.app，免密钥），缓存 12 小时；离线时使用内置参考表。

use crate::db::{now_iso, parse_iso, Db};
use crate::models::FxRates;
use chrono::{Duration, Utc};
use std::collections::BTreeMap;
use std::sync::RwLock;

pub struct Fx {
    rates: RwLock<FxRates>,
}

/// 内置参考表（以 EUR 为基准，2026 年中参考值；只在完全离线时使用）。
fn builtin() -> FxRates {
    let mut r = BTreeMap::new();
    for (k, v) in [
        ("EUR", 1.0), ("USD", 1.13), ("CNY", 8.15), ("GBP", 0.85), ("JPY", 168.0), ("HKD", 8.85), ("KRW", 1560.0),
        ("CHF", 0.94), ("SGD", 1.47), ("AUD", 1.74), ("CAD", 1.56), ("PLN", 4.27), ("SEK", 10.9), ("TWD", 34.5),
    ] {
        r.insert(k.to_string(), v);
    }
    FxRates { base: "EUR".into(), date: "builtin".into(), source: "builtin".into(), rates: r }
}

impl Fx {
    pub fn new(db: &Db) -> Self {
        let cached: Option<FxRates> = db.setting("fx_cache");
        Fx { rates: RwLock::new(cached.unwrap_or_else(builtin)) }
    }

    pub fn snapshot(&self) -> FxRates {
        self.rates.read().map(|r| r.clone()).unwrap_or_else(|_| builtin())
    }

    /// amount（from 币种）→ to 币种；未知币种返回 None。
    pub fn convert(&self, amount: f64, from: &str, to: &str) -> Option<f64> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();
        if from == to {
            return Some(amount);
        }
        let r = self.rates.read().ok()?;
        let rate = |c: &str| if c == r.base { Some(1.0) } else { r.rates.get(c).copied() };
        let a = rate(&from)?;
        let b = rate(&to)?;
        Some(((amount / a * b) * 100.0).round() / 100.0)
    }

    pub fn needs_refresh(&self, db: &Db) -> bool {
        let fetched: Option<String> = db.setting("fx_fetched_at");
        match fetched.as_deref().and_then(parse_iso) {
            Some(t) => Utc::now() - t > Duration::hours(12),
            None => true,
        }
    }

    /// 刷新汇率：先 ECB（frankfurter.app），失败再用 open.er-api.com；都失败保留缓存或内置表。
    pub async fn refresh(&self, db: &Db, http: &reqwest::Client) -> Result<(), String> {
        let mut errors = vec![];
        for (source, url) in [("ecb", "https://api.frankfurter.app/latest?from=EUR"), ("er-api", "https://open.er-api.com/v6/latest/EUR")] {
            match Self::fetch(http, url).await {
                Ok((date, mut rates)) => {
                    rates.insert("EUR".into(), 1.0);
                    if rates.len() < 10 || !rates.contains_key("CNY") || !rates.contains_key("USD") {
                        errors.push(format!("{source}: 数据不完整"));
                        continue;
                    }
                    let fx = FxRates { base: "EUR".into(), date, source: source.into(), rates };
                    let _ = db.put_setting("fx_cache", &fx);
                    let _ = db.put_setting("fx_fetched_at", &now_iso());
                    if let Ok(mut w) = self.rates.write() {
                        *w = fx;
                    }
                    return Ok(());
                }
                Err(e) => errors.push(format!("{source}: {e}")),
            }
        }
        Err(errors.join("；"))
    }

    async fn fetch(http: &reqwest::Client, url: &str) -> Result<(String, BTreeMap<String, f64>), String> {
        let resp = http.get(url).timeout(std::time::Duration::from_secs(12)).send().await.map_err(|e| e.without_url().to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let date = v["date"].as_str().or_else(|| v["time_last_update_utc"].as_str()).unwrap_or("").chars().take(16).collect::<String>();
        let rates: BTreeMap<String, f64> = v["rates"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, x)| x.as_f64().filter(|f| *f > 0.0).map(|f| (k.to_uppercase(), f))).collect())
            .unwrap_or_default();
        Ok((date, rates))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_via_base() {
        let db = Db::memory().unwrap();
        let fx = Fx::new(&db);
        assert_eq!(fx.convert(100.0, "EUR", "EUR"), Some(100.0));
        let cny = fx.convert(100.0, "EUR", "CNY").unwrap();
        assert!((cny - 815.0).abs() < 0.01);
        let back = fx.convert(cny, "CNY", "eur").unwrap();
        assert!((back - 100.0).abs() < 0.05);
        assert!(fx.convert(1.0, "XXX", "EUR").is_none());
    }
}
