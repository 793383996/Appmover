//! 迁移状态机。
//!
//! 状态转换严格遵循:
//!   Idle → Checking → Copying → Linking → Verifying → Completed
//!                                   ↓
//!                                  Failed(can_rollback: bool)
//!   Completed → RollingBack → RolledBack
//!   Failed(can_rollback) → RollingBack → RolledBack | RollbackFailed

use crate::domain::value_objects::{AppId, ByteSize};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPhase {
    Idle,
    Checking,
    Copying,
    Linking,
    Verifying,
    Completed,
    Failed,
    RollingBack,
    RolledBack,
    RollbackFailed,
    Cancelled,
}

impl MigrationPhase {
    /// 是否终态(不可再变化)。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::RolledBack | Self::RollbackFailed | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub app_id: AppId,
    pub phase: MigrationPhase,
    /// 已复制字节(0 → estimated_size)
    pub copied_bytes: ByteSize,
    /// 总字节(用于算进度百分比)
    pub total: ByteSize,
    /// 复制速度(字节/秒)
    pub speed_bps: u64,
    /// 错误信息(失败时)
    pub error: Option<String>,
    /// 起始时间
    pub started_at: Option<DateTime<Utc>>,
    /// 完成时间
    pub finished_at: Option<DateTime<Utc>>,
}

impl MigrationStatus {
    pub fn idle(app_id: AppId) -> Self {
        Self {
            app_id,
            phase: MigrationPhase::Idle,
            copied_bytes: ByteSize::ZERO,
            total: ByteSize::ZERO,
            speed_bps: 0,
            error: None,
            started_at: None,
            finished_at: None,
        }
    }

    /// 进度 0.0 - 1.0(只对 Copying 阶段有意义)。
    pub fn progress(&self, total: ByteSize) -> f64 {
        if total.as_bytes() == 0 {
            return 0.0;
        }
        self.copied_bytes.as_bytes() as f64 / total.as_bytes() as f64
    }
}

/// 状态机转换合法性检查。
pub fn is_valid_transition(from: MigrationPhase, to: MigrationPhase) -> bool {
    use MigrationPhase::*;
    matches!(
        (from, to),
        (Idle, Checking)
            | (Checking, Copying)
            | (Checking, Failed)
            | (Copying, Linking)
            | (Copying, Failed)
            | (Copying, Cancelled)
            | (Linking, Verifying)
            | (Linking, Failed)
            | (Verifying, Completed)
            | (Verifying, Failed)
            | (Failed, RollingBack)
            | (Completed, RollingBack)
            | (RollingBack, RolledBack)
            | (RollingBack, RollbackFailed)
    )
}
