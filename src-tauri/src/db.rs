//! SQLite（WAL）：单连接 + 互斥锁。所有时间以 RFC3339 UTC 文本存储，可直接按字典序比较。

use crate::error::AppResult;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn iso_in(minutes: i64) -> String {
    iso(Utc::now() + Duration::minutes(minutes))
}

pub fn iso_ago(days: i64) -> String {
    iso(Utc::now() - Duration::days(days))
}

pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS secrets(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS platform_overrides(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS login_meta(
        platform_id TEXT PRIMARY KEY, verified_at TEXT, last_wall_at TEXT, has_auth INTEGER DEFAULT 0,
        cookie_expires TEXT, updated_at TEXT);
    CREATE TABLE IF NOT EXISTS runs(
        id TEXT PRIMARY KEY, instruction TEXT NOT NULL DEFAULT '', plan TEXT NOT NULL DEFAULT '{}',
        status TEXT NOT NULL DEFAULT 'running', trigger TEXT NOT NULL DEFAULT 'manual', monitor_id INTEGER,
        started_at TEXT NOT NULL, finished_at TEXT, fetched INTEGER DEFAULT 0, kept INTEGER DEFAULT 0,
        error TEXT, summary TEXT, stats TEXT, best_price REAL, currency TEXT DEFAULT 'CNY');
    CREATE INDEX IF NOT EXISTS ix_runs_started ON runs(started_at);
    CREATE TABLE IF NOT EXISTS listings(
        id TEXT PRIMARY KEY, platform_id TEXT NOT NULL, external_id TEXT NOT NULL, title TEXT NOT NULL,
        url TEXT NOT NULL DEFAULT '', image TEXT, price REAL, currency TEXT NOT NULL DEFAULT 'CNY', original_price REAL,
        sales INTEGER, rating REAL, reviews INTEGER, shop TEXT, location TEXT, condition TEXT NOT NULL DEFAULT 'unknown',
        first_seen_at TEXT NOT NULL, last_seen_at TEXT NOT NULL, UNIQUE(platform_id, external_id));
    CREATE INDEX IF NOT EXISTS ix_listings_platform ON listings(platform_id);
    CREATE TABLE IF NOT EXISTS price_points(
        id INTEGER PRIMARY KEY AUTOINCREMENT, listing_id TEXT NOT NULL, price REAL NOT NULL, currency TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'search', captured_at TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS ix_pp_listing ON price_points(listing_id, captured_at);
    CREATE TABLE IF NOT EXISTS run_results(
        run_id TEXT NOT NULL, listing_id TEXT NOT NULL, rank INTEGER NOT NULL, score INTEGER NOT NULL,
        verdict TEXT NOT NULL, reasons TEXT NOT NULL DEFAULT '[]', flags TEXT NOT NULL DEFAULT '[]', cluster TEXT,
        ai_checked INTEGER NOT NULL DEFAULT 0, also_on TEXT NOT NULL DEFAULT '[]', display_price REAL,
        PRIMARY KEY(run_id, listing_id));
    CREATE TABLE IF NOT EXISTS watches(
        id INTEGER PRIMARY KEY AUTOINCREMENT, listing_id TEXT, platform_id TEXT NOT NULL, title TEXT NOT NULL,
        url TEXT NOT NULL, image TEXT, currency TEXT NOT NULL DEFAULT 'CNY', target_price REAL, last_price REAL,
        first_price REAL, lowest REAL, highest REAL, interval_minutes INTEGER NOT NULL DEFAULT 360,
        enabled INTEGER NOT NULL DEFAULT 1, last_checked_at TEXT, next_check_at TEXT, status TEXT NOT NULL DEFAULT 'new',
        last_error TEXT, fail_count INTEGER NOT NULL DEFAULT 0, note TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL,
        last_alert_price REAL);
    CREATE TABLE IF NOT EXISTS monitors(
        id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, instruction TEXT NOT NULL, plan TEXT NOT NULL,
        interval_minutes INTEGER NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, target_price REAL, last_run_at TEXT,
        next_run_at TEXT, last_status TEXT NOT NULL DEFAULT 'idle', last_new_count INTEGER NOT NULL DEFAULT 0,
        run_count INTEGER NOT NULL DEFAULT 0, last_run_id TEXT, best_price REAL, currency TEXT NOT NULL DEFAULT 'CNY',
        created_at TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS notices(
        id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, level TEXT NOT NULL, title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '', link TEXT NOT NULL DEFAULT '', read INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS copilot_messages(
        id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL,
        actions TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS ix_copilot_session ON copilot_messages(session_id, created_at);
    CREATE TABLE IF NOT EXISTS ai_usage(day TEXT PRIMARY KEY, calls INTEGER NOT NULL DEFAULT 0,
        prompt_tokens INTEGER NOT NULL DEFAULT 0, completion_tokens INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS ai_cache(key TEXT PRIMARY KEY, value TEXT NOT NULL, created_at TEXT NOT NULL);
    "#,
    // v2（v26.02）：到手价、商户售价、外部链接、"不相关"反馈
    r#"
    ALTER TABLE listings ADD COLUMN shipping REAL;
    ALTER TABLE listings ADD COLUMN coupon_price REAL;
    ALTER TABLE watches ADD COLUMN my_price REAL;
    ALTER TABLE notices ADD COLUMN url TEXT NOT NULL DEFAULT '';
    CREATE TABLE IF NOT EXISTS feedback(
        platform_id TEXT NOT NULL, external_id TEXT NOT NULL, scope TEXT NOT NULL, kind TEXT NOT NULL, created_at TEXT NOT NULL,
        PRIMARY KEY(platform_id, external_id, scope));
    "#,
];

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> AppResult<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        // WAL 对内存库无效，忽略返回值
        let _ = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0));
        conn.execute_batch("PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let target = (i + 1) as i64;
            if version < target {
                conn.execute_batch(sql)?;
                conn.execute_batch(&format!("PRAGMA user_version={target}"))?;
            }
        }
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // 线程在持锁期间 panic 时锁会"中毒"；SQLite 连接本身仍可用，直接恢复
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /* -------------------------------------------------------------- KV */
    pub fn get_json<T: DeserializeOwned>(&self, table: &str, key: &str) -> Option<T> {
        let sql = format!("SELECT value FROM {table} WHERE key=?1");
        let raw: Option<String> = self.conn().query_row(&sql, [key], |r| r.get(0)).optional().ok().flatten();
        raw.and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn put_json<T: Serialize>(&self, table: &str, key: &str, value: &T) -> AppResult<()> {
        let sql = format!("INSERT INTO {table}(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value");
        self.conn().execute(&sql, params![key, serde_json::to_string(value)?])?;
        Ok(())
    }

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get_json("settings", key)
    }

    pub fn put_setting<T: Serialize>(&self, key: &str, value: &T) -> AppResult<()> {
        self.put_json("settings", key, value)
    }

    pub fn secret(&self, key: &str) -> Option<String> {
        self.conn()
            .query_row("SELECT value FROM secrets WHERE key=?1", [key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
    }

    pub fn put_secret(&self, key: &str, value: &str) -> AppResult<()> {
        if value.is_empty() {
            self.conn().execute("DELETE FROM secrets WHERE key=?1", [key])?;
        } else {
            self.conn().execute(
                "INSERT INTO secrets(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )?;
        }
        Ok(())
    }

    /* -------------------------------------------------------------- AI cache / usage */
    pub fn cache_get(&self, key: &str, max_age_days: i64) -> Option<String> {
        let cutoff = iso_ago(max_age_days);
        self.conn()
            .query_row("SELECT value FROM ai_cache WHERE key=?1 AND created_at>=?2", params![key, cutoff], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn cache_put(&self, key: &str, value: &str) {
        let _ = self.conn().execute(
            "INSERT INTO ai_cache(key, value, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, created_at=excluded.created_at",
            params![key, value, now_iso()],
        );
    }

    pub fn add_usage(&self, prompt: i64, completion: i64) {
        let day = Utc::now().format("%Y-%m-%d").to_string();
        let _ = self.conn().execute(
            "INSERT INTO ai_usage(day, calls, prompt_tokens, completion_tokens) VALUES(?1, 1, ?2, ?3)
             ON CONFLICT(day) DO UPDATE SET calls=calls+1, prompt_tokens=prompt_tokens+excluded.prompt_tokens,
             completion_tokens=completion_tokens+excluded.completion_tokens",
            params![day, prompt, completion],
        );
    }

    /// 启动时收尾：上次异常退出遗留的 running 状态。
    pub fn recover_after_crash(&self) -> AppResult<usize> {
        let c = self.conn();
        let n = c.execute(
            "UPDATE runs SET status='interrupted', finished_at=COALESCE(finished_at, ?1),
             error=COALESCE(error, '程序在搜索完成前退出') WHERE status='running'",
            [now_iso()],
        )?;
        c.execute("UPDATE monitors SET last_status='idle' WHERE last_status='running'", [])?;
        c.execute("UPDATE watches SET status=CASE WHEN last_price IS NULL THEN 'new' ELSE 'ok' END WHERE status='checking'", [])?;
        let _ = c.execute("DELETE FROM ai_cache WHERE created_at < ?1", [iso_ago(14)]);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_and_kv() {
        let db = Db::memory().unwrap();
        db.put_setting("x", &serde_json::json!({"a": 1})).unwrap();
        let v: serde_json::Value = db.setting("x").unwrap();
        assert_eq!(v["a"], 1);
        db.put_secret("k", "s3cret").unwrap();
        assert_eq!(db.secret("k").as_deref(), Some("s3cret"));
        db.put_secret("k", "").unwrap();
        assert!(db.secret("k").is_none());
    }

    #[test]
    fn recover_marks_running_runs() {
        let db = Db::memory().unwrap();
        db.conn()
            .execute("INSERT INTO runs(id, started_at, status) VALUES('r1', ?1, 'running')", [now_iso()])
            .unwrap();
        assert_eq!(db.recover_after_crash().unwrap(), 1);
        let st: String = db.conn().query_row("SELECT status FROM runs WHERE id='r1'", [], |r| r.get(0)).unwrap();
        assert_eq!(st, "interrupted");
    }
}
