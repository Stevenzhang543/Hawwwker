//! 应用设置（JSON 存于 settings 表），支持局部更新（深度合并）。

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::AppSettings;
use serde_json::Value;

const KEY: &str = "app_settings";

pub fn load(db: &Db) -> AppSettings {
    db.setting(KEY).unwrap_or_default()
}

fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    continue;
                }
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

pub fn update(db: &Db, patch: Value) -> AppResult<AppSettings> {
    if !patch.is_object() {
        return Err(AppError::invalid("设置格式错误"));
    }
    let mut cur = serde_json::to_value(load(db))?;
    merge(&mut cur, &patch);
    let mut s: AppSettings = serde_json::from_value(cur).map_err(|e| AppError::invalid(format!("设置项无效：{e}")))?;
    if !["consumer", "merchant", "analyst"].contains(&s.role.as_str()) {
        s.role = "consumer".into();
    }
    s.display_currency = s.display_currency.trim().to_uppercase();
    if s.display_currency.len() != 3 {
        s.display_currency = "CNY".into();
    }
    let q = &mut s.search;
    q.max_queries = q.max_queries.clamp(1, 3);
    q.per_platform = q.per_platform.clamp(6, 60);
    q.concurrency = q.concurrency.clamp(1, 4);
    q.login_wait_secs = q.login_wait_secs.min(900);
    q.platform_timeout_secs = q.platform_timeout_secs.clamp(30, 600);
    q.result_limit = q.result_limit.clamp(5, 120);
    if !["all", "cn", "intl"].contains(&q.default_region.as_str()) {
        q.default_region = "all".into();
    }
    s.notify.drop_percent = s.notify.drop_percent.clamp(0.5, 90.0);
    db.put_setting(KEY, &s)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_update_keeps_other_fields() {
        let db = Db::memory().unwrap();
        let s = update(&db, serde_json::json!({"role": "merchant", "search": {"concurrency": 9}})).unwrap();
        assert_eq!(s.role, "merchant");
        assert_eq!(s.search.concurrency, 4, "clamped");
        assert_eq!(s.search.max_queries, 2, "untouched default kept");
        let s = update(&db, serde_json::json!({"notify": {"webhookKind": "feishu"}})).unwrap();
        assert_eq!(s.role, "merchant");
        assert_eq!(s.notify.webhook_kind, "feishu");
        assert!(update(&db, serde_json::json!({"search": {"concurrency": "x"}})).is_err());
    }
}
