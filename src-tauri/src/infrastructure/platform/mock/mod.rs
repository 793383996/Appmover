//! 非 Windows 平台(开发/单元测试)的桩实现。
//!
//! 跑在 macOS / Linux 上 `cargo check` 时使用,提供最小可用行为:
//! - 注册表读取:返回空列表
//! - Junction 创建:`unimplemented!` + 显式错误
//! - 进程占用:返回空
//! - 磁盘列表:用 `sysinfo` 抽一条根盘信息

pub mod junction;
pub mod process;
pub mod registry;
