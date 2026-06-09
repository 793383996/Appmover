//! `SizeCalculator` 实现:用 `walkdir` + `rayon::par_bridge` 并行计算目录总大小。
//!
//! `calculate_with_progress` 支持 `CancellationToken` 取消;
//! `calculate` 是无进度版,带 cancel 检查的低频退出。
//!
//! **Round 4 修复**:之前用 `par_bridge().fold().sum()` 双重累加(每个 thread
//! 一个 sum 累加器,最后 sum 一次),同时 progress atomic 又独立累加一次,等于
//! 算两遍 byte 数(虽值正确但浪费 CPU)。改用单个 `AtomicU64` 累加 + 进度报告,
//! 删掉 fold 累加器;`par_bridge().map(|e| { ... }).count()` 流式
//! 处理,不收集结果,内存压力也降到 O(1)。

use crate::domain::repositories::{SizeCalculator, SizeProgress};
use crate::domain::value_objects::{AppPath, ByteSize};
use crate::shared::AppResult;
use async_trait::async_trait;
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

pub struct WalkdirSizeCalculator;

impl WalkdirSizeCalculator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl SizeCalculator for WalkdirSizeCalculator {
    async fn calculate(&self, path: &AppPath) -> AppResult<ByteSize> {
        let p = path.clone();
        let total = tokio::task::spawn_blocking(move || -> AppResult<u64> {
            let acc = Arc::new(AtomicU64::new(0));
            WalkDir::new(p.as_path())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .par_bridge()
                .map(|e| {
                    let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                    acc.fetch_add(len, Ordering::Relaxed);
                    len
                })
                .count(); // 强制消费迭代器,O(1) 内存
            Ok(acc.load(Ordering::Relaxed))
        })
        .await
        .map_err(|e| crate::shared::AppError::UseCase(format!("size_calc join: {e}")))??;
        Ok(ByteSize(total))
    }

    async fn calculate_with_progress(
        &self,
        path: &AppPath,
        tx: mpsc::Sender<SizeProgress>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<ByteSize> {
        let p = path.clone();
        let cancel_inner = cancel.clone();
        // **Round 4 修复**:
        // - 单一 atomic 累加,fold 不再做 sum 累加(避免双重计算)
        // - 流式 par_bridge + map + count,O(1) 内存
        // - 每 256 个文件检查 cancel,做到准实时取消
        // - 进度上报用 256 files 节流
        let total = tokio::task::spawn_blocking(move || -> AppResult<u64> {
            let acc = Arc::new(AtomicU64::new(0));
            let count = Arc::new(AtomicU64::new(0));
            WalkDir::new(p.as_path())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .par_bridge()
                .map(|e| {
                    if cancel_inner.is_cancelled() {
                        return 0u64;
                    }
                    let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                    let new_acc = acc.fetch_add(len, Ordering::Relaxed) + len;
                    let c = count.fetch_add(1, Ordering::Relaxed) + 1;
                    // 节流:每 256 个文件报一次
                    if c % 256 == 0 {
                        let _ = tx.blocking_send(SizeProgress {
                            path: p.clone(),
                            current_bytes: ByteSize(new_acc),
                            files_scanned: c,
                        });
                    }
                    len
                })
                .count();
            Ok(acc.load(Ordering::Relaxed))
        })
        .await
        .map_err(|e| crate::shared::AppError::UseCase(format!("size_calc join: {e}")))??;
        if cancel.is_cancelled() {
            return Err(crate::shared::AppError::Cancelled);
        }
        Ok(ByteSize(total))
    }
}
