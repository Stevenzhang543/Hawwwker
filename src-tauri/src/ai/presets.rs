//! AI 服务预设。模型 ID 于 2026-11-02 按各服务商官方文档核对；服务商会持续更新，
//! 界面提供「获取模型列表」（GET /models）并允许自由填写。
//!
//! retired：已停用或即将停用、不再推荐的模型 ID（界面提示更换；真正停服的会在启动时自动迁移，见 `ai::migrate_retired`）。

use crate::models::AiPreset;

#[allow(clippy::too_many_arguments)]
fn p(id: &str, name: &str, protocol: &str, base: &str, models: &[&str], fast: Option<&str>, key_url: &str, note: &str, region: &str, retired: &[&str]) -> AiPreset {
    AiPreset {
        id: id.into(),
        name: name.into(),
        protocol: protocol.into(),
        base_url: base.into(),
        models: models.iter().map(|s| s.to_string()).collect(),
        fast_model: fast.map(|s| s.to_string()),
        key_url: key_url.into(),
        note: note.into(),
        region: region.into(),
        retired: retired.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn list() -> Vec<AiPreset> {
    vec![
        p(
            "deepseek", "DeepSeek", "openai", "https://api.deepseek.com",
            &["deepseek-flash", "deepseek-v4-pro"], Some("deepseek-flash"),
            "https://platform.deepseek.com/api_keys",
            "deepseek-flash 始终指向最新 Flash；结构化任务自动关闭思考模式；推荐作为默认", "cn",
            &["deepseek-chat", "deepseek-reasoner", "deepseek-v4-flash"],
        ),
        p(
            "qwen", "通义千问（阿里云百炼）", "openai", "https://dashscope.aliyuncs.com/compatible-mode/v1",
            &["qwen3.8-max", "qwen3.7-plus", "qwen3.8-flash"], Some("qwen3.8-flash"),
            "https://bailian.console.aliyun.com/",
            "OpenAI 兼容模式；qwen-plus / qwen-max 为旧别名", "cn",
            &["qwen-turbo", "qwen-plus", "qwen-max", "qwen-flash"],
        ),
        p(
            "kimi", "Kimi（月之暗面）", "openai", "https://api.moonshot.cn/v1",
            &["kimi-k3.1-pro", "kimi-k3", "kimi-k3.1-pro-highspeed"], Some("kimi-k3"),
            "https://platform.moonshot.cn/console/api-keys",
            "国际账号用 https://api.moonshot.ai/v1；K2 系列年底下线", "cn",
            &["kimi-latest", "moonshot-v1-8k", "moonshot-v1-32k", "moonshot-v1-128k", "kimi-k2-turbo-preview", "kimi-k2.5", "kimi-k2.6"],
        ),
        p(
            "zhipu", "智谱 GLM", "openai", "https://open.bigmodel.cn/api/paas/v4",
            &["glm-5.3", "glm-5.2", "glm-4.7-flash"], Some("glm-4.7-flash"),
            "https://open.bigmodel.cn/usercenter/apikeys",
            "glm-4.7-flash 免费，可作快速模型", "cn",
            &["glm-4-flash", "glm-4-plus", "glm-4-air"],
        ),
        p(
            "openai", "OpenAI", "openai", "https://api.openai.com/v1",
            &["gpt-6.1-sol", "gpt-6-astra", "gpt-6.1-terra"], Some("gpt-6-luna"),
            "https://platform.openai.com/api-keys",
            "需要可访问 OpenAI 的网络；gpt-5 系列 2026-12-11 停用", "intl",
            &["gpt-5", "gpt-5-mini", "gpt-5-nano", "gpt-4.1-mini", "gpt-4o", "gpt-4o-mini"],
        ),
        p(
            "anthropic", "Anthropic Claude", "anthropic", "https://api.anthropic.com",
            &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"], Some("claude-haiku-5-5"),
            "https://console.anthropic.com/settings/keys",
            "原生 Messages 协议；模型 ID 为固定版本，新模型发布后需手动更换", "intl",
            &["claude-sonnet-4-5", "claude-haiku-4-5"],
        ),
        p(
            "gemini", "Google Gemini", "openai", "https://generativelanguage.googleapis.com/v1beta/openai",
            &["gemini-3.8-flash", "gemini-4-argon", "gemini-3.5-flash-lite"], Some("gemini-3.5-flash-lite"),
            "https://aistudio.google.com/apikey",
            "OpenAI 兼容端点；2.5 系列仅老账号可用", "intl",
            &["gemini-2.0-flash", "gemini-2.0-flash-lite", "gemini-2.5-flash", "gemini-2.5-pro", "gemini-3-pro-preview", "gemini-3-flash-preview"],
        ),
        p(
            "ollama", "Ollama（本机）", "openai", "http://localhost:11434/v1",
            &["qwen3.5:9b", "qwen3.8:27b", "qwen3.5:4b"], None,
            "https://ollama.com/library",
            "本机运行，无需 API Key；小模型的校验准确度较低", "local",
            &["qwen2.5:14b", "qwen2.5:7b", "llama3.1:8b"],
        ),
        p("custom", "自定义（OpenAI 兼容）", "openai", "", &[], None, "", "任何兼容 /chat/completions 的服务", "intl", &[]),
    ]
}

/// 已经停服、调用必然失败的模型 → 替代模型（仅指官方服务；中转 / 自建服务可能仍提供旧 ID）。
pub fn replacement_for(model: &str) -> Option<&'static str> {
    match model.trim() {
        // DeepSeek 2026-07-24 停用旧名
        "deepseek-chat" | "deepseek-reasoner" | "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp" => Some("deepseek-flash"),
        // Gemini 已关闭的型号
        "gemini-2.0-flash" | "gemini-2.0-flash-lite" | "gemini-3-flash-preview" => Some("gemini-3.8-flash"),
        "gemini-3-pro-preview" => Some("gemini-4-argon"),
        _ => None,
    }
}

/// 旧模型 ID 的官方服务域名：只有 Base URL 指向官方域名时才自动迁移。
fn official_host(model: &str) -> Option<&'static str> {
    let m = model.trim();
    if m.starts_with("deepseek") {
        Some("api.deepseek.com")
    } else if m.starts_with("gemini") {
        Some("generativelanguage.googleapis.com")
    } else {
        None
    }
}

/// 启动迁移用：模型已停服且 Base URL 是该模型的官方服务时，返回替代模型。
pub fn replacement_for_base(model: &str, base_url: &str) -> Option<&'static str> {
    let r = replacement_for(model)?;
    let host = base_url.split_once("://").map(|(_, rest)| rest).unwrap_or(base_url);
    let host = host.split(['/', '?', '#', ':']).next().unwrap_or("").trim().to_lowercase();
    (Some(host.as_str()) == official_host(model)).then_some(r)
}

/// 不再推荐（可能仍可调用）的模型，用于界面提醒。
pub fn is_retired(model: &str) -> bool {
    let m = model.trim();
    !m.is_empty() && (replacement_for(m).is_some() || list().iter().any(|p| p.retired.iter().any(|r| r == m)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_uses_current_ids() {
        let ds = list().into_iter().find(|p| p.id == "deepseek").unwrap();
        assert_eq!(ds.models, vec!["deepseek-flash", "deepseek-v4-pro"]);
        assert_eq!(ds.fast_model.as_deref(), Some("deepseek-flash"));
        assert!(ds.retired.contains(&"deepseek-chat".to_string()));
        assert_eq!(replacement_for("deepseek-chat"), Some("deepseek-flash"));
        assert_eq!(replacement_for("deepseek-reasoner"), Some("deepseek-flash"));
        assert_eq!(replacement_for("deepseek-v4-pro"), None);
        assert!(is_retired("gpt-5-mini") && !is_retired("gpt-6.1-sol"));
        assert_eq!(replacement_for_base("deepseek-chat", "https://api.deepseek.com/v1"), Some("deepseek-flash"));
        assert_eq!(replacement_for_base("deepseek-chat", "https://my-relay.example/v1"), None, "relay keeps old ID");
        assert_eq!(replacement_for_base("gemini-2.0-flash", "https://generativelanguage.googleapis.com/v1beta/openai"), Some("gemini-3.8-flash"));
        assert_eq!(replacement_for_base("gemini-2.0-flash", "https://openrouter.ai/api/v1"), None);
    }

    #[test]
    fn presets_are_consistent() {
        for p in list() {
            for m in &p.models {
                assert!(!p.retired.contains(m), "{} lists retired model {m}", p.id);
                assert!(replacement_for(m).is_none(), "{} recommends retired {m}", p.id);
            }
            if let Some(f) = &p.fast_model {
                assert!(!p.retired.contains(f));
            }
            assert!(p.id == "custom" || p.base_url.starts_with("http"));
        }
    }
}
