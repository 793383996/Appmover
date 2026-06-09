//! `MigrationRepository` 实现 —— 真正的搬运动作。
//!
//! 流程:
//! 1. 复制 `source` → `target`(CopyEngine,带进度)
//! 2. rename `source` → `source + "_appmover_backup_YYYYMMDD_HHmmss"`
//! 3. 在 `source` 创建 junction 指向 `target`
//! 4. 验证:读 1 个 entry 的 metadata(防空目录误判)
//! 5. 返回 MigrationReport(包含 backup 路径)
//!
//! 失败处理:
//! - rename 失败 → 清理 target 目录
//! - junction 失败 → rename backup → source,清理 target
//! - verify 失败 → 同样的还原动作
//!
//! 回滚:
//! 1. 删除 junction(source 现在是 reparse point,`fs::remove_dir` 即可)
//! 2. rename `backup_path` → `source`
//! 3. best-effort 删除 `target`

use crate::domain::entities::MigrationReport;
use crate::domain::repositories::{CopyProgress, MigrationRepository};
use crate::domain::value_objects::{AppId, AppPath, ByteSize};
use crate::infrastructure::services::acl_preserver::AclPreserver;
use crate::infrastructure::services::copy_engine::CopyEngine;
use crate::infrastructure::services::junction_service::JunctionService;
use crate::shared::{AppError, AppResult};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct FileMigrationRepository {
    copy_engine: Arc<CopyEngine>,
    junction: JunctionService,
    acl: AclPreserver,
}

impl FileMigrationRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            copy_engine: CopyEngine::new(),
            junction: JunctionService::new(),
            acl: AclPreserver::new(),
        })
    }

    /// 计算 backup 目录路径:`<parent>/<source_name>_appmover_backup_YYYYMMDD_HHmmss`
    fn compute_backup_path(source: &AppPath, timestamp: &str) -> AppResult<AppPath> {
        let parent = source
            .as_path()
            .parent()
            .ok_or_else(|| AppError::UseCase("no parent for source".into()))?;
        let name = source
            .as_path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app");
        let backup_name = format!("{name}_appmover_backup_{timestamp}");
        let joined = parent.join(&backup_name);
        AppPath::new(joined.as_path()).map_err(|e| AppError::UseCase(format!("backup path: {e}")))
    }

    /// 递归数文件数。
    /// **Round 4 设计**:
    /// - 用 `std::fs`(同步)+ `spawn_blocking` 内部调用,避免阻塞 tokio worker
    /// - 上限 `max_files`,避免大目录 OOM 或栈深爆栈
    /// - 不收集路径,只数 cnt
    /// - 容错:read_dir 失败返回已计数(不抛错,因为 verify 是 best-effort)
    pub fn count_files_recursive(path: &std::path::Path, max_files: u64) -> u64 {
        let mut stack = vec![path.to_path_buf()];
        let mut cnt = 0u64;
        while let Some(p) = stack.pop() {
            let entries = match std::fs::read_dir(&p) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                cnt += 1;
                if cnt > max_files {
                    return cnt;
                }
                let ft = match entry.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if ft.is_dir() {
                    stack.push(entry.path());
                }
            }
        }
        cnt
    }
}

#[async_trait]
impl MigrationRepository for FileMigrationRepository {
    async fn migrate(
        &self,
        source: &AppPath,
        target: &AppPath,
        app_id: &AppId,
        progress_tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport> {
        let started_at = Utc::now();
        let started = std::time::Instant::now();
        let timestamp = started_at.format("%Y%m%d_%H%M%S").to_string();
        let backup = Self::compute_backup_path(source, &timestamp)?;

        // **Round 5 修复**:**count_files_recursive 是同步 IO**(`std::fs::read_dir`),
        // Round 4 直接 `async fn` 里调,大目录(百万文件)卡死 tokio worker 几秒。
        // 改为 `spawn_blocking` 包装,上限从 1M 提升到 10M。
        let source_path_buf = source.as_path().to_path_buf();
        let expected_file_count = tokio::task::spawn_blocking(move || {
            Self::count_files_recursive(&source_path_buf, 10_000_000)
        })
        .await
        .map_err(|e| AppError::UseCase(format!("count_files join: {e}")))?;

        // 1. 复制(失败不需清理 target,因为 target 是新目录;失败即结束)
        let total_estimate = ByteSize::ZERO; // 真实大小由 CopyEngine 自报
        let copied = match self
            .copy_engine
            .copy_dir(
                source.as_path(),
                target.as_path(),
                app_id,
                total_estimate,
                progress_tx,
                cancel,
            )
            .await
        {
            Ok(n) => n,
            Err(e) => {
                // 复制失败,best-effort 删 target 防止空间泄漏
                let _ = tokio::fs::remove_dir_all(target.as_path()).await;
                return Err(e);
            }
        };

        // 2. rename 源 → backup(失败则清理 target)
        if let Err(e) = tokio::fs::rename(source.as_path(), backup.as_path()).await {
            let _ = tokio::fs::remove_dir_all(target.as_path()).await;
            return Err(AppError::Io {
                path: source.as_path().to_path_buf(),
                source: e,
            });
        }

        // 3. 创建 junction(失败则回滚:rename backup → source,清理 target)
        if let Err(e) = self.junction.create(source, target) {
            let _ = tokio::fs::rename(backup.as_path(), source.as_path()).await;
            let _ = tokio::fs::remove_dir_all(target.as_path()).await;
            return Err(e);
        }

        // 4. 验证:对比 expected(复制前数) vs target 实际
        //    **Round 5 修复**:`count_files_recursive` 用 `spawn_blocking` 包装,
        //    上限 10M。差异 > 5% 视为异常,触发回滚。空 source 视为合法(0 == 0)。
        let target_path_buf = target.as_path().to_path_buf();
        let target_file_count = tokio::task::spawn_blocking(move || {
            Self::count_files_recursive(&target_path_buf, 10_000_000)
        })
        .await
        .map_err(|e| AppError::UseCase(format!("count_files join: {e}")))?;
        let verified = if expected_file_count == 0 {
            true
        } else {
            target_file_count * 100 >= expected_file_count * 95
        };
        if !verified {
            // **Round 6 修复**:还原时遵循 best-effort 原则(同 `rollback`),
            // 每步失败都记 error 继续,最后返回首个 error。
            // 原因:和 `rollback` 一样,junction 没删成功时 rename 在 Windows 上
            // 可能失败,但仍给一次机会。
            let mut first_error: Option<AppError> = None;
            if let Err(e) = self.junction.remove(source) {
                tracing::error!(
                    target: "appmover",
                    "verify-fail rollback: junction.remove failed: {e}; continuing"
                );
                first_error = Some(e);
            }
            if let Err(e) = tokio::fs::rename(backup.as_path(), source.as_path()).await {
                tracing::error!(
                    target: "appmover",
                    "verify-fail rollback: rename backup->source failed: {e}; continuing"
                );
                if first_error.is_none() {
                    first_error = Some(AppError::Io {
                        path: backup.as_path().to_path_buf(),
                        source: e,
                    });
                }
            }
            if let Err(e) = tokio::fs::remove_dir_all(target.as_path()).await {
                tracing::warn!(
                    target: "appmover",
                    "verify-fail rollback: remove target failed: {e}"
                );
            }
            return Err(first_error.unwrap_or_else(|| {
                AppError::UseCase(format!(
                    "verification failed: source had {expected_file_count} files, target has {target_file_count}"
                ))
            }));
        }

        // 5. ACL 兜底(对 target 应用 source 权限继承的简化版)。
        //    真实实现:Windows 走 `icacls <from> /save <file> && icacls <to> /restore <file>`
        //    见 `infrastructure::services::acl_preserver`。这里只做 best-effort 兜底。
        let _ = self.acl.preserve(source.as_path(), target.as_path()).await;

        let duration_ms = started.elapsed().as_millis() as u64;
        let finished_at = Utc::now();

        Ok(MigrationReport {
            app_id: app_id.clone(),
            source: source.clone(),
            target: target.clone(),
            backup_path: backup,
            total_size: ByteSize(copied),
            duration_ms,
            started_at,
            finished_at,
        })
    }

    /// **Round 6 修复**:`junction.remove` 失败时**不**直接 `?` 返回,
    /// 改为 best-effort 继续 `rename backup → source`:
    /// - 如果 junction 没删成功(常见原因:管理员权限/被占用),`fs::remove_dir` 失败。
    ///   此时 `source` 还是 reparse point,Windows 上 `rename(backup → source)`
    ///   会因为 target 存在(reparse)而失败 — 但**有些**情况下能成功(如
    ///   junction 在垃圾状态),给一次机会。
    /// - 如果 rename 也失败,记 error,继续 best-effort 清 target(部分清理),
    ///   再返回错误。**关键**:rollback 的 state.json 清理在 UseCase 层做,
    ///   这里只返回 Err,state.json 记录保留,用户可重试。
    async fn rollback(&self, report: &MigrationReport) -> AppResult<()> {
        let mut first_error: Option<AppError> = None;

        // 1. 删 junction(source 是 reparse point)。
        //    **Round 6**:失败不直接返回,先记 error 继续后续步骤。
        if let Err(e) = self.junction.remove(&report.source) {
            tracing::error!(
                target: "appmover",
                "rollback: junction.remove failed for {}: {e}; continuing best-effort",
                report.source.as_path().display()
            );
            first_error = Some(e);
        }

        // 2. rename backup → source。
        //    如果 junction 还在(上一步失败),Windows 上 rename 几乎肯定失败,
        //    记 error 继续清理 target。
        if let Err(e) = tokio::fs::rename(report.backup_path.as_path(), report.source.as_path()).await {
            tracing::error!(
                target: "appmover",
                "rollback: rename backup->source failed: {} -> {}: {e}",
                report.backup_path.as_path().display(),
                report.source.as_path().display()
            );
            if first_error.is_none() {
                first_error = Some(AppError::Io {
                    path: report.backup_path.as_path().to_path_buf(),
                    source: e,
                });
            }
        }

        // 3. best-effort 删除 target(即便前两步失败也尝试)
        if let Err(e) = tokio::fs::remove_dir_all(report.target.as_path()).await {
            tracing::warn!(
                target: "appmover",
                "rollback: failed to remove target {}: {e}",
                report.target.as_path().display()
            );
        }

        // 如果前两步任一失败,返回首个错误(state.json 保留,用户可重试)
        if let Some(e) = first_error {
            return Err(e);
        }
        Ok(())
    }
}
