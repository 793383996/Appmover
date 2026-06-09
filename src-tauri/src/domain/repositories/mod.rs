//! 领域层仓储接口集合。
//!
//! 命名"repository"是 DDD 术语,这里是抽象接口;
//! 真正的实现在 `infrastructure::repositories::*`。

pub mod app_repository;
pub mod drive_repository;
pub mod filesystem_probe;
pub mod migration_repository;
pub mod path_guard;
pub mod process_guard;
pub mod size_calculator;
pub mod state_store;

pub use app_repository::AppRepository;
pub use drive_repository::DriveRepository;
pub use filesystem_probe::{FilesystemProbe, StdFilesystemProbe};
pub use migration_repository::{CopyProgress, MigrationRepository};
pub use path_guard::{DefaultPathGuard, PathGuard};
pub use process_guard::ProcessGuard;
pub use size_calculator::{SizeCalculator, SizeProgress};
pub use state_store::StateStore;
