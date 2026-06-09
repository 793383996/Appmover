//! Tauri 事件协议。

pub mod protocol;

pub use protocol::{
    LOG, MIGRATION_COMPLETED, MIGRATION_PROGRESS, SIZE_PROGRESS, STATE_CHANGED,
};
