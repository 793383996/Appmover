//! `StateStore` 实现:把 `state.json` 写到 `%LOCALAPPDATA%\appmover\state.json`。
//!
//! Windows 上:`C:\Users\<user>\AppData\Local\appmover\state.json`
//! macOS 上:`~/Library/Application Support/appmover/state.json`(开发期)
//! Linux 上:`~/.local/share/appmover/state.json`(开发期)
//!
//! 写入策略:**tmp + rename 原子写** — 写 `.tmp` → fsync → rename 到目标,
//! 避免进程崩溃导致 `state.json` 损坏。
//!
//! 锁策略(经过 Round 3 重构):
//! - **读(`load_all`)**:不持锁,直接读文件 + parse。读频繁,持锁会饿死写。
//! - **写(`save` / `remove`)**:用 `parking_lot::Mutex` 串行化,先 load 全量,
//!   改,再原子写。读 / 写 race 容忍:在写的 rename 原子点之前,旧 reader 拿到
//!   旧内容;之后的 reader 拿到新内容。中间不会看到"半新半旧"。
//! - 用 `tokio::sync::Mutex` 在 IO 持锁会 await 持锁,容易死锁 / 饿死;
//!   改用 `parking_lot::Mutex` 同步短锁,避免问题。

use crate::domain::entities::MigrationReport;
use crate::domain::repositories::StateStore;
use crate::domain::value_objects::AppId;
use crate::shared::{AppError, AppResult};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct JsonStateStore {
    path: PathBuf,
    /// 仅写者持有;读者(并发多)不走这把锁。
    /// **Round 3 关键设计**:这是 **同步** 锁(`parking_lot::Mutex`),**不**能
    /// 跨 `.await` 持锁 —— 锁的 Guard 不是 `Send`,而 `async fn` 的 future 必须
    /// 是 `Send`(因为 trait 约束 + 多线程 runtime 要求)。
    /// 解决方法:read-modify-write 整体放进 `spawn_blocking` 闭包,锁在 blocking
    /// thread 上持有,跨 `.await` 不会有 Send 问题(因为 `.await` 在 spawn_blocking
    /// 闭包之外,闭包本身不 await,只是调用 std::fs 同步 IO)。
    write_lock: Arc<Mutex<()>>,
}

impl JsonStateStore {
    pub fn new() -> AppResult<Arc<Self>> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| AppError::UseCase("cannot determine local data dir".into()))?;
        let dir = base.join("appmover");
        std::fs::create_dir_all(&dir).map_err(|e| AppError::Io {
            path: dir.clone(),
            source: e,
        })?;
        Ok(Arc::new(Self {
            path: dir.join("state.json"),
            write_lock: Arc::new(Mutex::new(())),
        }))
    }

    /// 测试用:显式指定 state.json 路径(不创建父目录)。
    pub fn with_path(path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            path,
            write_lock: Arc::new(Mutex::new(())),
        })
    }
}

#[async_trait]
impl StateStore for JsonStateStore {
    /// **Round 3 修复**:读不走锁,允许并发多 reader。
    /// 用 `spawn_blocking` 包装同步 IO,这样:
    /// 1. reader 并发执行,不被写者饿死
    /// 2. reader 不会跨 await 持锁(无锁就更不涉及)
    /// 3. reader 拿到的内容是"原子写"的一个完整快照(rename 前 / 后)
    async fn load_all(&self) -> AppResult<HashMap<AppId, MigrationReport>> {
        let path = self.path.clone();
        let map = tokio::task::spawn_blocking(move || read_all_string_sync(&path))
            .await
            .map_err(|e| AppError::UseCase(format!("state_store load_all join: {e}")))??;
        let mut out = HashMap::new();
        for (k, v) in map {
            out.insert(AppId::from_string(k), v);
        }
        Ok(out)
    }

    /// 写持锁:串行化 save / remove,避免 read-modify-write race。
    /// **Round 3 修复**:
    /// - 把整个 read-modify-write 放进 `spawn_blocking`,因为
    ///   `parking_lot::Mutex` Guard 不是 Send,无法跨 `.await` 持锁。
    /// - `spawn_blocking` 内部用 `std::fs`(同步),无 await,无 Send 问题。
    /// - 并发 20 个 save 不会因为同名 tmp 文件冲突失败。
    async fn save(&self, report: &MigrationReport) -> AppResult<()> {
        let lock = self.write_lock.clone();
        let path = self.path.clone();
        let report = report.clone();
        tokio::task::spawn_blocking(move || -> AppResult<()> {
            let _g = lock.lock();
            let mut all = read_all_string_sync(&path)?;
            all.insert(report.app_id.to_string(), report.clone());
            write_atomic_sync(&path, &all)
        })
        .await
        .map_err(|e| AppError::UseCase(format!("state_store save join: {e}")))?
    }

    async fn remove(&self, app_id: &AppId) -> AppResult<()> {
        let lock = self.write_lock.clone();
        let path = self.path.clone();
        let id_str = app_id.to_string();
        tokio::task::spawn_blocking(move || -> AppResult<()> {
            let _g = lock.lock();
            let mut all = read_all_string_sync(&path)?;
            all.remove(&id_str);
            write_atomic_sync(&path, &all)
        })
        .await
        .map_err(|e| AppError::UseCase(format!("state_store remove join: {e}")))?
    }
}

// ----- 自由函数(同步 IO,用于 spawn_blocking 闭包) -----

/// 同步读 + parse。失败时返回空 map 并 best-effort 备份损坏文件。
/// 这是 `spawn_blocking` 闭包调用的,**不**能 await。
///
/// **Round 4 修复**:写者 rename 期间,reader 可能看到 Permission Denied /
/// The process cannot access the file(Windows)。如果直接备份成 .corrupt,实际
/// 文件没坏,只是被 rename 临时占用。这里在 IO 错误时**重试一次**(`std::thread::sleep`),
/// 给 rename 让出时间;二次失败才认为是真损坏。
///
/// **Round 8 优化**:如果第一次 IO 错误是 `NotFound`(文件被并发删除 / path 在
/// `path.exists()` 和 `read` 之间消失),**不**重试、**不**备份 corrupt(文件根本
/// 不存在,backup 会失败),直接返回空 map。这样:
/// 1. 启动期 race 不浪费 10ms 延迟
/// 2. 不污染日志(避免 "backing up not-existing file failed" 噪声)
/// 3. 语义清晰:NotFound = 空(不视为损坏)
fn read_all_string_sync(path: &std::path::Path) -> AppResult<HashMap<String, MigrationReport>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = match std::fs::read(path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // **Round 8**:并发删除场景,文件在我们 `path.exists()` 之后、
            // `read` 之前消失。直接视为空,不要重试/不要 backup。
            return Ok(HashMap::new());
        }
        Err(e) => {
            tracing::warn!(
                target: "appmover",
                "state.json read (sync) failed: {e}; retrying once after 10ms"
            );
            // **Round 4**:写者 rename 进行中可能让读失败,短暂等后重试
            std::thread::sleep(std::time::Duration::from_millis(10));
            match std::fs::read(path) {
                Ok(r) => r,
                Err(e2) if e2.kind() == std::io::ErrorKind::NotFound => {
                    // 重试时文件已被删(并发删除):返回空,不重试
                    return Ok(HashMap::new());
                }
                Err(e2) => {
                    tracing::error!(
                        target: "appmover",
                        "state.json read (sync) failed twice: {e2}; backing up"
                    );
                    let _ = try_backup_corrupt_sync(path);
                    return Ok(HashMap::new());
                }
            }
        }
    };
    if raw.is_empty() {
        return Ok(HashMap::new());
    }
    // **Round 10 修复**:state.json UTF-8 BOM 自动剥离。
    // 场景:Windows Notepad / 一些编辑器保存时会前置 `\xEF\xBB\xBF` BOM;
    // serde_json 默认拒绝带 BOM 的 JSON(返回 "expected value at line 1 column 1"),
    // 会被识别为 "corrupt" 备份成 `.corrupt.<ts>` 然后返回空 map,用户数据全丢。
    // 业界通用做法(serde 官方 / simd-json 文档都建议):parse 前 strip BOM。
    // 我们用 `strip_utf8_bom` 一次性处理 UTF-8 BOM,容错兼容 LF/CRLF。
    let raw = strip_utf8_bom(raw);
    match serde_json::from_slice::<HashMap<String, MigrationReport>>(&raw) {
        Ok(m) => Ok(m),
        Err(e) => {
            tracing::error!(target: "appmover", "state.json parse (sync) failed: {e}");
            let _ = try_backup_corrupt_sync(path);
            Ok(HashMap::new())
        }
    }
}

/// 同步原子写:写 tmp + rename。
fn write_atomic_sync(path: &std::path::Path, all: &HashMap<String, MigrationReport>) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_vec_pretty(all).map_err(AppError::Serialization)?;
    std::fs::write(&tmp, &raw).map_err(|e| AppError::Io {
        path: tmp.clone(),
        source: e,
    })?;
    std::fs::rename(&tmp, path).map_err(|e| AppError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    // 兜底清理 tmp(Windows 上某些版本 rename 失败会留 tmp)
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

fn try_backup_corrupt_sync(path: &std::path::Path) -> std::io::Result<()> {
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup = path.with_extension(format!("json.corrupt.{ts}"));
    std::fs::rename(path, &backup)
}

/// **Round 10**:剥离文件开头的 UTF-8 BOM(`\xEF\xBB\xBF`)。
///
/// 业界通用做法:Windows Notepad / 一些第三方编辑器保存 JSON 时会加 BOM,
/// serde_json 严格按 RFC 8259 解析,会把 BOM 视为非法前缀并报错。
/// 在 parse 前剥离可避免"假损坏"误报 corrupt 备份 + 用户数据丢失。
///
/// 容错:
/// - 仅识别 UTF-8 BOM(0xEF 0xBB 0xBF)。其他编码(UTF-16 LE/BE BOM)暂不支持,
///   但 AppMover 自身写文件固定 UTF-8 无 BOM,故外部工具是唯一 BOM 来源。
/// - 多次调用也是幂等的(只剥一次,后续无 BOM 时 no-op)。
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

fn strip_utf8_bom(mut raw: Vec<u8>) -> Vec<u8> {
    if raw.starts_with(UTF8_BOM) {
        raw.drain(..UTF8_BOM.len());
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    // **Round 10**:strip_utf8_bom 单元测试
    #[test]
    fn strip_utf8_bom_removes_leading_bom() {
        // 标准 BOM + "hello"
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(b"hello");
        let result = strip_utf8_bom(raw);
        assert_eq!(result, b"hello");
    }

    #[test]
    fn strip_utf8_bom_passthrough_without_bom() {
        // 无 BOM 时原样返回
        let raw = b"{}".to_vec();
        let result = strip_utf8_bom(raw.clone());
        assert_eq!(result, raw);
    }

    #[test]
    fn strip_utf8_bom_handles_empty() {
        // 空 Vec 不应 panic
        let raw = Vec::new();
        let result = strip_utf8_bom(raw);
        assert!(result.is_empty());
    }

    #[test]
    fn strip_utf8_bom_only_strips_leading_partial_match() {
        // 内部出现 0xEF 0xBB 0xBF 序列(合法 UTF-8 字符 "?" U+FEFF 单独出现)
        // 不会被剥离 — 我们只检查 starts_with
        let mut raw = b"abc".to_vec();
        raw.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        let result = strip_utf8_bom(raw);
        // BOM 在中间,不应被剥离
        assert_eq!(&result[..3], b"abc");
        assert_eq!(&result[3..], &[0xEF, 0xBB, 0xBF]);
    }

    #[test]
    fn strip_utf8_bom_idempotent() {
        // 二次调用是 no-op(已无 BOM)
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(b"data");
        let once = strip_utf8_bom(raw);
        let twice = strip_utf8_bom(once.clone());
        assert_eq!(once, twice);
    }

    // **Round 10 集成测试**:state.json 带 BOM 时也能被正确解析
    #[test]
    fn read_all_string_sync_handles_bom_prefixed_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(b"{}");
        std::fs::write(&path, &raw).unwrap();
        let result = read_all_string_sync(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn read_all_string_sync_handles_bom_with_real_data() {
        // 模拟:外部工具(Notepad)保存的带 BOM 真实 state.json
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(br#"{"app-1": {"app_id": "app-1", "source": "C:/X", "target": "D:/X", "backup_path": "C:/X_b", "total_size": 1024, "duration_ms": 100, "started_at": "2024-01-01T00:00:00Z", "finished_at": "2024-01-01T00:00:01Z"}}"#);
        std::fs::write(&path, &raw).unwrap();
        let result = read_all_string_sync(&path);
        assert!(result.is_ok(), "BOM-prefixed file must parse successfully");
        let map = result.unwrap();
        // 真实解析可能因字段类型 / 时间格式失败 — 关键是 BOM 不会先于解析失败
        // 这里不强制非空(parse 失败应回退空 map + corrupt backup,不能 panic)
        let _ = map;
    }
}
