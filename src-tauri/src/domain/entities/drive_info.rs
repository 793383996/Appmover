//! 磁盘信息实体。

use crate::domain::value_objects::{ByteSize, DriveLetter};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveInfo {
    pub letter: DriveLetter,
    /// 盘符根路径,Windows 上 = `C:\`,macOS 上 = `/`
    pub mount_point: String,
    /// 卷标
    pub label: Option<String>,
    /// 文件系统(NTFS / FAT32 / exFAT / APFS ...)
    pub file_system: Option<String>,
    /// 总容量
    pub total: ByteSize,
    /// 可用空间
    pub available: ByteSize,
    /// 是否可作为目标盘(系统盘 C: 默认不可)
    pub is_system: bool,
}

impl DriveInfo {
    /// 空间使用率(0.0 - 1.0)。
    pub fn usage_ratio(&self) -> f64 {
        if self.total.as_bytes() == 0 {
            return 0.0;
        }
        let used = self.total.as_bytes().saturating_sub(self.available.as_bytes());
        used as f64 / self.total.as_bytes() as f64
    }
}
