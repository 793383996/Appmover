//! `RollbackAppUseCase` —— 回滚单个应用。
//!
//! 1. 从 `state.json` 读取该 app 的 `MigrationReport`(含 backup_path)
//! 2. 让 `MigrationRepository` 真实执行:删 junction、rename backup → 源
//! 3. 清理 state.json 记录
//!
//! **Round 7 关键修复**:`state_store.remove` 失败时的处理:
//! - 之前:物理回滚成功后,`state.remove` 失败时直接 `?` 返回,留下"物理已
//!   回滚但 state.json 仍有 entry"的不一致状态。
//! - 后续启动 `ListMigrated` 又会显示该 app,用户可重试,但每次都"明明没文件
//!   了却显示已迁移"很烦。
//! - 现在:`state.remove` 失败时,记 error 并返回。**不**自动重试(避免无限
//!   循环),**不**改动 state.json(避免半完成状态)。`DetectOrphans` 后续
//!   启动时会发现该 entry 的 source 不存在 → 报 MissingJunction,前端可
//!   "force clean"清理。

use crate::domain::repositories::{MigrationRepository, StateStore};
use crate::domain::value_objects::AppId;
use crate::shared::{AppError, AppResult};
use std::sync::Arc;

pub struct RollbackAppUseCase {
    migration_repo: Arc<dyn MigrationRepository>,
    state_store: Arc<dyn StateStore>,
}

impl RollbackAppUseCase {
    pub fn new(
        migration_repo: Arc<dyn MigrationRepository>,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            migration_repo,
            state_store,
        }
    }

    pub async fn execute(&self, app_id: &AppId) -> AppResult<()> {
        // 1. 找 report
        let all = self.state_store.load_all().await?;
        let report = all.get(app_id).cloned().ok_or_else(|| {
            AppError::AppNotFound(format!("no migration report for {app_id}"))
        })?;

        // 2. 真实回滚:删 junction、rename 回源
        //    **Round 6**:失败时 best-effort 继续,记录首个 error
        self.migration_repo.rollback(&report).await?;

        // 3. 清理 state.json。
        //    **Round 7**:`remove` 失败时记 error 返回。物理状态已回滚,
        //    残留 entry 会被 `DetectOrphans` 在下次启动时标记为
        //    MissingJunction,前端可手动清理(state.json 写入时是覆盖式,
        //    不会出现半新半旧)。
        if let Err(e) = self.state_store.remove(app_id).await {
            tracing::error!(
                target: "appmover",
                "state_store.remove failed after physical rollback for {app_id}: {e}; \
                 entry will be detected as orphan on next launch"
            );
            return Err(e);
        }
        Ok(())
    }
}
