//! AI 客户端：OpenAI 兼容（DeepSeek / 通义 / Kimi / GLM / OpenAI / Gemini / Ollama）与 Anthropic 原生协议。
//! 提供普通对话、JSON 输出（容错 + 一次纠错重试）、流式输出、用量统计与并发上限。

pub mod copilot;
pub mod planner;
pub mod presets;
pub mod prompts;
pub mod verify;

use crate::db::{now_iso, Db};
use crate::error::{AppError, AppResult};
use crate::models::{AiConfig, AiConfigInput, AiStatus};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

const KEY_SECRET: &str = "ai_api_key";

#[derive(Debug, Clone)]
pub struct Msg {
    pub role: &'static str,
    pub content: String,
}

impl Msg {
    pub fn system(c: impl Into<String>) -> Self {
        Msg { role: "system", content: c.into() }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Msg { role: "user", content: c.into() }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Msg { role: "assistant", content: c.into() }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Opts {
    /// 使用快速模型（抽取/校验）
    pub fast: bool,
    pub json: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone)]
struct Resolved {
    protocol: String,
    url: String,
    key: String,
    model: String,
    temperature: f64,
    timeout: Duration,
}

pub struct AiClient {
    db: Db,
    http: reqwest::Client,
    sem: RwLock<Arc<Semaphore>>,
}

fn is_local(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("://localhost") || u.contains("://127.0.0.1") || u.contains("://0.0.0.0") || u.contains("://[::1]")
}

fn endpoint(protocol: &str, base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if protocol == "anthropic" {
        let b = if b.is_empty() { "https://api.anthropic.com" } else { b };
        if b.ends_with("/messages") {
            b.to_string()
        } else if b.ends_with("/v1") {
            format!("{b}/messages")
        } else {
            format!("{b}/v1/messages")
        }
    } else if b.ends_with("/chat/completions") {
        b.to_string()
    } else {
        format!("{b}/chat/completions")
    }
}

fn friendly_http(status: u16, body: &str) -> String {
    let snippet: String = body.chars().take(180).collect();
    let why = match status {
        400 => "请求被拒绝（模型名或参数不被支持）",
        401 | 403 => "API Key 无效、已过期或没有该模型的权限",
        404 => "接口地址或模型不存在，请检查 Base URL 与模型名",
        408 | 504 => "模型服务响应超时",
        429 => "请求过于频繁或账户额度不足",
        500..=599 => "模型服务暂时不可用",
        _ => "模型服务返回错误",
    };
    format!("{why}（HTTP {status}）{}", if snippet.is_empty() { String::new() } else { format!("：{snippet}") })
}

/// 取出模型返回的列表：既接受顶层数组，也接受 {"items": [...]} 或只有一个数组字段的对象。
pub fn items_of(v: serde_json::Value) -> Vec<serde_json::Value> {
    match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(mut m) => {
            if let Some(serde_json::Value::Array(a)) = m.remove("items") {
                return a;
            }
            m.into_iter().find_map(|(_, x)| if let serde_json::Value::Array(a) = x { Some(a) } else { None }).unwrap_or_default()
        }
        _ => vec![],
    }
}

/// 从模型输出中提取第一个完整的 JSON 对象或数组（容忍代码围栏与前后说明文字）。
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find(['{', '['])?;
    let bytes = text.as_bytes();
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

impl AiClient {
    pub fn new(db: Db, http: reqwest::Client) -> Self {
        let max = db.setting::<AiConfig>("ai_config").map(|c| c.max_concurrency).unwrap_or(3).clamp(1, 8);
        AiClient { db, http, sem: RwLock::new(Arc::new(Semaphore::new(max as usize))) }
    }

    /* ------------------------------------------------------------ 配置 */
    pub fn config(&self) -> AiConfig {
        let mut c: AiConfig = self.db.setting("ai_config").unwrap_or_default();
        let key = self.db.secret(KEY_SECRET);
        c.has_key = key.is_some();
        c.key_hint = key
            .map(|k| {
                let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
                format!("••••{tail}")
            })
            .unwrap_or_default();
        c
    }

    pub fn configured(&self) -> bool {
        let c = self.config();
        !c.base_url.trim().is_empty() && !c.model.trim().is_empty() && (c.has_key || is_local(&c.base_url))
    }

    pub fn status(&self) -> AiStatus {
        let c = self.config();
        let mut s: AiStatus = self.db.setting("ai_status").unwrap_or_default();
        s.configured = self.configured();
        s.model = c.model.clone();
        s.provider = c.provider.clone();
        if !s.configured {
            s.verified = false;
        }
        s
    }

    /// AI 必选：未配置或未通过测试时拒绝业务调用。
    pub fn require(&self) -> AppResult<()> {
        let s = self.status();
        if s.configured && s.verified {
            Ok(())
        } else {
            Err(AppError::ai_required())
        }
    }

    pub fn save_config(&self, input: AiConfigInput) -> AppResult<AiConfig> {
        let protocol = if input.protocol == "anthropic" { "anthropic" } else { "openai" };
        let base = input.base_url.trim().to_string();
        if !base.is_empty() && !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(AppError::invalid("Base URL 需要以 http:// 或 https:// 开头"));
        }
        let old = self.config();
        let cfg = AiConfig {
            protocol: protocol.into(),
            provider: input.provider.trim().to_string(),
            base_url: base,
            model: input.model.trim().to_string(),
            fast_model: input.fast_model.unwrap_or_default().trim().to_string(),
            temperature: input.temperature.unwrap_or(old.temperature).clamp(0.0, 1.5),
            timeout_secs: input.timeout_secs.unwrap_or(old.timeout_secs).clamp(10, 300),
            max_concurrency: input.max_concurrency.unwrap_or(old.max_concurrency).clamp(1, 8),
            has_key: false,
            key_hint: String::new(),
        };
        let mut key_changed = false;
        if let Some(k) = input.api_key {
            key_changed = self.db.secret(KEY_SECRET).as_deref() != Some(k.trim());
            self.db.put_secret(KEY_SECRET, k.trim())?;
        }
        self.db.put_setting("ai_config", &cfg)?;
        // 配置变化后必须重新测试
        let changed = key_changed || old.base_url != cfg.base_url || old.model != cfg.model || old.protocol != cfg.protocol;
        let mut st: AiStatus = self.db.setting("ai_status").unwrap_or_default();
        if changed || !self.configured() {
            st.verified = false;
            st.last_error = None;
        }
        self.db.put_setting("ai_status", &st)?;
        if let Ok(mut w) = self.sem.write() {
            *w = Arc::new(Semaphore::new(cfg.max_concurrency as usize));
        }
        Ok(self.config())
    }

    pub async fn test(&self) -> AiStatus {
        let started = Instant::now();
        let res = self
            .chat(
                &[Msg::system("你是连接测试助手。"), Msg::user("请只回复这几个字：Hawwwker 已连接")],
                Opts { max_tokens: Some(40), temperature: Some(0.0), ..Default::default() },
            )
            .await;
        let c = self.config();
        let mut st = AiStatus {
            configured: self.configured(),
            model: c.model.clone(),
            provider: c.provider.clone(),
            ..Default::default()
        };
        match res {
            Ok(text) if !text.trim().is_empty() => {
                st.verified = true;
                st.verified_at = Some(now_iso());
                st.latency_ms = Some(started.elapsed().as_millis() as u64);
                st.sample = Some(text.trim().chars().take(80).collect());
            }
            Ok(_) => {
                st.verified = false;
                st.last_error = Some("模型返回了空内容".into());
            }
            Err(e) => {
                st.verified = false;
                st.last_error = Some(e.message);
            }
        }
        let _ = self.db.put_setting("ai_status", &st);
        st
    }

    fn resolve(&self, fast: bool) -> AppResult<Resolved> {
        if !self.configured() {
            return Err(AppError::ai_required());
        }
        let c = self.config();
        let model = if fast && !c.fast_model.trim().is_empty() { c.fast_model.clone() } else { c.model.clone() };
        Ok(Resolved {
            protocol: c.protocol.clone(),
            url: endpoint(&c.protocol, &c.base_url),
            key: self.db.secret(KEY_SECRET).unwrap_or_else(|| "ollama".into()),
            model,
            temperature: c.temperature,
            timeout: Duration::from_secs(c.timeout_secs.max(10)),
        })
    }

    fn body(&self, r: &Resolved, msgs: &[Msg], o: &Opts, stream: bool, with_format: bool) -> Value {
        let temperature = o.temperature.unwrap_or(r.temperature);
        if r.protocol == "anthropic" {
            let system: Vec<&str> = msgs.iter().filter(|m| m.role == "system").map(|m| m.content.as_str()).collect();
            let mut list: Vec<Value> = vec![];
            for m in msgs.iter().filter(|m| m.role != "system") {
                list.push(json!({"role": m.role, "content": m.content}));
            }
            if o.json {
                // Anthropic 没有 JSON 模式：用预填 "{" / "[" 不可靠，改为在 system 中强约束
            }
            let mut b = json!({
                "model": r.model, "max_tokens": o.max_tokens.unwrap_or(4096), "messages": list, "temperature": temperature,
            });
            if !system.is_empty() {
                b["system"] = json!(system.join("\n\n"));
            }
            if stream {
                b["stream"] = json!(true);
            }
            b
        } else {
            let list: Vec<Value> = msgs.iter().map(|m| json!({"role": m.role, "content": m.content})).collect();
            let mut b = json!({"model": r.model, "messages": list, "temperature": temperature});
            if let Some(mt) = o.max_tokens {
                b["max_tokens"] = json!(mt);
            }
            if o.json && with_format {
                b["response_format"] = json!({"type": "json_object"});
            }
            if stream {
                b["stream"] = json!(true);
            }
            b
        }
    }

    fn request(&self, r: &Resolved, body: &Value) -> reqwest::RequestBuilder {
        let mut req = self.http.post(&r.url).timeout(r.timeout).json(body);
        if r.protocol == "anthropic" {
            req = req.header("x-api-key", &r.key).header("anthropic-version", "2023-06-01");
        } else {
            req = req.bearer_auth(&r.key);
        }
        req
    }

    fn permit_source(&self) -> Arc<Semaphore> {
        self.sem.read().map(|s| s.clone()).unwrap_or_else(|_| Arc::new(Semaphore::new(3)))
    }

    /// 普通对话，返回文本。
    pub async fn chat(&self, msgs: &[Msg], o: Opts) -> AppResult<String> {
        let r = self.resolve(o.fast)?;
        let sem = self.permit_source();
        let _permit = sem.acquire().await.map_err(|_| AppError::internal("AI 并发控制已关闭"))?;
        let mut with_format = true;
        for attempt in 0..3 {
            let body = self.body(&r, msgs, &o, false, with_format);
            let resp = match self.request(&r, &body).send().await {
                Ok(x) => x,
                Err(e) if attempt < 1 && (e.is_connect() || e.is_request()) => {
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    continue;
                }
                Err(e) => {
                    return Err(AppError::ai(if e.is_timeout() {
                        "连接模型服务超时，请检查网络或增大超时时间".to_string()
                    } else {
                        format!("无法连接模型服务：{e}")
                    }))
                }
            };
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            if status == 400 && o.json && with_format && r.protocol == "openai" {
                with_format = false; // 部分兼容服务不支持 response_format
                continue;
            }
            if status == 429 && attempt < 2 {
                tokio::time::sleep(Duration::from_millis(1500 * (attempt + 1) as u64)).await;
                continue;
            }
            if status >= 400 {
                return Err(AppError::ai(friendly_http(status, &text)));
            }
            let v: Value = serde_json::from_str(&text).map_err(|_| AppError::ai("模型返回的内容无法解析"))?;
            let (content, pt, ct) = if r.protocol == "anthropic" {
                let content = v["content"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
                    .unwrap_or_default();
                (content, v["usage"]["input_tokens"].as_i64().unwrap_or(0), v["usage"]["output_tokens"].as_i64().unwrap_or(0))
            } else {
                let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
                (content, v["usage"]["prompt_tokens"].as_i64().unwrap_or(0), v["usage"]["completion_tokens"].as_i64().unwrap_or(0))
            };
            self.db.add_usage(pt, ct);
            // 推理模型可能把思考过程放在 <think> 中
            let cleaned = strip_think(&content);
            return Ok(cleaned);
        }
        Err(AppError::ai("模型服务多次重试仍失败"))
    }

    /// JSON 输出：解析失败时把错误回传给模型重试一次。
    pub async fn chat_json<T: DeserializeOwned>(&self, system: &str, user: &str, fast: bool) -> AppResult<T> {
        let sys = format!("{system}\n\n只输出 JSON，不要输出任何解释、注释或代码围栏。");
        let mut msgs = vec![Msg::system(sys), Msg::user(user)];
        let o = Opts { fast, json: true, max_tokens: Some(4096), temperature: Some(0.1) };
        let mut last_err = String::new();
        for _ in 0..2 {
            let text = self.chat(&msgs, o).await?;
            match extract_json(&text).map(serde_json::from_str::<T>) {
                Some(Ok(v)) => return Ok(v),
                Some(Err(e)) => last_err = e.to_string(),
                None => last_err = "没有找到 JSON".into(),
            }
            msgs.push(Msg::assistant(text));
            msgs.push(Msg::user(format!("上面的输出无法解析（{last_err}）。请严格按要求只输出合法 JSON。")));
        }
        Err(AppError::ai(format!("模型没有返回合法 JSON：{last_err}")))
    }

    /// 流式输出：每收到一段文本调用 on_delta，返回完整文本。
    pub async fn stream<F: FnMut(&str)>(&self, msgs: &[Msg], o: Opts, mut on_delta: F) -> AppResult<String> {
        let r = self.resolve(o.fast)?;
        let sem = self.permit_source();
        let _permit = sem.acquire().await.map_err(|_| AppError::internal("AI 并发控制已关闭"))?;
        let body = self.body(&r, msgs, &o, true, false);
        let resp = self.request(&r, &body).timeout(r.timeout * 3).send().await.map_err(|e| {
            AppError::ai(if e.is_timeout() { "连接模型服务超时".into() } else { format!("无法连接模型服务：{e}") })
        })?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::ai(friendly_http(status, &text)));
        }
        let mut full = String::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        let mut in_think = false;
        let (mut pt, mut ct) = (0i64, 0i64);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::ai(format!("流式输出中断：{e}")))?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let raw: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw).trim().to_string();
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                let piece = if r.protocol == "anthropic" {
                    if v["type"] == "message_start" {
                        pt = v["message"]["usage"]["input_tokens"].as_i64().unwrap_or(0);
                    }
                    if v["type"] == "message_delta" {
                        ct = v["usage"]["output_tokens"].as_i64().unwrap_or(ct);
                    }
                    if v["type"] == "content_block_delta" { v["delta"]["text"].as_str().unwrap_or("").to_string() } else { String::new() }
                } else {
                    if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
                        pt = u["prompt_tokens"].as_i64().unwrap_or(pt);
                        ct = u["completion_tokens"].as_i64().unwrap_or(ct);
                    }
                    v["choices"][0]["delta"]["content"].as_str().unwrap_or("").to_string()
                };
                if piece.is_empty() {
                    continue;
                }
                // 过滤推理模型的 <think>…</think>
                let mut visible = String::new();
                let mut rest = piece.as_str();
                loop {
                    if in_think {
                        if let Some(i) = rest.find("</think>") {
                            in_think = false;
                            rest = &rest[i + 8..];
                        } else {
                            break;
                        }
                    } else if let Some(i) = rest.find("<think>") {
                        visible.push_str(&rest[..i]);
                        in_think = true;
                        rest = &rest[i + 7..];
                    } else {
                        visible.push_str(rest);
                        break;
                    }
                }
                if !visible.is_empty() {
                    full.push_str(&visible);
                    on_delta(&visible);
                }
            }
        }
        if ct == 0 {
            ct = (full.chars().count() as i64) / 2;
        }
        self.db.add_usage(pt, ct);
        Ok(full)
    }
}

fn strip_think(s: &str) -> String {
    let mut out = s.to_string();
    while let (Some(a), Some(b)) = (out.find("<think>"), out.find("</think>")) {
        if b < a {
            break;
        }
        out.replace_range(a..b + 8, "");
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_extraction() {
        assert_eq!(extract_json("```json\n{\"a\": [1, 2]}\n```"), Some("{\"a\": [1, 2]}"));
        assert_eq!(extract_json("结果如下：[{\"i\":1,\"t\":\"x}\"}] 以上"), Some("[{\"i\":1,\"t\":\"x}\"}]"));
        assert_eq!(extract_json("no json"), None);
        assert_eq!(extract_json("{\"a\": \"\\\"}\"}"), Some("{\"a\": \"\\\"}\"}"));
    }

    #[test]
    fn items_wrapping() {
        assert_eq!(items_of(serde_json::json!([1, 2])).len(), 2);
        assert_eq!(items_of(serde_json::json!({"items": [1]})).len(), 1);
        assert_eq!(items_of(serde_json::json!({"results": [1, 2, 3]})).len(), 3);
        assert!(items_of(serde_json::json!({"x": 1})).is_empty());
    }

    #[test]
    fn endpoints() {
        assert_eq!(endpoint("openai", "https://api.deepseek.com/v1/"), "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(endpoint("anthropic", "https://api.anthropic.com"), "https://api.anthropic.com/v1/messages");
        assert_eq!(endpoint("anthropic", "https://x.y/v1"), "https://x.y/v1/messages");
        assert_eq!(endpoint("openai", "http://localhost:11434/v1"), "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn think_is_stripped() {
        assert_eq!(strip_think("<think>abc</think>\n答案"), "答案");
    }

    #[test]
    fn gate_requires_verification() {
        let db = Db::memory().unwrap();
        let ai = AiClient::new(db.clone(), reqwest::Client::new());
        assert!(ai.require().is_err());
        ai.save_config(AiConfigInput {
            protocol: "openai".into(), provider: "ollama".into(), base_url: "http://localhost:11434/v1".into(),
            model: "qwen2.5".into(), fast_model: None, temperature: None, timeout_secs: None, max_concurrency: None, api_key: None,
        })
        .unwrap();
        assert!(ai.configured());
        assert!(ai.require().is_err(), "configured but not verified must still be gated");
    }
}
