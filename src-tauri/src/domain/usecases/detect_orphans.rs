//! `DetectOrphansUseCase` —— 启动时检测异常断电 / 崩溃留下的残留。
//!
//! - state.json 有记录,但 source 已不在 → 提示用户"完成迁移"或"清理"
//! - 以后扩展:文件系统有 `_appmover_backup_*` 但 state.json 无记录 → 提示用户恢复

use crate::domain::entities::MigrationReport;
use crate::domain::repositories::{FilesystemProbe, StateStore};
use crate::domain::value_objects::AppId;
use crate::shared::AppResult;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct OrphanInfo {
    pub app_id: AppId,
    pub kind: OrphanKind,
    pub report: Option<MigrationReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanKind {
    /// state 里有记录,但 source 路径已不存在(junction 没了 / 盘没挂)
    MissingJunction,
}

pub struct DetectOrphansUseCase {
    state_store: Arc<dyn StateStore>,
    fs: Arc<dyn FilesystemProbe>,
}

impl DetectOrphansUseCase {
    pub fn new(state_store: Arc<dyn StateStore>, fs: Arc<dyn FilesystemProbe>) -> Self {
        Self { state_store, fs }
    }

    /// 扫描 state.json + 文件系统,找出孤儿条目。
    pub async fn execute(&self) -> AppResult<Vec<OrphanInfo>> {
        let all = self.state_store.load_all().await?;
        let mut out = Vec::new();
        for (id, report) in all {
            if !self.fs.exists(&report.source) {
                out.push(OrphanInfo {
                    app_id: id,
                    kind: OrphanKind::MissingJunction,
                    report: Some(report),
                });
            }
        }
        Ok(out)
    }
}
