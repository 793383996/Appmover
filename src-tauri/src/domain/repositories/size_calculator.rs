//! 大小计算器:并行计算目录真实占用字节数。

use crate::domain::value_objects::{AppPath, ByteSize};
use crate::shared::AppResult;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

#[async_trait]
pub trait SizeCalculator: Send + Sync {
    /// 同步版本:返回总字节数,内部已并行。
    async fn calculate(&self, path: &AppPath) -> AppResult<ByteSize>;

    /// 进度版:每算完一个文件发一次 `SizeProgress` 到 `tx`。
    async fn calculate_with_progress(
        &self,
        path: &AppPath,
        tx: mpsc::Sender<SizeProgress>,
        cancel: Arc<tokio_util::sync::CancellationToken>,
    ) -> AppResult<ByteSize>;
}

/// 进度事件。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SizeProgress {
    pub path: AppPath,
    pub current_bytes: ByteSize,
    pub files_scanned: u64,
}
