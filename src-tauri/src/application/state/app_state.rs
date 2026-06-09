//! 应用层状态:UI 状态 + 业务状态(应用列表 / 磁盘 / 迁移 / 已迁移)。
//!
//! 整个应用在任一时刻只有 1 个 `AppState` 实例。
//! 状态是不可变的,变更通过 reducer 产生新 state(用 `with_*` 风格)。

use crate::domain::entities::{DriveInfo, InstalledApp, MigrationReport, MigrationStatus};
use crate::domain::value_objects::{AppId, ByteSize, DriveLetter};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// UI 过滤模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    All,
    /// 大于 100MB
    LargeOnly,
    /// 可迁移(非系统组件、PathGuard 通过)
    Migratable,
}

/// 列表排序方式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    Name,
    #[default]
    Size,
    Publisher,
}

/// Toast 通知(临时 UI 提示)。
///
/// `generation` 单调递增,用于 DismissToast 精准只 dismiss 自己触发的那次,
/// 避免新 toast 被旧 toast 的自动 dismiss 误清。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toast {
    pub kind: ToastKind,
    pub message: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// 顶部加载状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadingKind {
    Idle,
    Scanning,
    LoadingDrives,
    CalculatingSize,
    Migrating,
}

/// UI 状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    /// 选中的应用
    pub selected: HashSet<AppId>,
    /// 目标盘
    pub target_drive: Option<DriveLetter>,
    /// 搜索关键词
    pub search: String,
    pub filter: FilterMode,
    pub sort: SortMode,
    /// 加载状态
    pub loading: LoadingKind,
    /// 顶部 toast
    pub toast: Option<Toast>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected: HashSet::new(),
            target_drive: None,
            search: String::new(),
            filter: FilterMode::Migratable,
            sort: SortMode::Size,
            loading: LoadingKind::Idle,
            toast: None,
        }
    }
}

/// 应用状态 —— 整个应用的 single source of truth。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    /// 扫描得到的应用列表
    pub apps: Vec<InstalledApp>,
    /// 磁盘列表
    pub drives: Vec<DriveInfo>,
    /// 正在迁移中的应用状态
    pub migrations: HashMap<AppId, MigrationStatus>,
    /// 已完成迁移的应用(state.json)
    pub migrated: HashMap<AppId, MigrationReport>,
    /// UI 状态
    pub ui: UiState,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 经过过滤 + 排序 + 搜索的视图(给前端列表用)。
    pub fn filtered_apps(&self) -> Vec<&InstalledApp> {
        let mut list: Vec<&InstalledApp> = self
            .apps
            .iter()
            .filter(|a| match self.ui.filter {
                FilterMode::All => true,
                FilterMode::LargeOnly => a
                    .actual_size
                    .or(a.estimated_size)
                    .map(|s| s >= ByteSize::MB * 100)
                    .unwrap_or(false),
                FilterMode::Migratable => a.is_migratable(),
            })
            .filter(|a| {
                if self.ui.search.is_empty() {
                    true
                } else {
                    let q = self.ui.search.to_ascii_lowercase();
                    a.display_name.to_ascii_lowercase().contains(&q)
                        || a.publisher
                            .as_deref()
                            .map(|p| p.to_ascii_lowercase().contains(&q))
                            .unwrap_or(false)
                }
            })
            .collect();
        list.sort_by(|a, b| match self.ui.sort {
            SortMode::Name => a.display_name.cmp(&b.display_name),
            SortMode::Publisher => a.publisher.cmp(&b.publisher),
            SortMode::Size => b
                .actual_size
                .or(b.estimated_size)
                .cmp(&a.actual_size.or(a.estimated_size)),
        });
        list
    }

    /// 选中项的总估算大小。
    pub fn selected_total_size(&self) -> ByteSize {
        self.apps
            .iter()
            .filter(|a| self.ui.selected.contains(&a.id))
            .map(|a| a.actual_size.or(a.estimated_size).unwrap_or(ByteSize::ZERO))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app(name: &str, size_mb: u64) -> InstalledApp {
        use crate::domain::value_objects::AppPath;
        InstalledApp {
            id: AppId::new(),
            source: crate::domain::entities::AppSource::Hklm64,
            display_name: name.into(),
            publisher: Some("Test".into()),
            display_version: None,
            install_location: AppPath::new("C:/Program Files/Test").unwrap(),
            uninstall_string: None,
            display_icon: None,
            estimated_size: Some(ByteSize::MB * size_mb),
            actual_size: None,
            release_type: None,
            parent_key_name: None,
        }
    }

    #[test]
    fn filtered_by_size() {
        let mut s = AppState::new();
        s.apps = vec![make_app("A", 10), make_app("B", 200), make_app("C", 50)];
        s.ui.filter = FilterMode::LargeOnly;
        let v = s.filtered_apps();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].display_name, "B");
    }

    #[test]
    fn sorted_by_size_desc() {
        let mut s = AppState::new();
        s.apps = vec![make_app("A", 10), make_app("B", 200), make_app("C", 50)];
        s.ui.sort = SortMode::Size;
        let v = s.filtered_apps();
        assert_eq!(v[0].display_name, "B");
        assert_eq!(v[1].display_name, "C");
        assert_eq!(v[2].display_name, "A");
    }
}
