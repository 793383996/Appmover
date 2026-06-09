//! 进程占用检测:扫描 `sysinfo`,看是否有进程的 exe 路径在 install_path 下。

use crate::domain::value_objects::AppPath;
use crate::shared::AppResult;
use async_trait::async_trait;

#[async_trait]
pub trait ProcessGuard: Send + Sync {
    /// 返回占用该路径的所有进程 (`name (pid)` 形式)。
    async fn find_blocking_processes(&self, path: &AppPath) -> AppResult<Vec<String>>;

    /// 强制结束占用进程(慎用,需用户二次确认)。
    async fn kill_blocking(&self, processes: &[String]) -> AppResult<()>;
}
