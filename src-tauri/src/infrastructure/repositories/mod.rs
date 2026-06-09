//! 基础设施层 · 仓储实现。
//!
//! 每个文件是 domain::repositories 某个 trait 的具体实现。
//! 平台门控在 `platform` 子模块内做,这里只做组合 + 公开 use。

pub mod drive_repository;
pub mod migration_repository;
pub mod size_calculator;
pub mod state_store;

// ---- 平台门控的 re-export ----
pub mod app_repository {
    //! `AppRepository` 的最终实现,按平台条件编译。
    #[cfg(windows)]
    pub use crate::infrastructure::platform::windows::registry::RegistryAppRepository as AppRepositoryImpl;
    #[cfg(not(windows))]
    pub use crate::infrastructure::platform::mock::registry::MockAppRepository as AppRepositoryImpl;
}

pub mod process_guard_impl {
    //! `ProcessGuard` 的最终实现,按平台条件编译。
    #[cfg(windows)]
    pub use crate::infrastructure::platform::windows::process::WindowsProcessGuard as ProcessGuardImpl;
    #[cfg(not(windows))]
    pub use crate::infrastructure::platform::mock::process::MockProcessGuard as ProcessGuardImpl;
}

pub use drive_repository::SysinfoDriveRepository as DriveRepositoryImpl;
pub use migration_repository::FileMigrationRepository as MigrationRepositoryImpl;
pub use size_calculator::WalkdirSizeCalculator as SizeCalculatorImpl;
pub use state_store::JsonStateStore as StateStoreImpl;
