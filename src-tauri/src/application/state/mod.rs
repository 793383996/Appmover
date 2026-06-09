//! MVI 中的 **M**(Model)—— 应用状态。

pub mod app_state;

pub use app_state::{AppState, FilterMode, LoadingKind, SortMode, Toast, ToastKind, UiState};
