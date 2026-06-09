//! `MigrateAppUseCase` —— 迁移主流程。
//!
//! 步骤:
//! 1. 前置校验
//! 2. 复制 source → target(进度上报)
//! 3. 创建 junction(source 现在指向 target)
//! 4. 写 state.json(完成持久化,用于回滚 + 重启后列表)
//! 5. 返回 MigrationReport
//!
//! **Round 7 关键修复**:`state.json` 持久化失败时必须回滚刚完成的物理迁移:
//! - 之前:复制 + junction + verify 都成功后,`state_store.save` 失败时直接
//!   `?` 返回,留下 junction + backup + target 三份物理残留。
//! - 用户后续无法 rollback(use case 从 state.json 找不到 entry),`DetectOrphans`
//!   也只检测"state 有但 source 没",漏检此场景。
//! - 现在:save 失败时调 `migration_repo.rollback(&report)`,让物理状态回到
//!   "source 重新可用,target 被清理",返回首个 error 给上层。

use crate::domain::entities::{MigrationPlan, MigrationReport};
use crate::domain::repositories::{CopyProgress, MigrationRepository, StateStore};
use crate::domain::usecases::CheckMigrationPreconditionsUseCase;
use crate::shared::AppResult;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct MigrateAppUseCase {
    precheck: Arc<CheckMigrationPreconditionsUseCase>,
    migration_repo: Arc<dyn MigrationRepository>,
    state_store: Arc<dyn StateStore>,
}

impl MigrateAppUseCase {
    pub fn new(
        precheck: Arc<CheckMigrationPreconditionsUseCase>,
        migration_repo: Arc<dyn MigrationRepository>,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            precheck,
            migration_repo,
            state_store,
        }
    }

    /// 全量执行。`progress_tx` 可选,传 `None` 则不报进度。
    pub async fn execute(
        &self,
        plan: &MigrationPlan,
        publisher: Option<&str>,
        progress_tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport> {
        // 1. 前置校验
        self.precheck
            .execute(
                &plan.source,
                &plan.target,
                publisher,
                plan.estimated_size.as_bytes(),
            )
            .await?;

        // 2. 真实迁移(进度透传)
        //    **Round 3 修复**:不要建一个没人 receiver 的空 channel。如果
        //    `progress_tx` 是 None,CopyEngine.send 一定会失败(rx drop 之后),
        //    既浪费 IO 又无意义。改为直接 `None`。
        let report = self
            .migration_repo
            .migrate(&plan.source, &plan.target, &plan.app_id, progress_tx, cancel)
            .await?;

        // 3. 写 state.json(持久化完成结果,后续重启 / 回滚都依赖它)。
        //    **Round 7 关键修复**:写失败时**必须**回滚刚完成的物理迁移。
        //    否则 junction + backup + target 三份物理残留,用户后续无法
        //    回滚(use case 从 state.json 找不到 entry),orphan 检测也漏掉。
        //    **Round 8 增强**:物理 rollback 也可能 fail(Windows 上 junction
        //    占用 / backup 已被人为删除等),此时**返回首个 error**(save 的),
        //    但 log 严重 warn 提示用户需要手动清理。
        if let Err(e) = self.state_store.save(&report).await {
            tracing::error!(
                target: "appmover",
                "state_store.save failed after physical migration: {e}; rolling back"
            );
            // best-effort 物理回滚(同 rollback UseCase,但不删 state.json
            // 因为这里 state.json 根本没存进去)
            if let Err(rb_err) = self.migration_repo.rollback(&report).await {
                tracing::error!(
                    target: "appmover",
                    "CRITICAL: physical rollback after save failure also failed: {rb_err}; \
                     MANUAL CLEANUP REQUIRED for app={} source={} target={} backup={}",
                    report.app_id,
                    report.source.as_path().display(),
                    report.target.as_path().display(),
                    report.backup_path.as_path().display(),
                );
            }
            // 返回首个 error(save 的),让上层知道流程失败
            return Err(e);
        }

        Ok(report)
    }
}
