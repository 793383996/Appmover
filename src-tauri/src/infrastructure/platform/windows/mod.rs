//! Windows 平台实现 —— `#[cfg(windows)]` 才编译。
//!
//! 提供:
//! - 注册表读取(RegistryAppRepository)
//! - Junction/Symlink 创建(WindowsLinkService)
//! - 进程占用(WindowsProcessGuard)
//! - 磁盘列表(WindowsDriveRepository)
//! - 路径 / 复制 / 状态持久化等

#[cfg(windows)]
pub mod junction;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod registry;
