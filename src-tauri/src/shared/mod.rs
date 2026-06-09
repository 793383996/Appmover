//! 横切关注点:错误、结果、日志、ID、时间格式化等。

pub mod error;
pub mod logger;
pub mod result;

pub use error::{AppError, AppResult};
