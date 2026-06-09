//! 已安装应用实体。
//!
//! 表达"被 Windows 注册表识别出来的一个可被迁移的程序"。
//! 与 `MigrationPlan` 解耦:同一应用可以被多次规划(目标盘不同)。

use crate::domain::value_objects::{AppId, AppPath, ByteSize};
use serde::{Deserialize, Serialize};

/// 应用来源(注册表 hive)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    /// 64 位应用 HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
    Hklm64,
    /// 32 位应用 HKLM\SOFTWARE\WOW6432Node\...\Uninstall
    Hklm32,
    /// 当前用户 HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
    Hkcu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    /// 业务 ID(去重后生成,稳定)
    pub id: AppId,
    /// 注册表来源
    pub source: AppSource,
    /// 显示名
    pub display_name: String,
    /// 发布者
    pub publisher: Option<String>,
    /// 版本
    pub display_version: Option<String>,
    /// 安装位置(必须是 C:\ 开头才被纳入迁移候选)
    pub install_location: AppPath,
    /// 卸载命令行
    pub uninstall_string: Option<String>,
    /// 显示图标
    pub display_icon: Option<String>,
    /// 注册表自报大小(未必准确,真实大小要 `CalculateSizeUseCase` 重算)
    pub estimated_size: Option<ByteSize>,
    /// 计算得到的实际目录大小(异步填充)
    pub actual_size: Option<ByteSize>,
    /// **Round 4 新增**:ReleaseType 字段(Win10/11 补丁用,值为 "Security Update" /
    /// "Update Rollup" / etc)。为空表示正常应用;KB 启发式过滤会用到。
    pub release_type: Option<String>,
    /// **Round 4 新增**:ParentKeyName 字段。Windows 7 时代 KB 补丁会作为某个父 KB
    /// 的子项;父项缺失,该字段存在 → 子补丁。
    pub parent_key_name: Option<String>,
}

impl InstalledApp {
    /// 是否可作为迁移候选。
    pub fn is_migratable(&self) -> bool {
        self.install_location
            .as_path()
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("C:/")
    }
}
