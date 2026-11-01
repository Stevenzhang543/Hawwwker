//! AI 服务预设（模型名会随服务商更新，界面允许自由填写）。

use crate::models::AiPreset;

fn p(id: &str, name: &str, protocol: &str, base: &str, models: &[&str], fast: Option<&str>, key_url: &str, note: &str, region: &str) -> AiPreset {
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
    }
}

pub fn list() -> Vec<AiPreset> {
    vec![
        p("deepseek", "DeepSeek", "openai", "https://api.deepseek.com/v1", &["deepseek-chat", "deepseek-reasoner"], Some("deepseek-chat"),
          "https://platform.deepseek.com/api_keys", "性价比高，中文理解好；推荐作为默认", "cn"),
        p("qwen", "通义千问（阿里云百炼）", "openai", "https://dashscope.aliyuncs.com/compatible-mode/v1", &["qwen-plus", "qwen-max", "qwen-turbo"],
          Some("qwen-turbo"), "https://bailian.console.aliyun.com/", "OpenAI 兼容模式", "cn"),
        p("kimi", "Kimi（月之暗面）", "openai", "https://api.moonshot.cn/v1", &["kimi-latest", "moonshot-v1-32k", "moonshot-v1-8k"], None,
          "https://platform.moonshot.cn/console/api-keys", "模型名以控制台为准", "cn"),
        p("zhipu", "智谱 GLM", "openai", "https://open.bigmodel.cn/api/paas/v4", &["glm-4-plus", "glm-4-air", "glm-4-flash"], Some("glm-4-flash"),
          "https://open.bigmodel.cn/usercenter/apikeys", "glm-4-flash 可作快速模型", "cn"),
        p("openai", "OpenAI", "openai", "https://api.openai.com/v1", &["gpt-5", "gpt-5-mini", "gpt-4.1-mini"], Some("gpt-5-mini"),
          "https://platform.openai.com/api-keys", "需要可访问 OpenAI 的网络", "intl"),
        p("anthropic", "Anthropic Claude", "anthropic", "https://api.anthropic.com", &["claude-sonnet-4-5", "claude-haiku-4-5"],
          Some("claude-haiku-4-5"), "https://console.anthropic.com/settings/keys", "原生 Messages 协议；模型名以控制台为准", "intl"),
        p("gemini", "Google Gemini", "openai", "https://generativelanguage.googleapis.com/v1beta/openai", &["gemini-2.5-flash", "gemini-2.5-pro"],
          Some("gemini-2.5-flash"), "https://aistudio.google.com/apikey", "OpenAI 兼容端点", "intl"),
        p("ollama", "Ollama（本机）", "openai", "http://localhost:11434/v1", &["qwen2.5:14b", "qwen2.5:7b", "llama3.1:8b"], None,
          "https://ollama.com/library", "本机运行，无需 API Key；小模型的校验准确度较低", "local"),
        p("custom", "自定义（OpenAI 兼容）", "openai", "", &[], None, "", "任何兼容 /chat/completions 的服务", "intl"),
    ]
}
