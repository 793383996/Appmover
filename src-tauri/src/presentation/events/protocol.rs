//! Tauri 事件名常量 + payload 类型。
//!
//! 全部事件集中此处,前端 `listen` 用同一名字。

use serde::{Deserialize, Serialize};

/// 状态整体变更(后端 reducer 跑了,推一份新 state 给前端)
pub const STATE_CHANGED: &str = "appmover://state-changed";

/// 进度:复制中实时上报
pub const MIGRATION_PROGRESS: &str = "appmover://migration-progress";

/// 单个迁移完成
pub const MIGRATION_COMPLETED: &str = "appmover://migration-completed";

/// 通用日志流
pub const LOG: &str = "appmover://log";

/// **Round 3 新增**:size calc 实时进度(高频,走独立事件而非 reducer 状态)。
/// payload 是 `SizeProgress`(path / current_bytes / files_scanned)。
pub const SIZE_PROGRESS: &str = "appmover://size-progress";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub app_id: String,
    pub copied: u64,
    pub total: u64,
    pub speed_bps: u64,
}
