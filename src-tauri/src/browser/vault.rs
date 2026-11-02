//! 登录会话备份（v26.03）。
//!
//! 浏览器意外退出、被强制结束或会话 Cookie 没来得及写盘时，"明明登录了却搜不出来"。这里在登录验证通过、
//! 搜索 / 保活之后，把该平台域名下的 Cookie 加密保存到本机数据库；浏览器下次启动（或搜索前预检）发现
//! 该平台的强登录 Cookie 缺失时，把备份里尚未过期、且浏览器里没有的 Cookie 写回去。
//!
//! - 加密：Windows 用 DPAPI（只有当前 Windows 用户能解密）；其他系统用 ChaCha20-Poly1305，
//!   密钥是数据目录下权限 0600 的 `vault.key`。
//! - 只在本机使用，不上传、不出现在日志里；退出某平台或"清除全部登录"时一并删除。

use super::manager::{domain_match, Browser};
use crate::db::{now_iso, Db};
use crate::platforms::Platform;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// 备份写入与退出（删 Cookie + 删备份）互斥，避免"退出后备份又写回、下次启动被恢复"。
static VAULT_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
/// 浏览器启动钩子恢复过的平台（预检时取走并重新验证）。
static RESTORED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

pub async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    VAULT_LOCK.lock().await
}

/// 记录启动钩子恢复的平台。
pub fn note_restored(ids: &[String]) {
    if let Ok(mut r) = RESTORED.lock() {
        r.extend(ids.iter().cloned());
    }
}

/// 取走启动钩子恢复过、尚未重新验证的平台。
pub fn take_restored() -> Vec<String> {
    RESTORED.lock().map(|mut r| r.drain().collect()).unwrap_or_default()
}

/// 恢复的会话 Cookie 给 7 天有效期（与浏览器"继续上次浏览"保留会话 Cookie 的效果一致）。
const SESSION_RESTORE_DAYS: f64 = 7.0;

/* ------------------------------------------------------------------ 加密 */

#[cfg(windows)]
pub fn seal(plain: &[u8], _key_dir: &Path) -> Result<Vec<u8>, String> {
    let mut out = b"HW1".to_vec();
    out.extend(dpapi(plain, true)?);
    Ok(out)
}

#[cfg(windows)]
pub fn open(blob: &[u8], _key_dir: &Path) -> Result<Vec<u8>, String> {
    let body = blob.strip_prefix(b"HW1").ok_or("会话备份格式不对")?;
    dpapi(body, false)
}

#[cfg(windows)]
fn dpapi(data: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY：输入缓冲在调用期间有效；输出由系统分配，复制后用 LocalFree 释放。
    let ok = unsafe {
        if protect {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        }
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(if protect { "会话备份加密失败".into() } else { "会话备份无法解密（可能来自其他 Windows 用户）".into() });
    }
    let v = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(v)
}

#[cfg(not(windows))]
static KEY_LOCK: Mutex<()> = Mutex::new(());

#[cfg(not(windows))]
fn key(key_dir: &Path) -> Result<chacha20poly1305::Key, String> {
    use chacha20poly1305::aead::{KeyInit, OsRng};
    use chacha20poly1305::ChaCha20Poly1305;
    use std::io::Write;
    // 同一进程内串行；跨进程用 create_new，已存在就读取（不会出现两把密钥）
    let _g = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = key_dir.join("vault.key");
    let read = |path: &Path| -> Option<chacha20poly1305::Key> {
        std::fs::read(path).ok().filter(|b| b.len() == 32).map(|b| *chacha20poly1305::Key::from_slice(&b))
    };
    if let Some(k) = read(&path) {
        return Ok(k);
    }
    std::fs::create_dir_all(key_dir).map_err(|e| e.to_string())?;
    let k = ChaCha20Poly1305::generate_key(&mut OsRng);
    let tmp = key_dir.join(format!("vault.key.{}.tmp", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    {
        let mut f = opts.open(&tmp).map_err(|e| format!("无法创建会话备份密钥：{e}"))?;
        f.write_all(k.as_slice()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    // 原子地放到位：已有密钥（另一个进程先写好）时不覆盖，改用已有的
    match std::fs::hard_link(&tmp, &path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            Ok(k)
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            read(&path).ok_or_else(|| "会话备份密钥无法读取".to_string())
        }
    }
}

#[cfg(not(windows))]
pub fn seal(plain: &[u8], key_dir: &Path) -> Result<Vec<u8>, String> {
    use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
    use chacha20poly1305::ChaCha20Poly1305;
    let cipher = ChaCha20Poly1305::new(&key(key_dir)?);
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plain).map_err(|_| "会话备份加密失败".to_string())?;
    let mut out = b"HV1".to_vec();
    out.extend_from_slice(nonce.as_slice());
    out.extend(ct);
    Ok(out)
}

#[cfg(not(windows))]
pub fn open(blob: &[u8], key_dir: &Path) -> Result<Vec<u8>, String> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};
    let body = blob.strip_prefix(b"HV1").ok_or("会话备份格式不对")?;
    if body.len() < 12 + 16 {
        return Err("会话备份已损坏".into());
    }
    let (nonce, ct) = body.split_at(12);
    let cipher = ChaCha20Poly1305::new(&key(key_dir)?);
    cipher.decrypt(Nonce::from_slice(nonce), ct).map_err(|_| "会话备份无法解密（密钥已变化）".to_string())
}

/* ------------------------------------------------------------------ Cookie 选择 */

fn s<'a>(c: &'a Value, k: &str) -> &'a str {
    c.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

/// 平台域名下的 Cookie。
pub fn platform_cookies(cookies: &[Value], p: &Platform) -> Vec<Value> {
    cookies.iter().filter(|c| domain_match(s(c, "domain"), &p.cookie_domains)).cloned().collect()
}

/// 浏览器里是否有该平台的强登录 Cookie。
pub fn has_strong(cookies: &[Value], p: &Platform) -> bool {
    !p.auth_cookies.is_empty()
        && cookies.iter().any(|c| domain_match(s(c, "domain"), &p.cookie_domains) && p.auth_cookies.iter().any(|n| n == s(c, "name")) && !s(c, "value").is_empty())
}

fn key_of(c: &Value) -> (String, String, String) {
    (s(c, "name").to_string(), s(c, "domain").trim_start_matches('.').to_lowercase(), s(c, "path").to_string())
}

/// 需要写回浏览器的 Cookie：备份里有、浏览器里没有（按 名称+域名+路径），并且没过期。返回 CDP `Storage.setCookies` 参数。
pub fn restorable(saved: &[Value], current: &[Value], now_ts: f64) -> Vec<Value> {
    let have: std::collections::HashSet<(String, String, String)> = current.iter().map(key_of).collect();
    saved
        .iter()
        .filter(|c| !s(c, "name").is_empty() && !s(c, "domain").is_empty())
        .filter(|c| !have.contains(&key_of(c)))
        .filter_map(|c| {
            let session = c.get("session").and_then(|v| v.as_bool()).unwrap_or(false);
            let exp = c.get("expires").and_then(|v| v.as_f64()).unwrap_or(-1.0);
            let expires = if session || exp <= 0.0 { now_ts + SESSION_RESTORE_DAYS * 86400.0 } else { exp };
            if expires <= now_ts + 60.0 {
                return None;
            }
            Some(to_param(c, expires))
        })
        .collect()
}

/// CDP Cookie → `Storage.setCookies` 参数。
pub fn to_param(c: &Value, expires: f64) -> Value {
    let dom = s(c, "domain");
    let secure = c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false);
    let path = if s(c, "path").is_empty() { "/" } else { s(c, "path") };
    let mut p = json!({
        "name": s(c, "name"), "value": s(c, "value"), "path": path, "secure": secure,
        "httpOnly": c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false), "expires": expires,
    });
    if let Some(ss) = c.get("sameSite").filter(|v| v.is_string()) {
        p["sameSite"] = ss.clone();
    }
    if let Some(pk) = c.get("partitionKey").filter(|v| !v.is_null()) {
        p["partitionKey"] = pk.clone();
    }
    if dom.starts_with('.') {
        p["domain"] = json!(dom);
    } else {
        p["url"] = json!(format!("{}://{}{}", if secure { "https" } else { "http" }, dom, path));
    }
    p
}

/* ------------------------------------------------------------------ 存取 */

pub fn has(db: &Db, id: &str) -> bool {
    db.conn().query_row("SELECT 1 FROM login_vault WHERE platform_id=?1", [id], |_| Ok(())).optional().ok().flatten().is_some()
}

pub fn remove(db: &Db, id: &str) {
    let _ = db.conn().execute("DELETE FROM login_vault WHERE platform_id=?1", [id]);
}

pub fn clear_all(db: &Db) {
    let _ = db.conn().execute("DELETE FROM login_vault", []);
}

/// 保存平台会话（只有浏览器里确实有强登录 Cookie 时才保存）。返回保存的条数。
pub fn save(db: &Db, key_dir: &Path, p: &Platform, cookies: &[Value]) -> Result<usize, String> {
    if !has_strong(cookies, p) {
        return Ok(0);
    }
    let mine = platform_cookies(cookies, p);
    let blob = seal(serde_json::to_vec(&mine).map_err(|e| e.to_string())?.as_slice(), key_dir)?;
    db.conn()
        .execute(
            "INSERT INTO login_vault(platform_id, blob, cookie_count, saved_at) VALUES(?1,?2,?3,?4)
             ON CONFLICT(platform_id) DO UPDATE SET blob=excluded.blob, cookie_count=excluded.cookie_count, saved_at=excluded.saved_at",
            params![p.id, blob, mine.len() as i64, now_iso()],
        )
        .map_err(|e| e.to_string())?;
    Ok(mine.len())
}

pub fn load(db: &Db, key_dir: &Path, id: &str) -> Option<Vec<Value>> {
    let blob: Vec<u8> = db.conn().query_row("SELECT blob FROM login_vault WHERE platform_id=?1", [id], |r| r.get(0)).optional().ok().flatten()?;
    match open(&blob, key_dir) {
        Ok(plain) => serde_json::from_slice(&plain).ok(),
        Err(e) => {
            log::warn!("vault {id}: {e}");
            None
        }
    }
}

/// 根据当前浏览器 Cookie 计算需要写回的 Cookie（只处理强登录 Cookie 缺失的平台）。返回 (平台 id 列表, setCookies 参数)。
pub fn plan_restore(db: &Db, key_dir: &Path, platforms: &[Platform], current: &[Value]) -> (Vec<String>, Vec<Value>) {
    let now = chrono::Utc::now().timestamp() as f64;
    let mut ids = vec![];
    let mut params = vec![];
    for p in platforms.iter().filter(|p| !p.auth_cookies.is_empty()) {
        if has_strong(current, p) || !has(db, &p.id) {
            continue;
        }
        let Some(saved) = load(db, key_dir, &p.id) else { continue };
        // 备份里的强登录 Cookie 也已过期 → 不恢复（避免写回无效会话）
        let strong_valid = saved.iter().any(|c| {
            let session = c.get("session").and_then(|v| v.as_bool()).unwrap_or(false);
            let exp = c.get("expires").and_then(|v| v.as_f64()).unwrap_or(-1.0);
            domain_match(s(c, "domain"), &p.cookie_domains)
                && p.auth_cookies.iter().any(|n| n == s(c, "name"))
                && !s(c, "value").is_empty()
                && (session || exp <= 0.0 || exp > now + 60.0)
        });
        if !strong_valid {
            continue;
        }
        ids.push(p.id.clone());
        params.extend(restorable(&saved, current, now));
    }
    (ids, params)
}

/// 立即恢复：给预检、启动后使用。返回恢复了会话的平台。
pub async fn restore_missing(db: &Db, browser: &Browser, platforms: &[Platform]) -> Vec<String> {
    let Some(dir) = browser.data_dir() else { return vec![] };
    let Ok(current) = browser.cookies().await else { return vec![] };
    let (ids, params) = plan_restore(db, &dir, platforms, &current);
    if params.is_empty() {
        return vec![];
    }
    match browser.set_cookies(params).await {
        Ok(_) => {
            log::info!("vault restored sessions: {ids:?}");
            ids
        }
        Err(e) => {
            log::warn!("vault restore failed: {e}");
            vec![]
        }
    }
}

/// 备份这些平台的当前会话（浏览器在运行时）。返回已备份的平台。
/// 读 Cookie 与写备份在锁内完成，与退出互斥。调用方只应传入"已登录且没有正在登录"的平台。
pub async fn snapshot(db: &Db, browser: &Browser, platforms: &[&Platform]) -> Vec<String> {
    let Some(dir) = browser.data_dir() else { return vec![] };
    if platforms.is_empty() || !browser.alive().await {
        return vec![];
    }
    let _g = lock().await;
    let Ok(cookies) = browser.cookies().await else { return vec![] };
    platforms.iter().filter(|p| matches!(save(db, &dir, p, &cookies), Ok(n) if n > 0)).map(|p| p.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plat() -> Platform {
        Platform { id: "shop".into(), cookie_domains: vec!["shop.test".into()], auth_cookies: vec!["sid".into()], ..Default::default() }
    }

    #[test]
    fn seal_open_roundtrip_and_tamper() {
        let dir = std::env::temp_dir().join(format!("hw-vault-{}", uuid::Uuid::new_v4()));
        let blob = seal(b"secret cookies", &dir).unwrap();
        assert!(!blob.windows(6).any(|w| w == b"secret"), "ciphertext must not contain plaintext");
        assert_eq!(open(&blob, &dir).unwrap(), b"secret cookies");
        let mut bad = blob.clone();
        let n = bad.len() - 1;
        bad[n] ^= 0x55;
        assert!(open(&bad, &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_and_plan_restore() {
        let db = Db::memory().unwrap();
        let dir = std::env::temp_dir().join(format!("hw-vault-{}", uuid::Uuid::new_v4()));
        let p = plat();
        let future = chrono::Utc::now().timestamp() as f64 + 86400.0;
        let cookies = vec![
            json!({"name": "sid", "value": "ok-1", "domain": ".shop.test", "path": "/", "expires": future, "secure": true}),
            json!({"name": "guest", "value": "1", "domain": "shop.test", "path": "/", "expires": -1, "session": true}),
            json!({"name": "other", "value": "x", "domain": ".else.test", "path": "/", "expires": future}),
        ];
        // 没有强 Cookie 不保存
        assert_eq!(save(&db, &dir, &p, &cookies[1..]).unwrap(), 0);
        assert!(!has(&db, "shop"));
        assert_eq!(save(&db, &dir, &p, &cookies).unwrap(), 2, "only the platform's own domains");
        assert!(has(&db, "shop"));
        let loaded = load(&db, &dir, "shop").unwrap();
        assert_eq!(loaded.len(), 2);
        // 浏览器里强 Cookie 还在 → 不恢复
        let (ids, _) = plan_restore(&db, &dir, std::slice::from_ref(&p), &cookies);
        assert!(ids.is_empty());
        // 强 Cookie 丢失 → 恢复缺失的两条（会话 Cookie 给出有效期）
        let (ids, params) = plan_restore(&db, &dir, std::slice::from_ref(&p), &cookies[2..]);
        assert_eq!(ids, vec!["shop".to_string()]);
        assert_eq!(params.len(), 2);
        let guest = params.iter().find(|x| x["name"] == "guest").unwrap();
        assert!(guest["expires"].as_f64().unwrap() > future - 86400.0);
        assert_eq!(guest["url"], "http://shop.test/");
        remove(&db, "shop");
        assert!(!has(&db, "shop"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_strong_cookie_is_not_restored() {
        let db = Db::memory().unwrap();
        let dir = std::env::temp_dir().join(format!("hw-vault-{}", uuid::Uuid::new_v4()));
        let p = plat();
        let past = chrono::Utc::now().timestamp() as f64 - 10.0;
        let cookies = vec![json!({"name": "sid", "value": "old", "domain": ".shop.test", "path": "/", "expires": past})];
        save(&db, &dir, &p, &cookies).unwrap();
        let (ids, params) = plan_restore(&db, &dir, &[p], &[]);
        assert!(ids.is_empty() && params.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
