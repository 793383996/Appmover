//! 应用仓储:枚举 Windows 已安装应用。
//!
//! 接口在 domain,实现在 `infrastructure::repositories::registry_app_repository`。

use crate::domain::entities::InstalledApp;
use crate::shared::AppResult;
use async_trait::async_trait;

#[async_trait]
pub trait AppRepository: Send + Sync {
    /// 全量扫描(从所有 hive 枚举 + 合并去重)。
    async fn scan_all(&self) -> AppResult<Vec<InstalledApp>>;

    /// 重新扫描(等价 scan_all,语义清晰)。
    async fn refresh(&self) -> AppResult<Vec<InstalledApp>> {
        self.scan_all().await
    }
}
