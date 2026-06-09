//! 基础设施层(Infrastructure Layer)。
//!
//! Clean Architecture 的最外层,提供 domain 层 trait 的具体实现。
//! 平台门控:`#[cfg(windows)]` 走 Win API,`#[cfg(not(windows))]` 走 mock。

pub mod platform;
pub mod repositories;
pub mod services;
