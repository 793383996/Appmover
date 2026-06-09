//! 文件系统探测抽象(给 orphan 检测用)。
//!
//! domain 层不应该直接 `std::fs`,而是通过 trait 注入,这样:
//! - 单元测试可以注入 mock
//! - 跨平台实现可以挂 Win32 API

use crate::domain::value_objects::AppPath;

pub trait FilesystemProbe: Send + Sync {
    /// 路径是否存在(文件或目录)。
    fn exists(&self, path: &AppPath) -> bool;
}

/// 默认实现:用 `std::path::Path::exists`。
pub struct StdFilesystemProbe;

impl FilesystemProbe for StdFilesystemProbe {
    fn exists(&self, path: &AppPath) -> bool {
        path.as_path().exists()
    }
}
