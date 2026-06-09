//! 领域实体集合。
//!
//! 实体是带身份的可变业务对象,值对象是不可变无身份数据。
//! 这里 InstalledApp 用 AppId 作为身份,MigrationStatus 用 phase 表达可变性。

pub mod drive_info;
pub mod installed_app;
pub mod migration_plan;
pub mod migration_report;
pub mod migration_status;

pub use drive_info::DriveInfo;
pub use installed_app::{AppSource, InstalledApp};
pub use migration_plan::MigrationPlan;
pub use migration_report::MigrationReport;
pub use migration_status::{is_valid_transition, MigrationPhase, MigrationStatus};
