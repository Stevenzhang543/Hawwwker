//! 提醒：应用内通知（notices 表 + notice://new）、系统通知、群机器人（企业微信 / 飞书 / 钉钉 / Slack / Discord / 通用）。

use crate::db::{now_iso, Db};
use crate::error::AppResult;
use crate::models::{AppSettings, Notice};
use rusqlite::params;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub fn add(db: &Db, kind: &str, level: &str, title: &str, body: &str, link: &str) -> AppResult<Notice> {
    let created = now_iso();
    let c = db.conn();
    c.execute(
        "INSERT INTO notices(kind, level, title, body, link, read, created_at) VALUES(?1,?2,?3,?4,?5,0,?6)",
        params![kind, level, title, body, link, created],
    )?;
    Ok(Notice {
        id: c.last_insert_rowid(),
        kind: kind.into(),
        level: level.into(),
        title: title.into(),
        body: body.into(),
        link: link.into(),
        read: false,
        created_at: created,
    })
}

pub fn list(db: &Db, limit: i64) -> AppResult<Vec<Notice>> {
    let c = db.conn();
    let mut stmt = c.prepare("SELECT id, kind, level, title, body, link, read, created_at FROM notices ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit], |r| {
        Ok(Notice {
            id: r.get(0)?,
            kind: r.get(1)?,
            level: r.get(2)?,
            title: r.get(3)?,
            body: r.get(4)?,
            link: r.get(5)?,
            read: r.get::<_, i64>(6)? != 0,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn unread(db: &Db) -> i64 {
    db.conn().query_row("SELECT COUNT(*) FROM notices WHERE read=0", [], |r| r.get(0)).unwrap_or(0)
}

/// 新提醒：落库 + 推送前端 +（按设置）系统通知 + 群机器人。
pub async fn push(app: &AppHandle, db: &Db, kind: &str, level: &str, title: &str, body: &str, link: &str) {
    let Ok(n) = add(db, kind, level, title, body, link) else { return };
    let _ = app.emit("notice://new", &n);
    let settings: AppSettings = db.setting("app_settings").unwrap_or_default();
    let important = matches!(kind, "price_drop" | "target_hit" | "new_items" | "login") || level == "error";
    if settings.notify.system && important {
        let _ = app.notification().builder().title(title).body(body).show();
    }
    if matches!(kind, "price_drop" | "target_hit" | "new_items") && settings.notify.webhook_kind != "none" && !settings.notify.webhook_url.is_empty() {
        if let Err(e) = webhook(&settings.notify.webhook_kind, &settings.notify.webhook_url, title, body).await {
            log::warn!("webhook failed: {e}");
        }
    }
}

pub async fn webhook(kind: &str, url: &str, title: &str, body: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("群机器人地址必须是 https".into());
    }
    let text = format!("Hawwwker · {title}\n{body}");
    let payload = match kind {
        "wecom" => json!({"msgtype": "text", "text": {"content": text}}),
        "feishu" => json!({"msg_type": "text", "content": {"text": text}}),
        "dingtalk" => json!({"msgtype": "text", "text": {"content": text}}),
        "slack" => json!({"text": text}),
        "discord" => json!({"content": text}),
        _ => json!({"title": title, "body": body, "source": "hawwwker"}),
    };
    let resp = reqwest::Client::new()
        .post(url)
        .timeout(std::time::Duration::from_secs(10))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}
