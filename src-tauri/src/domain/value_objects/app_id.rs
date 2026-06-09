//! 应用 ID 值对象。
//!
//! 内部用 UUID 字符串,保证全局唯一,跨进程可序列化。

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// 业务层 ID,稳定不可变。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppId(String);

impl AppId {
    /// 生成新的 ID。
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// 从字符串创建(用于反序列化场景)。
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AppId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for AppId {
    fn from(s: &str) -> Self {
        Self::from_string(s)
    }
}
