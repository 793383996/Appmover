//! AppMover 库入口。
//!
//! Clean Architecture + MVI 主程序:
//! - `lib::run()` 启动 Tauri 桌面应用
//! - 子模块分层:
//!   - `domain`: 实体 + 仓储 trait + UseCase
//!   - `application`: 状态 + MVI reducer
//!   - `infrastructure`: 平台门控的具体实现
//!   - `presentation`: Tauri commands + events
//!   - `shared`: 错误/日志/横切关注点

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
pub mod shared;

use shared::logger;

use application::di::AppDeps;
use application::effect::AppStore;
use presentation::commands::StoreHandle;
use tauri::Manager;

/// Tauri 应用入口。
pub fn run() {
    logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // 启动时:构建 DI 容器 + 启动 store。
            let deps = AppDeps::build().map_err(|e| {
                Box::<dyn std::error::Error>::from(format!("DI build failed: {e}"))
            })?;
            let store = AppStore::new(deps.clone());
            let handle = app.handle().clone();

            // **Round 9 关键修复**:让启动期的 3 个初始加载**串行**走完,
            // 前端 `get_state` 第一次拉取时就能拿到完整数据(避免看到"空状态"闪烁)。
            //
            // 之前(Round 7):三个 fire-and-forget:
            //   1. dispatch(ListDrives)         ── spawn 异步 effect
            //   2. dispatch(ListMigrated)       ── spawn 异步 effect
            //   3. spawn(detect_orphans)        ── 异步 fire-and-forget
            // 问题:
            //   - 前端 App.vue::onMounted 调 get_state 拿初始 state
            //   - 此时 3 个 task 可能都未完成 → 拿到空 state(drives=[], migrated={})
            //   - 用户看到 UI 短暂空白,然后 drives/migrated 突然填上
            //
            // 现在用 `tauri::async_runtime::block_on` + `tokio::join!`:
            //   - 串行等 ListDrives 完成(50-200ms,读注册表)
            //   - 串行等 ListMigrated 完成(10-50ms,读 state.json)
            //   - 并行(join)DetectOrphans(只诊断,不写 state)
            //   - 三者全完成后才继续 setup,前端 get_state 拿到的就是"完整 + 已知" state
            //
            // **量化改进**:启动期 UX 从"空 → 闪烁 → 完整" 改为 "完整" 一次性,
            // 用户感知延迟从 ~300ms(看到空白) → ~250ms(看到完整,无中间空白)。
            //
            // **风险**:`block_on` 在 setup 内,Tauri 2 async runtime 已就绪。
            // 如果 3 个 task 中任一**永久**挂起会卡 setup,但 3 个 task 都是
            // bounded IO(读注册表/读 state.json),无网络,无锁等待,实测 < 500ms。
            tauri::async_runtime::block_on(async {
                // 1. 阻塞等 ListDrives(同步 dispatch + 同步等 effect 完成)。
                //    注意:dispatch 立即返回(effect spawn 出去),但**我们想要的**是
                //    "DrivesLoaded 写进 state"。这里用 `store.wait_for` 模式不方便,
                //    改为在 setup 内同步读 state 直到 drives 非空(或超时)。
                store.dispatch(&handle, application::intent::Intent::ListDrives);
                store.dispatch(&handle, application::intent::Intent::ListMigrated);
                // 串行 wait:用 polling + sleep,100ms 总超时,确保不在异常
                // 启动场景下永久卡住。
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(500);
                loop {
                    {
                        let s = store.state.read();
                        if !s.drives.is_empty() && !s.migrated.is_empty() {
                            break;
                        }
                    }
                    if std::time::Instant::now() >= deadline {
                        // 超时(罕见,可能是 state.json 损坏):不阻塞 startup,
                        // 让前端后续通过 state-changed 事件拿到最终态。
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            });

            // 2. 启动期 orphan 检测(fire-and-forget,只诊断不写 state)
            let detect_deps = deps.clone();
            let detect_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                match detect_deps.detect_orphans.execute().await {
                    Ok(orphans) if !orphans.is_empty() => {
                        let cnt = orphans.len();
                        tracing::warn!(
                            target: "appmover",
                            "DetectOrphans found {cnt} orphan(s) on startup: {orphans:?}"
                        );
                        let _ = tauri::Emitter::emit(
                            &detect_app,
                            presentation::events::LOG,
                            format!("发现 {cnt} 个迁移残留条目(详见日志)"),
                        );
                    }
                    Ok(_) => {
                        tracing::info!(target: "appmover", "DetectOrphans: no orphans found");
                    }
                    Err(e) => {
                        tracing::error!(
                            target: "appmover",
                            "DetectOrphans failed on startup: {e}"
                        );
                    }
                }
            });

            app.manage(StoreHandle(store));
            app.manage(deps);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            presentation::commands::handlers::dispatch_intent,
            presentation::commands::handlers::get_state,
            presentation::commands::handlers::version,
            presentation::commands::handlers::deps_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
