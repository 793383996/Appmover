//! 磁盘仓储:列出机器上所有可用盘符 + 容量信息。

use crate::domain::entities::DriveInfo;
use crate::shared::AppResult;
use async_trait::async_trait;

#[async_trait]
pub trait DriveRepository: Send + Sync {
    async fn list_all(&self) -> AppResult<Vec<DriveInfo>>;
}
