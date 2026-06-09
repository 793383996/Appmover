//! 依赖注入容器。
//!
//! 启动时一次性 wire 所有 UseCase + Repository + 平台实现;
//! 之后通过 `tauri::Builder::manage(Arc<AppDeps>)` 注册,命令层用 `tauri::State` 取出。
//!
//! 设计原则:
//! - 容器是单例(`Arc<AppDeps>`)
//! - 替换实现只换 `build_with` 这一处
//! - 测试时注入 mock

use crate::domain::repositories::{
    AppRepository, DefaultPathGuard, DriveRepository, MigrationRepository, PathGuard,
    ProcessGuard, SizeCalculator, StateStore, StdFilesystemProbe,
};
use crate::domain::usecases::{
    CalculateSizeUseCase, CheckMigrationPreconditionsUseCase, DetectOrphansUseCase,
    ListDrivesUseCase, ListMigratedAppsUseCase, MigrateAppUseCase, RollbackAppUseCase,
    ScanAppsUseCase,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppDeps {
    // ---- 仓储(平台门控) ----
    pub app_repo: Arc<dyn AppRepository>,
    pub drive_repo: Arc<dyn DriveRepository>,
    pub size_calc: Arc<dyn SizeCalculator>,
    pub process_guard: Arc<dyn ProcessGuard>,
    pub path_guard: Arc<dyn PathGuard>,
    pub migration_repo: Arc<dyn MigrationRepository>,
    pub state_store: Arc<dyn StateStore>,

    // ---- UseCase ----
    pub scan_apps: Arc<ScanAppsUseCase>,
    pub list_drives: Arc<ListDrivesUseCase>,
    pub calculate_size: Arc<CalculateSizeUseCase>,
    pub precheck: Arc<CheckMigrationPreconditionsUseCase>,
    pub migrate: Arc<MigrateAppUseCase>,
    pub rollback: Arc<RollbackAppUseCase>,
    pub list_migrated: Arc<ListMigratedAppsUseCase>,
    pub detect_orphans: Arc<DetectOrphansUseCase>,
}

impl AppDeps {
    /// 默认构造:用平台门控的最终实现 + 默认 PathGuard。
    pub fn build() -> Result<Arc<Self>, crate::shared::AppError> {
        use crate::infrastructure::repositories::{
            app_repository::AppRepositoryImpl, process_guard_impl::ProcessGuardImpl,
            DriveRepositoryImpl, MigrationRepositoryImpl, SizeCalculatorImpl, StateStoreImpl,
        };

        let app_repo: Arc<dyn AppRepository> = AppRepositoryImpl::new();
        let drive_repo: Arc<dyn DriveRepository> = DriveRepositoryImpl::new();
        let size_calc: Arc<dyn SizeCalculator> = SizeCalculatorImpl::new();
        let process_guard: Arc<dyn ProcessGuard> = ProcessGuardImpl::new();
        let path_guard: Arc<dyn PathGuard> = Arc::new(DefaultPathGuard::new());
        let migration_repo: Arc<dyn MigrationRepository> = MigrationRepositoryImpl::new();
        let state_store: Arc<dyn StateStore> = StateStoreImpl::new()?;

        Ok(Self::build_with(
            app_repo,
            drive_repo,
            size_calc,
            process_guard,
            path_guard,
            migration_repo,
            state_store,
        ))
    }

    /// 注入自定义实现(单测 / 特殊场景)。
    #[allow(clippy::too_many_arguments)]
    pub fn build_with(
        app_repo: Arc<dyn AppRepository>,
        drive_repo: Arc<dyn DriveRepository>,
        size_calc: Arc<dyn SizeCalculator>,
        process_guard: Arc<dyn ProcessGuard>,
        path_guard: Arc<dyn PathGuard>,
        migration_repo: Arc<dyn MigrationRepository>,
        state_store: Arc<dyn StateStore>,
    ) -> Arc<Self> {
        let scan_apps = Arc::new(ScanAppsUseCase::new(app_repo.clone(), path_guard.clone()));
        let list_drives = Arc::new(ListDrivesUseCase::new(drive_repo.clone()));
        let calculate_size = Arc::new(CalculateSizeUseCase::new(size_calc.clone()));
        let precheck = Arc::new(CheckMigrationPreconditionsUseCase::new(
            path_guard.clone(),
            process_guard.clone(),
            drive_repo.clone(),
            state_store.clone(),
        ));
        let migrate = Arc::new(MigrateAppUseCase::new(
            precheck.clone(),
            migration_repo.clone(),
            state_store.clone(),
        ));
        let rollback = Arc::new(RollbackAppUseCase::new(migration_repo.clone(), state_store.clone()));
        let list_migrated = Arc::new(ListMigratedAppsUseCase::new(state_store.clone()));
        let fs: Arc<dyn crate::domain::repositories::FilesystemProbe> = Arc::new(StdFilesystemProbe);
        let detect_orphans = Arc::new(DetectOrphansUseCase::new(state_store.clone(), fs));

        Arc::new(Self {
            app_repo,
            drive_repo,
            size_calc,
            process_guard,
            path_guard,
            migration_repo,
            state_store,
            scan_apps,
            list_drives,
            calculate_size,
            precheck,
            migrate,
            rollback,
            list_migrated,
            detect_orphans,
        })
    }
}
