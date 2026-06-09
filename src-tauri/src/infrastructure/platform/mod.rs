//! 平台门控:Windows / macOS / Linux 不同实现。
//!
//! 实际实现分散在 `windows/` 与 `mock/` 子模块中,
//! 通过 `pub use` 按 `#[cfg(windows)]` 重新导出。

pub mod mock;
pub mod windows;
