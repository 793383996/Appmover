//! 状态持久化仓储:把"已迁移的应用"写到 `%LOCALAPPDATA%\appmover\state.json`。
//!
//! 启动时读取,用于:
//! - UI "已迁移"列表
//! - 异常断电后的孤儿 junction 检测
//! - 回滚入口

use crate::domain::entities::MigrationReport;
use crate::domain::value_objects::AppId;
use crate::shared::AppResult;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait StateStore: Send + Sync {
    async fn load_all(&self) -> AppResult<HashMap<AppId, MigrationReport>>;
    async fn save(&self, report: &MigrationReport) -> AppResult<()>;
    async fn remove(&self, app_id: &AppId) -> AppResult<()>;
}
