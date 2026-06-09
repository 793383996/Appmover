//! Windows 进程占用检测 —— `sysinfo` 枚举所有进程,看 exe 路径是否在 install_path 下。

#[cfg(windows)]
use crate::domain::repositories::ProcessGuard;
#[cfg(windows)]
use crate::domain::value_objects::AppPath;
#[cfg(windows)]
use crate::shared::AppResult;
#[cfg(windows)]
use async_trait::async_trait;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use sysinfo::System;

#[cfg(windows)]
pub struct WindowsProcessGuard;

#[cfg(windows)]
impl WindowsProcessGuard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[cfg(windows)]
#[async_trait]
impl ProcessGuard for WindowsProcessGuard {
    async fn find_blocking_processes(&self, path: &AppPath) -> AppResult<Vec<String>> {
        let prefix = path.as_path().to_string_lossy().to_ascii_lowercase();
        // **Round 3 修复**:`System::new_all()` + `refresh_processes()` 是同步阻塞 IO
        // (读 /proc 或 NtQuerySystemInformation),直接 `async fn` 调用会卡死 tokio worker。
        // 用 `spawn_blocking` 包装,这是 tokio 官方对阻塞 IO 的标准处理。
        // 注:`System` 缓存到 struct 字段里可以避免每次重建,但 `System` 不是 `Send`,
        // 后续如果需要缓存可包 `Arc<parking_lot::Mutex<System>>`,当前保持简单。
        tokio::task::spawn_blocking(move || -> AppResult<Vec<String>> {
            let mut sys = System::new_all();
            sys.refresh_processes();
            let mut out = Vec::new();
            for (pid, p) in sys.processes() {
                if let Some(exe) = p.exe() {
                    if exe
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .starts_with(&prefix)
                    {
                        out.push(format!(
                            "{} (pid={})",
                            p.name().to_string_lossy(),
                            pid
                        ));
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| crate::shared::AppError::UseCase(format!("process_guard join: {e}")))?
    }

    async fn kill_blocking(&self, _processes: &[String]) -> AppResult<()> {
        // 强制 kill 需要用户二次确认;此处只占位
        Ok(())
    }
}
