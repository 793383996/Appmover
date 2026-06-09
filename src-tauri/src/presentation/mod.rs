//! 表现层(Presentation Layer)。
//!
//! 包含:
//! - `commands`:Tauri commands(供前端 `invoke`)
//! - `events`:Tauri events(主动 `emit` 给前端)

pub mod commands;
pub mod events;
