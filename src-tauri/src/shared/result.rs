//! 应用层 Result 类型别名
//!
//! 统一错误处理,domain 层和应用层都使用 `AppResult<T>`。

use crate::shared::error::AppError;

/// 应用层统一的 `Result` 简写。
pub type AppResult<T> = Result<T, AppError>;
