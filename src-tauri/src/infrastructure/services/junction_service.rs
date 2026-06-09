//! Junction/Symlink 服务 —— 平台门控的薄包装。
//!
//! - Windows:`infrastructure::platform::windows::junction::create_junction`
//! - 其他:返回明确错误

use crate::domain::value_objects::AppPath;
use crate::shared::AppResult;

pub struct JunctionService;

impl Default for JunctionService {
    fn default() -> Self {
        Self::new()
    }
}

impl JunctionService {
    pub fn new() -> Self {
        Self
    }

    pub fn create(&self, link: &AppPath, target: &AppPath) -> AppResult<()> {
        #[cfg(windows)]
        {
            crate::infrastructure::platform::windows::junction::create_junction(link, target)
        }
        #[cfg(not(windows))]
        {
            crate::infrastructure::platform::mock::junction::create_junction(link, target)
        }
    }

    pub fn remove(&self, link: &AppPath) -> AppResult<()> {
        // junction 是目录 reparse point,用 remove_dir
        std::fs::remove_dir(link.as_path()).map_err(|e| crate::shared::AppError::Io {
            path: link.as_path().to_path_buf(),
            source: e,
        })
    }
}
