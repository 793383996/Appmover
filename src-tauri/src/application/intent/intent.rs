//! MVI 中的 **I**(Intent)—— 前端发来的"我要做什么"。

use crate::domain::entities::InstalledApp;
use crate::domain::value_objects::{AppId, ByteSize, DriveLetter};
use crate::application::state::{FilterMode, LoadingKind, SortMode, ToastKind};
use serde::{Deserialize, Serialize};

/// 所有用户意图的统一枚举。
/// 单一来源,所有变更都从 reducer 走。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    // ---- 扫描 / 列表 ----
    /// 触发应用扫描
    ScanApps,
    /// 触发磁盘列表
    ListDrives,
    /// 加载已迁移清单
    ListMigrated,
    /// 扫描完成,后端 effect 回填
    AppsScanned(Vec<InstalledApp>),
    DrivesLoaded(Vec<crate::domain::entities::DriveInfo>),
    MigratedLoaded(Vec<crate::domain::entities::MigrationReport>),

    // ---- 选中 / 过滤 / 搜索 ----
    SelectApp { id: AppId, selected: bool },
    SelectAll,
    ClearSelection,
    SetTargetDrive { letter: DriveLetter },
    SetSearch { query: String },
    SetFilter { mode: FilterMode },
    SetSort { mode: SortMode },

    // ---- 大小计算 ----
    CalculateSizes,
    SizeProgress { id: AppId, current: ByteSize },

    // ---- 迁移 ----
    StartMigration { ids: Vec<AppId> },
    CancelMigration { id: AppId },
    /// 进度上报(由 effect 内部触发,前端不直接发)
    MigrationProgress {
        id: AppId,
        copied: ByteSize,
        total: ByteSize,
        speed_bps: u64,
    },
    MigrationPhase {
        id: AppId,
        phase: crate::domain::entities::MigrationPhase,
    },
    MigrationFailed {
        id: AppId,
        error: String,
    },
    MigrationCompleted {
        id: AppId,
        report: crate::domain::entities::MigrationReport,
    },

    // ---- 回滚 ----
    Rollback { id: AppId },

    // ---- 系统 ----
    SetLoading { kind: LoadingKind },
    /// `generation` 由 effect 层在 spawn DismissToast 时填回,确保 dismiss 不误伤其他 toast。
    ShowToast { kind: ToastKind, message: String, generation: u64 },
    DismissToast { generation: u64 },
}

impl Intent {
    /// 此 Intent 是否会触发副作用(需要 effect 处理)。
    /// 反向:仅改 state 的 Intent 不触发 IO。
    pub fn needs_effect(&self) -> bool {
        matches!(
            self,
            Intent::ScanApps
                | Intent::ListDrives
                | Intent::ListMigrated
                | Intent::CalculateSizes
                | Intent::StartMigration { .. }
                | Intent::CancelMigration { .. }
                | Intent::Rollback { .. }
        )
    }
}
