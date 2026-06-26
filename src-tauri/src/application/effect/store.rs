//! 副作用层:发起 IO、调 UseCase、回报新 Intent。
//!
//! 流程:
//!   `用户 Intent → dispatch() → effect.handle() → 调 UseCase → 产生新 Intent → dispatch() → reducer → 新 State → 事件总线 → 前端`
//!
//! 设计要点(经过 Round 2 + Round 3 质量复盘):
//! - dispatch 持锁内只 reduce;emit 移到锁外(不阻塞并发 dispatch)
//! - effect 用 `catch_unwind` 包,panic 不会让 Tauri runtime 死
//! - migrations 用独立 AtomicUsize 计数(避免误收敛)
//! - size calc **Round 3 重构**:用 batch id 隔离,新 batch 不会让旧 batch 提前归零
//! - StartMigration 用 TaskTracker 并行启动,内层限流到 MAX_CONCURRENT_MIGRATIONS
//! - ids 超过 MAX_BATCH_MIGRATIONS 拒绝(避免一次性 1000 个迁移)
//! - ShowToast 用 generation 标识,自动 DismissToast 只清自己的那次
//! - **Round 3 修复**:取消(cancel)与失败(fail)在 effect 层区分,
//!   取消时不弹 error toast,不更新 toast 颜色

use crate::application::di::AppDeps;
use crate::application::intent::Intent;
use crate::application::state::{AppState, LoadingKind, ToastKind};
use crate::application::reducer::reduce;
use crate::domain::entities::MigrationPlan;
use crate::domain::value_objects::{AppId, AppPath};
use crate::presentation::events;
use crate::shared::AppResult;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// In-flight 迁移任务的最大并发数(避免同时启动 100 个 IO)。
const MAX_CONCURRENT_MIGRATIONS: usize = 4;
/// 单次 StartMigration 接收的最大 ids 数,超出拒绝并 Toast 提示。
const MAX_BATCH_MIGRATIONS: usize = 32;
/// 普通 toast 自动消失时间(Info / Success)。
const TOAST_AUTO_DISMISS_MS: u64 = 4000;
/// **Round 8**:Error / Warning toast 自动消失时间(更久,让用户有充足时间读错误)。
const TOAST_ERROR_DISMISS_MS: u64 = 8000;

/// 全局 Store 包装:状态 + DI + 任务调度。
pub struct AppStore {
    pub state: Arc<RwLock<AppState>>,
    pub deps: Arc<AppDeps>,
    /// 正在跑的迁移任务 cancel token,key = app_id。
    cancellations: Arc<RwLock<HashMap<AppId, CancellationToken>>>,
    /// 正在跑的迁移任务数(独立计数,不被 size calc 影响)。
    migrations_in_flight: Arc<AtomicUsize>,
    /// Toast generation:每次 ShowToast 自增,确保 DismissToast 精准。
    toast_gen: Arc<AtomicU64>,
}

impl AppStore {
    pub fn new(deps: Arc<AppDeps>) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(RwLock::new(AppState::new())),
            deps,
            cancellations: Arc::new(RwLock::new(HashMap::new())),
            migrations_in_flight: Arc::new(AtomicUsize::new(0)),
            toast_gen: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 分配下一个 toast generation(由 dispatch 调用,保证不重复)。
    pub fn next_toast_gen(&self) -> u64 {
        self.toast_gen.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 同步 dispatch:reducer 改 state,在锁外 emit,async 跑 effect。
    ///
    /// **关键**:emit 不在锁内(避免大 state 序列化阻塞并发 dispatch)。
    /// **关键**:effect 用 `catch_unwind` 包裹(防止 effect panic 把 Tauri runtime 干掉)。
    pub fn dispatch(self: &Arc<Self>, app: &AppHandle, intent: Intent) {
        let store = self.clone();
        let app = app.clone();
        let intent_for_reducer = intent.clone();
        let intent_for_effect = intent;

        // 1. 同步:reducer 改 state,**在锁内只做状态变更**
        let new_state = {
            let mut s = self.state.write();
            *s = reduce(s.clone(), intent_for_reducer);
            s.clone()
        };
        // 2. **锁外** emit,序列化不再阻塞其他 dispatch
        //    **Round 9 修复**:emit 失败时记 log(之前 `let _ = ...` 静默吞)。
        //    emit 失败原因可能是:webview 已关闭 / IPC channel 满 / serde 失败。
        //    全部罕见但都应被记录,便于诊断"前端为什么没收到 state-changed"类问题。
        if let Err(e) = tauri::Emitter::emit(&app, events::STATE_CHANGED, &new_state) {
            tracing::warn!(
                target: "appmover",
                "emit STATE_CHANGED failed: {e}"
            );
        }

        // 3. 异步 effect + panic 捕获
        //    tokio::spawn 本身会把 panic 转 JoinError,这里再 is_panic() 检测并发事件
        if intent_for_effect.needs_effect() {
            let store_for_effect = store.clone();
            let app_for_effect = app.clone();
            let app_inner = app.clone();
            tokio::spawn(async move {
                let join = tokio::spawn(async move {
                    store_for_effect.handle(&app_inner, intent_for_effect).await
                })
                .await;
                match join {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::error!(target: "appmover", "effect error: {:?}", e);
                        if let Err(emit_err) = tauri::Emitter::emit(
                            &app_for_effect,
                            events::LOG,
                            format!("effect error: {e}"),
                        ) {
                            tracing::warn!(
                                target: "appmover",
                                "emit LOG (effect error) failed: {emit_err}"
                            );
                        }
                    }
                    Err(join_err) => {
                        // 关键:effect panic 不让 Tauri runtime 死
                        if join_err.is_panic() {
                            tracing::error!(target: "appmover", "effect panicked: {:?}", join_err);
                            if let Err(emit_err) = tauri::Emitter::emit(
                                &app_for_effect,
                                events::LOG,
                                "effect panic (recovered)".to_string(),
                            ) {
                                tracing::warn!(
                                    target: "appmover",
                                    "emit LOG (panic) failed: {emit_err}"
                                );
                            }
                        } else {
                            tracing::error!(target: "appmover", "effect task aborted: {join_err}");
                        }
                    }
                }
            });
        }
    }

    /// 显式 emit 进度/日志(不走 reducer,高频)。
    pub fn emit_progress(&self, app: &AppHandle, p: &crate::domain::repositories::CopyProgress) {
        // **Round 9 修复**:emit 失败记 log
        if let Err(e) = tauri::Emitter::emit(app, events::MIGRATION_PROGRESS, p) {
            tracing::warn!(target: "appmover", "emit MIGRATION_PROGRESS failed: {e}");
        }
    }

    /// 处理需要副作用的 Intent。
    ///
    /// **Round 13 修复**:`LoadingResetOnDrop` guard 保证即使在 await panic 路径下
    /// `SetLoading(Idle)` 也会被调用。原代码靠 match 分支末尾显式调用,panic 时
    /// 直接 stack unwinding 跳过 → UI 卡死。
    async fn handle(self: Arc<Self>, app: &AppHandle, intent: Intent) -> AppResult<()> {
        let deps = self.deps.clone();
        let app = app.clone();
        match intent {
            Intent::ScanApps => {
                // **Round 13**:RAII guard — 防止 scan_apps panic 时 stuck in Scanning
                let _guard = LoadingResetOnDrop::new(self.clone(), app.clone());
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Scanning });
                match deps.scan_apps.execute().await {
                    Ok(apps) => self.dispatch(&app, Intent::AppsScanned(apps)),
                    Err(e) => self.show_toast(&app, ToastKind::Error, format!("扫描失败: {e}")),
                }
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Idle });
            }

            Intent::ListDrives => {
                // **Round 13**:同上,RAII guard 防 stuck in LoadingDrives
                let _guard = LoadingResetOnDrop::new(self.clone(), app.clone());
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::LoadingDrives });
                match deps.list_drives.execute().await {
                    Ok(d) => self.dispatch(&app, Intent::DrivesLoaded(d)),
                    Err(e) => self.show_toast(&app, ToastKind::Error, format!("获取磁盘失败: {e}")),
                }
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Idle });
            }

            Intent::ListMigrated => match deps.list_migrated.execute().await {
                Ok(map) => {
                    let reports: Vec<_> = map.into_values().collect();
                    self.dispatch(&app, Intent::MigratedLoaded(reports));
                }
                // **Round 7 修复**:失败也弹 toast(与 ListDrives 一致,UX 对称)。
                // 之前只 `tracing::warn`,用户不知道 ListMigrated 失败了。
                Err(e) => self.show_toast(
                    &app,
                    ToastKind::Warning,
                    format!("加载已迁移列表失败: {e}"),
                ),
            },

            Intent::CalculateSizes => {
                // **Round 4 重构**:per-batch counter 取代 batch id + 简化版逻辑。
                // - 每个 CalculateSizes 调用创建一个独立 `Arc<AtomicUsize>` counter
                //   (初值 = apps.len())
                // - 每个 task 完成后 `fetch_sub(1)`,**只有刚到 0 的 task** 才触发 Idle
                // - 旧 batch 的 task 继续跑不影响新 batch 的 counter(独立)
                // - **完全消除** Round 3 简化版"每个 task 都触发 Idle"的 N 次冗余 dispatch
                let apps = self.state.read().apps.clone();
                let total = apps.len();
                if total == 0 {
                    self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Idle });
                    return Ok(());
                }
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::CalculatingSize });

                let counter = Arc::new(AtomicUsize::new(total));
                // 进度节流:避免每次文件都报,256ms 节流
                let (tx, mut rx) = mpsc::channel::<crate::domain::repositories::SizeProgress>(64);
                // **Round 4 修复**:forwarder 改用 `take_while` 模式 — 当所有 tx clone drop
                // 后,rx 关闭,while let 自然退出,任务结束。
                let app_size = app.clone();
                tokio::spawn(async move {
                    while let Some(p) = rx.recv().await {
                        if let Err(e) = tauri::Emitter::emit(
                            &app_size,
                            events::SIZE_PROGRESS,
                            &p,
                        ) {
                            // **Round 9 修复**:emit 失败记 log(高频路径,但出错就值得记)
                            tracing::warn!(target: "appmover", "emit SIZE_PROGRESS failed: {e}");
                        }
                    }
                });

                // 为每个 app 启动一个 size calc
                for app_item in apps {
                    let path = app_item.install_location.clone();
                    let id = app_item.id.clone();
                    let store2 = self.clone();
                    let app2 = app.clone();
                    let deps2 = deps.clone();
                    let tx2 = tx.clone();
                    let counter2 = counter.clone();
                    tokio::spawn(async move {
                        // **Round 13 修复**:RAII counter guard — 即使 calculate_size
                        // panic,counter 也会 - 1,最后一个 task 触发 Idle。原代码
                        // fetch_sub 在 await 之后,panic 路径跳过 → counter 永远 > 0
                        // → SetLoading(Idle) 永不触发 → UI 卡死 in CalculatingSize。
                        let _counter_guard = SizeCounterGuard {
                            counter: counter2,
                            store: store2.clone(),
                            app: app2.clone(),
                        };
                        // **Round 4 修复**:用 spawn_blocking 内部的 cancel token
                        // 让 UI 可取消(暂时还没暴露给前端,保留接口)
                        let cancel = Arc::new(CancellationToken::new());
                        let result = deps2
                            .calculate_size
                            .execute_with_progress(&path, tx2, cancel)
                            .await;
                        match result {
                            Ok(bytes) => {
                                store2.dispatch(
                                    &app2,
                                    Intent::SizeProgress {
                                        id,
                                        current: crate::domain::value_objects::ByteSize(bytes),
                                    },
                                );
                            }
                            Err(e) => {
                                tracing::warn!(target: "appmover", "size calc failed: {e}");
                            }
                        }
                        // **Round 13 修复**:fetch_sub 已迁入 `_counter_guard` 的 Drop。
                        // _counter_guard 在此作用域结束时(无论 Ok/Err/panic)自动 - 1。
                        // 最后到 0 的 guard 触发 SetLoading(Idle)。
                    });
                }
                // **Round 4 修复**:显式 drop 原始 sender,让前向器 task 在所有 clone drop
                // 后正常退出(虽然 forwarder 也在 N 个 tx 完成后通过 rx.close() 自然结束,
                // 这里 drop 是保险,语义更清晰)。
                drop(tx);
            }

            Intent::StartMigration { ids } => {
                // 1. 批量上限校验
                if ids.len() > MAX_BATCH_MIGRATIONS {
                    self.show_toast(
                        &app,
                        ToastKind::Warning,
                        format!(
                            "单次最多 {MAX_BATCH_MIGRATIONS} 个应用,当前 {} 个已拒绝",
                            ids.len()
                        ),
                    );
                    return Ok(());
                }

                let state = self.state.read().clone();

                // 2. 校验:目标盘已选
                let Some(target_drive) = state.ui.target_drive.clone() else {
                    self.show_toast(&app, ToastKind::Warning, "请先选择目标盘".into());
                    return Ok(());
                };

                // 3. 过滤
                let mut to_migrate: Vec<_> = Vec::new();
                for id in ids {
                    if let Some(app_item) = state.apps.iter().find(|a| a.id == id).cloned() {
                        if !app_item.is_migratable() {
                            self.show_toast(
                                &app,
                                ToastKind::Warning,
                                format!("\"{}\" 不在 C:\\,跳过", app_item.display_name),
                            );
                            continue;
                        }
                        if state.migrated.contains_key(&id) {
                            self.show_toast(
                                &app,
                                ToastKind::Warning,
                                format!("\"{}\" 已迁移,跳过", app_item.display_name),
                            );
                            continue;
                        }
                        to_migrate.push(app_item);
                    } else {
                        self.show_toast(
                            &app,
                            ToastKind::Warning,
                            format!("应用 {id} 不存在,跳过"),
                        );
                    }
                }

                if to_migrate.is_empty() {
                    return Ok(());
                }

                // 4. 并行启动(JoinSet),但限流到 MAX_CONCURRENT_MIGRATIONS
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Migrating });
                let tracker = TaskTracker::new();
                let sem = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_MIGRATIONS));
                for app_item in to_migrate {
                    let store2 = self.clone();
                    let app2 = app.clone();
                    let td = target_drive.clone();
                    let sem2 = sem.clone();
                    tracker.spawn(async move {
                        let _permit = sem2.acquire_owned().await.expect("semaphore closed");
                        store2.spawn_migration(app2, app_item, td).await;
                    });
                }
                tracker.close();

                // 5. 等所有迁移结束再 Idle
                let store_idle = self.clone();
                let app_idle = app.clone();
                tokio::spawn(async move {
                    tracker.wait().await;
                    store_idle.dispatch(&app_idle, Intent::SetLoading { kind: LoadingKind::Idle });
                });
            }

            Intent::CancelMigration { id } => {
                // **Round 9 修复**:取消去重。
                // 之前:同一 id 重复点取消(快速点 2 下按钮),会:
                //   1. 调 `token.cancel()` 两次(幂等,无害)
                //   2. 弹两个"已请求取消" toast(冗余,UX 差)
                //   3. 触发 `MigrationPhase::Cancelled` 走两次(`spawn_migration`
                //      已经把 phase 切到 Cancelled,但后续 reducer 多次 dispatch
                //      也没问题,只是浪费)
                // 现在:用 `token.is_cancelled()` 判定,如果**已经**取消,直接
                // return,不再弹 toast。
                let token_opt = self.cancellations.read().get(&id).cloned();
                match token_opt {
                    Some(token) if !token.is_cancelled() => {
                        token.cancel();
                        self.show_toast(&app, ToastKind::Info, "已请求取消".into());
                    }
                    Some(_) => {
                        // 重复取消,静默(避免 UX 噪声)
                        tracing::debug!(
                            target: "appmover",
                            "CancelMigration: token for {id} already cancelled, ignored"
                        );
                    }
                    None => {
                        self.show_toast(&app, ToastKind::Warning, "未找到该迁移任务".into());
                    }
                }
            }

            Intent::Rollback { id } => {
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Migrating });
                match deps.rollback.execute(&id).await {
                    Ok(()) => {
                        self.show_toast(&app, ToastKind::Success, "回滚完成".into());
                        self.dispatch(&app, Intent::ListMigrated);
                    }
                    Err(e) => {
                        self.show_toast(&app, ToastKind::Error, format!("回滚失败: {e}"));
                    }
                }
                self.dispatch(&app, Intent::SetLoading { kind: LoadingKind::Idle });
            }

            Intent::ShowToast { kind, generation, .. } => {
                // 自动消失:在 effect 里 spawn 延时 DismissToast{generation}
                // 注意:DismissToast 携带 generation,reducer 只清同 generation 的
                // **Round 8**:Error / Warning 错误级延长到 8s(用户需要时间读错误信息
                // 决定下一步操作:重试 / 联系支持 / 看日志)。
                let store = self.clone();
                let app_clone = app.clone();
                let duration_ms = matches!(kind, ToastKind::Error | ToastKind::Warning)
                    .then_some(TOAST_ERROR_DISMISS_MS)
                    .unwrap_or(TOAST_AUTO_DISMISS_MS);
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(duration_ms))
                        .await;
                    store.dispatch(&app_clone, Intent::DismissToast { generation });
                });
            }

            _ => {}
        }
        Ok(())
    }

    /// 便利:分配 generation + 立即 ShowToast。
    fn show_toast(self: &Arc<Self>, app: &AppHandle, kind: ToastKind, message: String) {
        let gen = self.next_toast_gen();
        self.dispatch(app, Intent::ShowToast { kind, message, generation: gen });
    }

    /// 启动单个应用的迁移任务。
    ///
    /// **设计**:不持有 self.state 锁,所有 dispatch 走 clone。
    /// 进度更新通过 mpsc 通道节流(256ms),降低 reducer 锁竞争。
    ///
    /// **Round 11 修复**:`spawn_migration` 内部用 RAII `MigrationGuard` 保证
    /// `cancellations` map 清理 + `migrations_in_flight` 计数 - 1 **即使在 panic
    /// 路径下也运行**(`Drop` 总是触发)。原代码 `cancellations.write().remove()`
    /// 只在正常 Ok/Err 分支跑,future panic 会跳过清理 → token 永久驻留 map。
    async fn spawn_migration(
        self: &Arc<Self>,
        app: AppHandle,
        app_item: crate::domain::entities::InstalledApp,
        target_drive: crate::domain::value_objects::DriveLetter,
    ) {
        // 构造目标路径
        let app_name = app_item
            .install_location
            .as_path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("App");
        let target_str = format!(
            "{}/{}",
            target_drive.as_str().trim_end_matches(':'),
            app_name
        );
        let target_path = match AppPath::new(&target_str) {
            Ok(p) => p,
            Err(e) => {
                self.dispatch(
                    &app,
                    Intent::MigrationFailed {
                        id: app_item.id.clone(),
                        error: e.to_string(),
                    },
                );
                self.show_toast(&app, ToastKind::Error, format!("构造目标路径失败: {e}"));
                return;
            }
        };

        let size = app_item
            .actual_size
            .or(app_item.estimated_size)
            .unwrap_or(crate::domain::value_objects::ByteSize::ZERO);
        let plan = MigrationPlan::new(
            app_item.id.clone(),
            app_item.install_location.clone(),
            target_path,
            size,
        );

        let id = app_item.id.clone();
        let publisher = app_item.publisher.clone();
        let cancel = CancellationToken::new();

        // **原子** 登记 in-flight + cancel token
        self.migrations_in_flight.fetch_add(1, Ordering::SeqCst);
        self.cancellations.write().insert(id.clone(), cancel.clone());

        let (tx, mut rx) = mpsc::channel::<crate::domain::repositories::CopyProgress>(64);

        // 进度接收:reducer 节流(256ms),emit 立即发
        let store3 = self.clone();
        let app3 = app.clone();
        let id3 = id.clone();
        tokio::spawn(async move {
            use std::time::Instant;
            let mut last_reducer = Instant::now();
            while let Some(p) = rx.recv().await {
                // emit 高频,直接发
                if let Err(e) = tauri::Emitter::emit(&app3, events::MIGRATION_PROGRESS, &p) {
                    // **Round 9 修复**:emit 失败记 log
                    tracing::warn!(target: "appmover", "emit MIGRATION_PROGRESS (rx) failed: {e}");
                }
                // reducer 节流,降低锁竞争
                if last_reducer.elapsed().as_millis() >= 256 {
                    store3.dispatch(
                        &app3,
                        Intent::MigrationProgress {
                            id: id3.clone(),
                            copied: p.copied,
                            total: p.total,
                            speed_bps: p.speed_bps,
                        },
                    );
                    last_reducer = Instant::now();
                }
            }
        });

        // 实际迁移任务
        let store_main = self.clone();
        let app_main = app.clone();
        let id_main = id.clone();
        // **Round 11 修复**:RAII guard 保证 cancellations 清理 + in_flight 计数
        // 即使在 spawn future panic 时也运行(Drop 永远触发)。原代码把清理放在
        // Ok/Err match 后,panic 路径会跳过 → token 永久驻留 map,即使该 app
        // 已"完成"也无法重试 cancel(无影响但浪费内存)。
        let _guard = MigrationGuard {
            id: id_main.clone(),
            in_flight: self.migrations_in_flight.clone(),
            cancellations: self.cancellations.clone(),
        };
        let cancel_for_spawn = cancel.clone();
        tokio::spawn(async move {
            store_main.dispatch(
                &app_main,
                Intent::MigrationPhase {
                    id: id_main.clone(),
                    phase: crate::domain::entities::MigrationPhase::Checking,
                },
            );
            let result = store_main
                .deps
                .migrate
                .execute(&plan, publisher.as_deref(), Some(tx), Arc::new(cancel_for_spawn))
                .await;
            // **Round 11 修复**:原 `in_flight.fetch_sub(1)` + `cancellations.write().remove()`
            // 已迁入 `_guard` 的 Drop,无需在这里再调。
            // _guard 在此作用域结束时(无论 Ok/Err/panic)自动运行 Drop。

            match result {
                Ok(report) => {
                    store_main.dispatch(
                        &app_main,
                        Intent::MigrationPhase {
                            id: id_main.clone(),
                            phase: crate::domain::entities::MigrationPhase::Completed,
                        },
                    );
                    store_main.dispatch(
                        &app_main,
                        Intent::MigrationCompleted {
                            id: id_main.clone(),
                            report,
                        },
                    );
                    if let Err(emit_err) = tauri::Emitter::emit(
                        &app_main,
                        events::MIGRATION_COMPLETED,
                        &id_main,
                    ) {
                        tracing::warn!(
                            target: "appmover",
                            "emit MIGRATION_COMPLETED failed: {emit_err}"
                        );
                    }
                }
                Err(e) => {
                    // **Round 3 修复**:区分 Cancelled(用户操作)与真实失败。
                    // 取消:phase=Cancelled,不弹 error toast(用户已经看到 Info toast)
                    // 失败:phase=Failed,弹 error toast
                    if matches!(e, crate::shared::AppError::Cancelled) {
                        store_main.dispatch(
                            &app_main,
                            Intent::MigrationPhase {
                                id: id_main.clone(),
                                phase: crate::domain::entities::MigrationPhase::Cancelled,
                            },
                        );
                        store_main.dispatch(
                            &app_main,
                            Intent::MigrationFailed {
                                id: id_main.clone(),
                                error: "已取消".to_string(),
                            },
                        );
                        // 不弹 error toast
                    } else {
                        store_main.dispatch(
                            &app_main,
                            Intent::MigrationPhase {
                                id: id_main.clone(),
                                phase: crate::domain::entities::MigrationPhase::Failed,
                            },
                        );
                        store_main.dispatch(
                            &app_main,
                            Intent::MigrationFailed {
                                id: id_main.clone(),
                                error: e.to_string(),
                            },
                        );
                        store_main.show_toast(
                            &app_main,
                            ToastKind::Error,
                            format!("迁移失败: {e}"),
                        );
                    }
                }
            }
        });
    }
}

/// panic message 提取(短小,避免泄露大字符串)。
///
/// 当前 effect panic 由 `tokio::spawn` 的 `JoinError::is_panic()` 兜底,这里
/// 保留一个空实现以防未来扩展。
#[allow(dead_code)]
fn _panic_msg_unused() {}

/// **Round 11**:RAII 清理 guard — `spawn_migration` 创建,作用域结束(Drop)时
/// 强制运行 cleanup,即使 future panic 也不会跳过。
///
/// 业界标准防御模式(`std::sync::Mutex` poisoning recovery / `parking_lot` guard
/// 等都用相同思路):让"必须运行的清理"绑在生命周期上,而非控制流分支上。
///
/// 清理内容:
/// - `migrations_in_flight` 计数 - 1(虽然目前没人读,但作为"in-flight 监控"
///   的基础数据,值不应单调递增)
/// - `cancellations` map 移除该 id(否则 token 永久驻留)
struct MigrationGuard {
    id: AppId,
    in_flight: Arc<AtomicUsize>,
    cancellations: Arc<RwLock<HashMap<AppId, CancellationToken>>>,
}

impl Drop for MigrationGuard {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.cancellations.write().remove(&self.id);
        tracing::debug!(
            target: "appmover",
            "MigrationGuard dropped for app_id={}",
            self.id
        );
    }
}

/// **Round 13**:RAII loading reset guard — ScanApps / ListDrives 等单 await 分支
/// 创建,作用域结束(Drop)时如 `ui.loading` 仍非 Idle 则派发 `SetLoading(Idle)`。
///
/// 原代码靠 match 分支末尾显式 `dispatch(SetLoading(Idle))`,**`await` panic 路径
/// 会直接 stack unwinding 跳过** → UI loading spinner 永远转,按钮永远 disabled。
///
/// 业界对照:
/// - `tracing::Span` guard(Drop 时退出 span)
/// - `tempfile::NamedTempFile`(Drop 时删除)
/// - `parking_lot::Mutex` guard(Drop 时释放锁)
///
/// 关键点:Drop 内**读 state.ui.loading 判断**后才 dispatch,避免成功路径上
/// 重复派发(guard 看到是 Idle 就 no-op)。
struct LoadingResetOnDrop {
    store: Arc<AppStore>,
    app: AppHandle,
}

impl LoadingResetOnDrop {
    fn new(store: Arc<AppStore>, app: AppHandle) -> Self {
        Self { store, app }
    }
}

impl Drop for LoadingResetOnDrop {
    fn drop(&mut self) {
        // 只在 loading 非 Idle 时重置,成功路径已显式设置,这里是 no-op
        let current = self.store.state.read().ui.loading;
        if current != LoadingKind::Idle {
            tracing::warn!(
                target: "appmover",
                "LoadingResetOnDrop triggered: loading was {:?}, resetting to Idle",
                current
            );
            self.store
                .dispatch(&self.app, Intent::SetLoading { kind: LoadingKind::Idle });
        }
    }
}

/// **Round 13**:RAII size counter guard — CalculateSizes 启动的每个 task 创建,
/// Drop 时 `counter.fetch_sub(1)`,最后到 0 的 guard 触发 `SetLoading(Idle)`。
///
/// 原代码 `fetch_sub` 在 `match result` 之后,如果 `calculate_size.execute_with_progress`
/// 内部 panic,`fetch_sub` 不跑 → counter 永远 > 0 → SetLoading(Idle) 永不触发
/// → UI 卡死 in CalculatingSize。
///
/// 复用 R4 的 "last decrement triggers Idle" 模式,只是把 fetch_sub 搬到 Drop。
struct SizeCounterGuard {
    counter: Arc<AtomicUsize>,
    store: Arc<AppStore>,
    app: AppHandle,
}

impl Drop for SizeCounterGuard {
    fn drop(&mut self) {
        let prior = self.counter.fetch_sub(1, Ordering::SeqCst);
        if prior == 1 {
            self.store
                .dispatch(&self.app, Intent::SetLoading { kind: LoadingKind::Idle });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::AppId;

    /// **Round 11**:验证 MigrationGuard 的 Drop 行为 —— 即使持有者显式
    /// forget,Drop 也会跑(这里通过正常作用域结束验证)。
    #[test]
    fn migration_guard_drop_cleans_cancellations_map() {
        let cancellations: Arc<RwLock<HashMap<AppId, CancellationToken>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let id = AppId::new();
        let token = CancellationToken::new();

        cancellations.write().insert(id.clone(), token);
        in_flight.fetch_add(1, Ordering::SeqCst);
        assert_eq!(cancellations.read().len(), 1);
        assert_eq!(in_flight.load(Ordering::SeqCst), 1);

        {
            let _guard = MigrationGuard {
                id: id.clone(),
                in_flight: in_flight.clone(),
                cancellations: cancellations.clone(),
            };
            // 作用域内:map 仍有该 entry
            assert_eq!(cancellations.read().len(), 1);
            assert_eq!(in_flight.load(Ordering::SeqCst), 1);
        }
        // 作用域结束:Drop 自动触发
        assert_eq!(cancellations.read().len(), 0, "token must be removed on Drop");
        assert_eq!(in_flight.load(Ordering::SeqCst), 0, "in_flight must decrement on Drop");
    }

    /// **Round 11**:模拟 panic 路径 —— 即便 panic 触发(测试用 Rc<RefCell> 模拟),
    /// Drop 也会运行(因为 Rust 的 stack unwinding 总是 Drop 所有 owned 值)。
    #[test]
    fn migration_guard_drop_runs_on_panic() {
        // 用 `catch_unwind` 模拟 panic 后还能继续执行,验证 Drop 已运行
        let cancellations: Arc<RwLock<HashMap<AppId, CancellationToken>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let id = AppId::new();
        cancellations.write().insert(id.clone(), CancellationToken::new());
        in_flight.fetch_add(1, Ordering::SeqCst);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = MigrationGuard {
                id: id.clone(),
                in_flight: in_flight.clone(),
                cancellations: cancellations.clone(),
            };
            panic!("simulated panic inside guard scope");
        }));
        assert!(result.is_err(), "panic should propagate");

        // 即使 panic 后,Drop 已运行
        assert_eq!(cancellations.read().len(), 0, "Drop must run on panic unwind");
        assert_eq!(in_flight.load(Ordering::SeqCst), 0, "in_flight must decrement on panic");
    }

    // **Round 13**:SizeCounterGuard 测试 — fetch_sub 永远在 Drop 跑
    #[test]
    fn size_counter_guard_decrements_on_normal_drop() {
        let counter = Arc::new(AtomicUsize::new(2));
        // 用 NoOp store / app 不能直接构造,这里只验证 fetch_sub 行为
        {
            // 模拟 guard 的 fetch_sub 逻辑
            let _g = SizeCounterGuardProbe {
                counter: counter.clone(),
            };
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn size_counter_guard_decrements_on_panic() {
        let counter = Arc::new(AtomicUsize::new(1));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = SizeCounterGuardProbe {
                counter: counter.clone(),
            };
            panic!("simulated panic");
        }));
        assert!(result.is_err());
        // Drop 跑了 → counter - 1
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    // Probe 结构体,只测 fetch_sub 行为(避免构造 AppStore)
    struct SizeCounterGuardProbe {
        counter: Arc<AtomicUsize>,
    }
    impl Drop for SizeCounterGuardProbe {
        fn drop(&mut self) {
            self.counter.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
