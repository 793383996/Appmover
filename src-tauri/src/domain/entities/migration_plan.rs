//! 迁移计划实体。
//!
//! 表达"把源路径的内容,搬到目标路径,建符号链接"。
//! 一个 InstalledApp 一次只能有 1 个 active plan。

use crate::domain::value_objects::{AppId, AppPath, ByteSize};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub app_id: AppId,
    /// 原始位置(C:\Program Files\7-Zip)
    pub source: AppPath,
    /// 目标位置(D:\Apps\7-Zip)
    pub target: AppPath,
    /// 计划占用大小
    pub estimated_size: ByteSize,
    /// 计划创建时间
    pub created_at: DateTime<Utc>,
}

impl MigrationPlan {
    pub fn new(
        app_id: AppId,
        source: AppPath,
        target: AppPath,
        estimated_size: ByteSize,
    ) -> Self {
        Self {
            app_id,
            source,
            target,
            estimated_size,
            created_at: Utc::now(),
        }
    }
}
