//! 统一错误：序列化为 `{ code, message }` 交给前端。

use serde::Serialize;

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    /// v26.03：LOGIN_REQUIRED 时列出需要先登录的平台 id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), platforms: None }
    }
    /// v26.03：所选平台需要先登录
    pub fn login_required(names: &[String], ids: Vec<String>) -> Self {
        Self { code: "LOGIN_REQUIRED", message: format!("请先登录：{}（登录后再搜索）", names.join("、")), platforms: Some(ids) }
    }
    pub fn ai_required() -> Self {
        Self::new("AI_REQUIRED", "需要先连接 AI 模型：请到「设置 · AI 模型」填写并测试连接")
    }
    pub fn not_found(what: &str) -> Self {
        Self::new("NOT_FOUND", format!("{what}不存在或已被删除"))
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new("INVALID", msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new("INTERNAL", msg)
    }
    pub fn browser(msg: impl Into<String>) -> Self {
        Self::new("BROWSER", msg)
    }
    pub fn login(msg: impl Into<String>) -> Self {
        Self::new("LOGIN", msg)
    }
    pub fn ai(msg: impl Into<String>) -> Self {
        Self::new("AI", msg)
    }
    pub fn cancelled() -> Self {
        Self::new("CANCELLED", "已取消")
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::new("NOT_FOUND", "记录不存在"),
            other => AppError::internal(format!("数据库错误：{other}")),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::internal(format!("数据格式错误：{e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::internal(format!("文件读写失败：{e}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::internal(format!("{e}"))
    }
}
