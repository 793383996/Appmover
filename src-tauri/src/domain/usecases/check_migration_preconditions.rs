//! `CheckMigrationPreconditionsUseCase` —— 迁移前多重校验。
//!
//! 1. 路径安全(PathGuard)
//! 2. 目标空间够(need × 1.05)
//! 3. 应用未在运行(ProcessGuard)
//! 4. 源路径未在迁移中(StateStore)

use crate::domain::entities::DriveInfo;
use crate::domain::repositories::{DriveRepository, PathGuard, ProcessGuard, StateStore};
use crate::domain::value_objects::AppPath;
use crate::shared::{AppError, AppResult};
use std::sync::Arc;

pub struct CheckMigrationPreconditionsUseCase {
    path_guard: Arc<dyn PathGuard>,
    process_guard: Arc<dyn ProcessGuard>,
    drive_repo: Arc<dyn DriveRepository>,
    #[allow(dead_code)]
    state_store: Arc<dyn StateStore>,
}

impl CheckMigrationPreconditionsUseCase {
    pub fn new(
        path_guard: Arc<dyn PathGuard>,
        process_guard: Arc<dyn ProcessGuard>,
        drive_repo: Arc<dyn DriveRepository>,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            path_guard,
            process_guard,
            drive_repo,
            state_store,
        }
    }

    pub async fn execute(
        &self,
        source: &AppPath,
        target: &AppPath,
        publisher: Option<&str>,
        needed_bytes: u64,
    ) -> AppResult<()> {
        // 1. 路径安全
        self.path_guard.is_critical(source, publisher)?;

        // 2. 进程占用
        let blockers = self.process_guard.find_blocking_processes(source).await?;
        if !blockers.is_empty() {
            return Err(AppError::ProcessInUse(blockers));
        }

        // 3. 目标盘空间
        let drives = self.drive_repo.list_all().await?;
        let target_drive = Self::find_drive_for(&drives, target)?;
        let need_with_buffer = (needed_bytes as f64 * 1.05) as u64;
        if target_drive.available.as_bytes() < need_with_buffer {
            return Err(AppError::InsufficientSpace {
                need: need_with_buffer,
                have: target_drive.available.as_bytes(),
            });
        }

        Ok(())
    }

    fn find_drive_for<'a>(drives: &'a [DriveInfo], path: &AppPath) -> AppResult<&'a DriveInfo> {
        let path_str = path.as_path().to_string_lossy();
        drives
            .iter()
            .find(|d| {
                let mount = d.mount_point.trim_end_matches('\\').trim_end_matches('/');
                path_str
                    .to_ascii_uppercase()
                    .starts_with(&mount.to_ascii_uppercase())
            })
            .ok_or_else(|| AppError::UseCase(format!("no drive for path: {}", path_str)))
    }
}
