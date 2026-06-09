//! Tauri commands 集合。

pub mod handlers;

pub use handlers::{deps_info, dispatch_intent, get_state, version, StoreHandle};
