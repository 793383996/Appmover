//! 应用层统一错误类型。
//!
//! 分层错误模型:
//! - `Domain`:领域层校验错误
//! - `Infrastructure`:基础设施层错误(IO / OS / 注册表)
//! - `Application`:应用层编排错误
//! - `Presentation`:表现层错误(序列化、IPC 协议)
//!
//! 各层在边界处 `From` 转换,内部不混合。

use std::path::PathBuf;
use thiserror::Error;

/// 应用层统一错误。
#[derive(Debug, Error)]
pub enum AppError {
    // ---- Domain ----
    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("path guard violation: {reason}")]
    PathGuardViolation { reason: String },

    #[error("app not found: {0}")]
    AppNotFound(String),

    #[error("invalid state transition: from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    // ---- Infrastructure ----
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("registry error: {0}")]
    Registry(String),

    #[error("junction/symlink error: {0}")]
    Link(String),

    #[error("process in use: {0:?}")]
    ProcessInUse(Vec<String>),

    #[error("insufficient disk space: need {need} bytes, have {have} bytes")]
    InsufficientSpace { need: u64, have: u64 },

    // ---- Application ----
    #[error("use case cancelled")]
    Cancelled,

    #[error("use case failed: {0}")]
    UseCase(String),

    #[error("dependency injection error: {0}")]
    Di(String),

    // ---- Presentation ----
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    // ---- External ----
    #[error("tauri error: {0}")]
    Tauri(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ---- 序列化给前端 ----
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 3)?;
        st.serialize_field("category", self.category())?;
        st.serialize_field("message", &self.to_string())?;
        st.serialize_field("can_rollback", &self.can_rollback())?;
        st.end()
    }
}

impl<'de> serde::Deserialize<'de> for AppError {
    fn deserialize<D: serde::Deserializer<'de>>(_d: D) -> Result<Self, D::Error> {
        // 反序列化只用于读取 state.json 中可能的错误快照,这里简化为返回 UseCase 错误
        Ok(AppError::UseCase("deserialized error".into()))
    }
}

impl AppError {
    /// 是否可回滚。
    pub fn can_rollback(&self) -> bool {
        matches!(
            self,
            AppError::Link(_)
                | AppError::ProcessInUse(_)
                | AppError::InsufficientSpace { .. }
                | AppError::Io { .. }
        )
    }

    /// 错误分类(给前端做埋点 / UI 分流用)。
    pub fn category(&self) -> &'static str {
        match self {
            AppError::InvalidPath(_) | AppError::AppNotFound(_) => "domain",
            AppError::PathGuardViolation { .. } | AppError::InvalidStateTransition { .. } => "domain",
            AppError::Io { .. }
            | AppError::Registry(_)
            | AppError::Link(_)
            | AppError::ProcessInUse(_)
            | AppError::InsufficientSpace { .. } => "infrastructure",
            AppError::Cancelled | AppError::UseCase(_) | AppError::Di(_) => "application",
            AppError::Serialization(_) => "presentation",
            AppError::Tauri(_) | AppError::Other(_) => "external",
        }
    }
}

// ---------- From impls ----------

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io {
            path: PathBuf::new(),
            source: e,
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        AppError::Link(e.message())
    }
}

#[cfg(windows)]
impl From<winreg::types::FromRegError> for AppError {
    fn from(e: winreg::types::FromRegError) -> Self {
        AppError::Registry(e.to_string())
    }
}

#[cfg(windows)]
impl From<winreg::types::ToRegError> for AppError {
    fn from(e: winreg::types::ToRegError) -> Self {
        AppError::Registry(e.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::UseCase(format!("task join: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
