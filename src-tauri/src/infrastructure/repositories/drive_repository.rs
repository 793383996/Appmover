//! `DriveRepository` 实现:用 `sysinfo::Disks` 枚举所有盘符。

use crate::domain::entities::DriveInfo;
use crate::domain::repositories::DriveRepository;
use crate::domain::value_objects::{ByteSize, DriveLetter};
use crate::shared::AppResult;
use async_trait::async_trait;
use std::sync::Arc;
use sysinfo::Disks;

pub struct SysinfoDriveRepository;

impl SysinfoDriveRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl DriveRepository for SysinfoDriveRepository {
    /// **Round 3 修复**:`Disks::new_with_refreshed_list()` 是同步阻塞 IO(读
    /// 设备/挂载点表),在 macOS 上尤其慢。`async fn` 直接调会卡死 tokio worker。
    /// 用 `spawn_blocking` 包装,符合 tokio 官方对阻塞 IO 的标准处理。
    async fn list_all(&self) -> AppResult<Vec<DriveInfo>> {
        tokio::task::spawn_blocking(|| -> AppResult<Vec<DriveInfo>> {
            let disks = Disks::new_with_refreshed_list();
            let mut out = Vec::new();
            for d in &disks {
                let mount = d.mount_point().to_string_lossy().to_string();
                let letter = DriveLetter::from_path(d.mount_point()).unwrap_or_else(|| {
                    // 兜底:取 mount_point 第一个字符
                    DriveLetter::raw(mount.clone())
                });
                let total = ByteSize(d.total_space());
                let available = ByteSize(d.available_space());
                let is_system = Self::is_system_disk(d);
                out.push(DriveInfo {
                    letter,
                    mount_point: mount.trim_end_matches('\\').trim_end_matches('/').to_string(),
                    label: d.name().to_str().map(|s| s.to_string()),
                    file_system: Some(d.file_system().to_string_lossy().to_string()),
                    total,
                    available,
                    is_system,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| crate::shared::AppError::UseCase(format!("list_all join: {e}")))?
    }
}

impl SysinfoDriveRepository {
    fn is_system_disk(d: &sysinfo::Disk) -> bool {
        #[cfg(windows)]
        {
            // Windows: mount_point 是 `C:\`,盘符为 `C:`
            d.mount_point().to_string_lossy().to_ascii_uppercase().starts_with("C:")
        }
        #[cfg(not(windows))]
        {
            // 类 Unix:根盘符即为系统盘
            d.mount_point().to_string_lossy() == "/"
        }
    }
}
