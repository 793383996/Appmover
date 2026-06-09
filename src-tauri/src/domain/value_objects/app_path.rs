//! 路径值对象。
//!
//! 包装 `PathBuf`,在构造时做规范化(去除 `\\?\` 前缀、统一分隔符),
//! 提供 `is_absolute()` / `starts_with()` / `parent()` 等语义化方法。
//!
//! **不**做"系统路径保护"——那是 `path_guard` UseCase 的职责,值对象只保证格式正确。

use crate::shared::AppError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppPath(PathBuf);

impl AppPath {
    /// 从字符串创建,自动规范化。
    pub fn new(p: impl AsRef<Path>) -> Result<Self, AppError> {
        let raw = p.as_ref();
        if raw.as_os_str().is_empty() {
            return Err(AppError::InvalidPath("empty path".into()));
        }
        let normalized = Self::normalize(raw);
        if !Self::is_path_absolute(&normalized) {
            return Err(AppError::InvalidPath(format!(
                "path must be absolute: {}",
                normalized.display()
            )));
        }
        Ok(Self(normalized))
    }

    /// 跨平台"绝对"判断:Windows 盘符形式 + 类 Unix `/` 开头都算。
    fn is_path_absolute(p: &Path) -> bool {
        if p.is_absolute() {
            return true;
        }
        // Windows 盘符 `C:\` 或 `C:/` 形式
        let s = p.to_string_lossy();
        if s.len() >= 3 {
            let bytes = s.as_bytes();
            if bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/')
            {
                return true;
            }
        }
        false
    }

    /// 内部规范化:去除 Windows 扩展长度前缀 `\\?\`、统一为正斜杠在内存里(展示时还原)。
    fn normalize(p: &Path) -> PathBuf {
        let s = p.to_string_lossy();
        let trimmed = s.strip_prefix(r"\\?\").unwrap_or(&s);
        PathBuf::from(trimmed.replace('\\', "/"))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_inner(self) -> PathBuf {
        self.0
    }

    /// 父目录;根目录返回 `None`。
    pub fn parent(&self) -> Option<AppPath> {
        self.0.parent().and_then(|p| Self::new(p).ok())
    }

    /// 拼接子路径(语义化,不直接用 PathBuf::join 避免引入绝对路径覆盖)。
    pub fn join(&self, child: &str) -> AppPath {
        Self(self.0.join(child))
    }

    pub fn starts_with(&self, prefix: &AppPath) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

impl AsRef<Path> for AppPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for AppPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.display().to_string())
    }
}

impl TryFrom<&str> for AppPath {
    type Error = AppError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl TryFrom<String> for AppPath {
    type Error = AppError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}
