//! 盘符值对象。Windows: `C:`,`D:`,macOS: `/`,Linux: `/mnt/data` 形式。
//!
//! 实际生产只在 Windows 上跑,macOS 上用 `/` 兜底。

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DriveLetter(String);

impl DriveLetter {
    #[cfg(windows)]
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        // Windows 盘符形如 "C:\"
        let s = path.to_string_lossy();
        if s.len() >= 2 && s.as_bytes()[1] == b':' {
            let c = s.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphabetic() {
                return Some(Self(format!("{}:", c)));
            }
        }
        None
    }

    #[cfg(not(windows))]
    pub fn from_path(_path: &std::path::Path) -> Option<Self> {
        Some(Self("/".into()))
    }

    /// 兜底构造:用任意字符串(给 infrastructure 层解析失败时用)。
    pub fn raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DriveLetter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
