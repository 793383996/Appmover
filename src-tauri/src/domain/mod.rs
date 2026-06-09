//! 领域层(Domain Layer)。
//!
//! Clean Architecture 的最内层,定义业务规则、实体、值对象、仓储接口、UseCase。
//!
//! 依赖约束:
//! - 只能依赖 `shared` 层(错误、结果类型)
//! - 绝不依赖 `application` / `infrastructure` / `presentation`
//! - 不引入 IO、UI、网络、文件系统的具体 API

pub mod entities;
pub mod repositories;
pub mod usecases;
pub mod value_objects;
