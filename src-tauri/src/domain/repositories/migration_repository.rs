//! 迁移执行仓储:复制 + 切换 + 验证 + 回滚 —— 真正的"搬运动作"。

use crate::domain::entities::MigrationReport;
use crate::domain::value_objects::{AppId, AppPath, ByteSize};
use crate::shared::AppResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyProgress {
    pub app_id: AppId,
    pub copied: ByteSize,
    pub total: ByteSize,
    pub speed_bps: u64,
}

#[async_trait]
pub trait MigrationRepository: Send + Sync {
    /// 完整迁移:复制 → 建 junction → 验证。
    /// 实现方必须把 backup 路径写入返回的 `MigrationReport.backup_path`。
    /// **Round 3 修复**:`progress_tx` 改为 `Option`,None 时不报进度(避免建空 channel)。
    async fn migrate(
        &self,
        source: &AppPath,
        target: &AppPath,
        app_id: &AppId,
        progress_tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport>;

    /// 回滚:删除 junction,把 `report.backup_path` rename 回 `report.source`,
    /// 然后清理 `report.target`(best-effort)。
    async fn rollback(&self, report: &MigrationReport) -> AppResult<()>;
}
