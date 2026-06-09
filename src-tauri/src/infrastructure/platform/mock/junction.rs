//! Mock Junction —— 在非 Windows 平台上做"软通过"。
//!
//! 设计意图:`AppMover` 是 Windows 工具,生产只跑 Windows。non-windows 是
//! 开发 / CI 平台,需要让 FileMigrationRepository 端到端测试能跑通。
//! 软通过:create 假装成功、remove 假装成功。这样集成测试可以验证 copy +
//! rename + verify 逻辑,而不依赖真实 reparse point。
//! 真正的"only-Windows"行为在 CLI 启动时(`bin/cli.rs`)做硬提示。

use crate::domain::value_objects::AppPath;
use crate::shared::AppResult;

pub fn create_junction(_link: &AppPath, _target: &AppPath) -> AppResult<()> {
    Ok(())
}

pub fn remove_junction(_link: &AppPath) -> AppResult<()> {
    Ok(())
}
