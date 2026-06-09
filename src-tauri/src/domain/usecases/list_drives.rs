//! `ListDrivesUseCase` —— 列出所有可作为迁移目标的盘符。

use crate::domain::entities::DriveInfo;
use crate::domain::repositories::DriveRepository;
use crate::shared::AppResult;
use std::cmp::Reverse;
use std::sync::Arc;

pub struct ListDrivesUseCase {
    drive_repo: Arc<dyn DriveRepository>,
}

impl ListDrivesUseCase {
    pub fn new(drive_repo: Arc<dyn DriveRepository>) -> Self {
        Self { drive_repo }
    }

    pub async fn execute(&self) -> AppResult<Vec<DriveInfo>> {
        let mut drives = self.drive_repo.list_all().await?;
        // 系统盘排到最后,非系统盘在前
        drives.sort_by_key(|d| Reverse(d.is_system));
        Ok(drives)
    }
}
