//! `ScanAppsUseCase` —— 扫描所有已安装应用,并过滤系统组件。

use crate::domain::entities::InstalledApp;
use crate::domain::repositories::{AppRepository, PathGuard};
use crate::shared::AppResult;
use std::sync::Arc;

pub struct ScanAppsUseCase {
    app_repo: Arc<dyn AppRepository>,
    path_guard: Arc<dyn PathGuard>,
}

impl ScanAppsUseCase {
    pub fn new(app_repo: Arc<dyn AppRepository>, path_guard: Arc<dyn PathGuard>) -> Self {
        Self { app_repo, path_guard }
    }

    /// 全量扫描并过滤。
    pub async fn execute(&self) -> AppResult<Vec<InstalledApp>> {
        let all = self.app_repo.scan_all().await?;
        Ok(all
            .into_iter()
            .filter(|a| a.is_migratable())
            .filter(|a| {
                // 路径白名单过滤
                self.path_guard
                    .is_critical(&a.install_location, a.publisher.as_deref())
                    .is_ok()
            })
            .collect())
    }
}
