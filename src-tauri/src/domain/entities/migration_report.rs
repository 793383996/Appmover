//! 迁移报告(完成时产出)。

use crate::domain::value_objects::{AppId, AppPath, ByteSize};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub app_id: AppId,
    /// 最终源路径(已是 junction)
    pub source: AppPath,
    /// 最终目标路径(实际数据)
    pub target: AppPath,
    /// 迁移时被 rename 为 backup 的源目录(回滚时用)
    pub backup_path: AppPath,
    pub total_size: ByteSize,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}
