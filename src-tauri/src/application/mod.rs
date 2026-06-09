//! 应用层(Application Layer)。
//!
//! MVI 模式:
//! - `state`  —— 不可变应用状态
//! - `intent` —— 前端发来的意图(命令)
//! - `reducer`—— 纯函数:`(State, Intent) → State`
//! - `effect` —— 副作用:发事件、调外部、推进度
//! - `di`     —— 依赖注入容器,组合所有 UseCase 与仓储

pub mod di;
pub mod effect;
pub mod intent;
pub mod reducer;
pub mod state;
