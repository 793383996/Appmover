//! 复制引擎:`tokio::fs` + 分块 1MB 复制。
//!
//! 进度通过 `mpsc::Sender<CopyProgress>` 上报,256ms 节流。
//! 取消:`CancellationToken` 在每个 await 点检查。

use crate::domain::repositories::CopyProgress;
use crate::domain::value_objects::{AppId, ByteSize};
use crate::shared::AppResult;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const CHUNK_SIZE: usize = 1024 * 1024; // 1MB
const PROGRESS_THROTTLE_MS: u64 = 256;

pub struct CopyEngine;

impl CopyEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// 递归复制整个目录。
    /// 返回总字节数(已写入)。
    /// **Round 3 修复**:`tx` 改为 `Option`,None 时不发进度事件(避免无 consumer 的 send 失败)。
    /// **Round 4 修复**:`cancel` 透传到 `copy_file`,每 chunk 检查取消,粒度从
    /// "每文件"细化到"每 1MB"。大文件(几 GB)复制中也能准实时取消。
    pub async fn copy_dir(
        &self,
        src: &std::path::Path,
        dst: &std::path::Path,
        app_id: &AppId,
        total_estimate: ByteSize,
        tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<u64> {
        tokio::fs::create_dir_all(dst).await.map_err(|e| crate::shared::AppError::Io {
            path: dst.to_path_buf(),
            source: e,
        })?;
        let mut total: u64 = 0;
        let mut entries = tokio::fs::read_dir(src).await.map_err(|e| crate::shared::AppError::Io {
            path: src.to_path_buf(),
            source: e,
        })?;
        let started = Instant::now();
        let mut last_emit = Instant::now();

        // **Round 6 优化**:把 `tx` / `cancel` 的 clone 提到循环外(原代码每 entry
        // 都 clone 一次,大目录(几千文件)会做几千次 `Arc` clone,虽小但累积
        // 起来 + 递归内层更多次),改为循环内复用 capture。
        // - tx 是 `Option<Sender>`,Sender 内部是 Arc,clone 是 refcount inc。
        // - cancel 是 `Arc<CancellationToken>`,同理。
        // 这里不直接借用(&Sender / &Arc)是因为 `Box::pin(self.copy_dir(...))` 递归
        // 调用需要 Send + 'static,需要 owned value。改用 entry loop 内的 clone 一次。
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| crate::shared::AppError::Io {
                path: src.to_path_buf(),
                source: e,
            })?
        {
            if cancel.is_cancelled() {
                return Err(crate::shared::AppError::Cancelled);
            }
            let from = entry.path();
            let to = dst.join(entry.file_name());
            let ft = entry
                .file_type()
                .await
                .map_err(|e| crate::shared::AppError::Io {
                    path: from.clone(),
                    source: e,
                })?;
            if ft.is_dir() {
                // 递归:仍然需要 owned Sender/Token(因为 future 是 'static)
                // 但每个分支只 clone 一次,而不是原代码中即使走 file 路径也 clone
                let tx_child = tx.clone();
                let cancel_child = cancel.clone();
                Box::pin(self.copy_dir(&from, &to, app_id, total_estimate, tx_child, cancel_child))
                    .await
                    // **Round 4**:把 copy_file 返回的 Interrupted io error
                    // 翻译成 AppError::Cancelled,让外层 migrate_repo 走取消分支
                    .map_err(|e| match e {
                        crate::shared::AppError::Io { source, .. }
                            if source.kind() == std::io::ErrorKind::Interrupted =>
                        {
                            crate::shared::AppError::Cancelled
                        }
                        other => other,
                    })?;
            } else {
                let cancel_child = cancel.clone();
                total += self
                    .copy_file(&from, &to, cancel_child)
                    .await
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::Interrupted {
                            crate::shared::AppError::Cancelled
                        } else {
                            crate::shared::AppError::Io {
                                path: to.clone(),
                                source: e,
                            }
                        }
                    })?;
                // 节流
                if let Some(sender) = &tx {
                    if last_emit.elapsed().as_millis() as u64 >= PROGRESS_THROTTLE_MS {
                        let elapsed = started.elapsed().as_secs_f64().max(0.001);
                        let speed = (total as f64 / elapsed) as u64;
                        // **Round 6**:末尾 force send 也加 cancel 检查,
                        // 取消时不再发最后一次(让前端的"已取消"状态保持干净)
                        if cancel.is_cancelled() {
                            return Err(crate::shared::AppError::Cancelled);
                        }
                        let _ = sender
                            .send(CopyProgress {
                                app_id: app_id.clone(),
                                copied: ByteSize(total),
                                total: total_estimate,
                                speed_bps: speed,
                            })
                            .await;
                        last_emit = Instant::now();
                    }
                }
            }
        }
        // 末尾强制报一次最终值
        if let Some(sender) = &tx {
            // **Round 6**:取消时不再发送"100% 完成"事件(避免前端看到已取消
            // 后又出现"完成"的不一致)。
            if cancel.is_cancelled() {
                return Err(crate::shared::AppError::Cancelled);
            }
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed = (total as f64 / elapsed) as u64;
            let _ = sender
                .send(CopyProgress {
                    app_id: app_id.clone(),
                    copied: ByteSize(total),
                    total: total_estimate,
                    speed_bps: speed,
                })
                .await;
        }
        Ok(total)
    }

    /// **Round 5 修复**:用 `tokio::select!` 让正在进行的 `read` 可以被 cancel。
    /// Round 4 只在每 chunk **开头**查 cancel,慢设备上的 large chunk read
    /// (4096 bytes 的 tokio read 其实很快,但对于几 GB 文件,读完后才检查≈几秒 delay)。
    /// 改为 `tokio::select!` 在 read 和 cancel 之间竞速:一旦 cancel,立即中断 read。
    /// 取消时返回 `Err(io::ErrorKind::Interrupted)` — 调用方 `copy_dir` 翻译为 `AppError::Cancelled`。
    async fn copy_file(
        &self,
        from: &std::path::Path,
        to: &std::path::Path,
        cancel: Arc<CancellationToken>,
    ) -> std::io::Result<u64> {
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut src = tokio::fs::File::open(from).await?;
        let mut dst = tokio::fs::File::create(to).await?;
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut written = 0u64;
        loop {
            // **Round 5**:tokio::select! 竞速 read vs cancel
            let n = tokio::select! {
                res = src.read(&mut buf) => res?,
                () = cancel.cancelled() => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "copy cancelled mid-chunk",
                    ));
                }
            };
            if n == 0 {
                break;
            }
            // write 阶段也可以 cancel,但不 select(write 通常很快)
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "copy cancelled mid-write",
                ));
            }
            dst.write_all(&buf[..n]).await?;
            written += n as u64;
        }
        dst.flush().await?;
        Ok(written)
    }
}
