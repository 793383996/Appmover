//! 通用服务:Junction 创建 / 复制引擎 / 状态文件 IO / ACL 保留等。
//!
//! 这些不是仓储,但同样属于"基础设施"层。

pub mod acl_preserver;
pub mod copy_engine;
pub mod junction_service;
