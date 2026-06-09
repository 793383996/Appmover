//! 业务用例集合。
//!
//! 每个 UseCase 单一职责、可独立单测、组合简单。
//! UseCase 之间**不直接**互相调用,通过 application 层编排。

pub mod calculate_size;
pub mod check_migration_preconditions;
pub mod detect_orphans;
pub mod list_drives;
pub mod list_migrated_apps;
pub mod migrate_app;
pub mod rollback_app;
pub mod scan_apps;

pub use calculate_size::CalculateSizeUseCase;
pub use check_migration_preconditions::CheckMigrationPreconditionsUseCase;
pub use detect_orphans::{DetectOrphansUseCase, OrphanInfo, OrphanKind};
pub use list_drives::ListDrivesUseCase;
pub use list_migrated_apps::ListMigratedAppsUseCase;
pub use migrate_app::MigrateAppUseCase;
pub use rollback_app::RollbackAppUseCase;
pub use scan_apps::ScanAppsUseCase;
