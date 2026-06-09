//! Reducer —— 纯函数 `(State, Intent) → State`。
//!
//! 不允许 IO,不允许 await,只做数据变换。
//! 副作用由 effect 层负责。

use crate::application::intent::Intent;
use crate::application::state::AppState;
use crate::domain::entities::MigrationStatus;

pub fn reduce(state: AppState, intent: Intent) -> AppState {
    match intent {
        // ---- 扫描结果回填 ----
        Intent::AppsScanned(apps) => AppState { apps, ..state },
        Intent::DrivesLoaded(drives) => {
            // 自动选第一个非系统盘为目标盘(若没选)
            let target = state.ui.target_drive.clone().or_else(|| {
                drives
                    .iter()
                    .find(|d| !d.is_system)
                    .map(|d| d.letter.clone())
            });
            AppState {
                drives,
                ui: crate::application::state::UiState {
                    target_drive: target,
                    ..state.ui
                },
                ..state
            }
        }
        Intent::MigratedLoaded(reports) => {
            let mut migrated = state.migrated.clone();
            for r in reports {
                migrated.insert(r.app_id.clone(), r);
            }
            AppState {
                migrated,
                ..state
            }
        }

        // ---- 选择 / 过滤 / 搜索 ----
        Intent::SelectApp { id, selected } => {
            let mut s = state;
            if selected {
                s.ui.selected.insert(id);
            } else {
                s.ui.selected.remove(&id);
            }
            s
        }
        Intent::SelectAll => {
            let mut s = state;
            s.ui.selected = s.apps.iter().map(|a| a.id.clone()).collect();
            s
        }
        Intent::ClearSelection => {
            let mut s = state;
            s.ui.selected.clear();
            s
        }
        Intent::SetTargetDrive { letter } => {
            let mut s = state;
            s.ui.target_drive = Some(letter);
            s
        }
        Intent::SetSearch { query } => {
            let mut s = state;
            s.ui.search = query;
            s
        }
        Intent::SetFilter { mode } => {
            let mut s = state;
            s.ui.filter = mode;
            s
        }
        Intent::SetSort { mode } => {
            let mut s = state;
            s.ui.sort = mode;
            s
        }

        // ---- 进度 ----
        Intent::SizeProgress { id, current } => {
            let mut s = state;
            if let Some(app) = s.apps.iter_mut().find(|a| a.id == id) {
                app.actual_size = Some(current);
            }
            s
        }

        // ---- 迁移状态机 ----
        Intent::MigrationProgress {
            id,
            copied,
            total,
            speed_bps,
        } => {
            let mut s = state;
            let id_key = id;
            let status = s
                .migrations
                .entry(id_key.clone())
                .or_insert_with(|| MigrationStatus::idle(id_key));
            status.copied_bytes = copied;
            status.total = total;
            status.speed_bps = speed_bps;
            s
        }
        Intent::MigrationPhase { id, phase } => {
            let mut s = state;
            let id_key = id;
            let status = s
                .migrations
                .entry(id_key.clone())
                .or_insert_with(|| MigrationStatus::idle(id_key));
            status.phase = phase;
            s
        }
        Intent::MigrationFailed { id, error } => {
            let mut s = state;
            let id_key = id;
            let status = s
                .migrations
                .entry(id_key.clone())
                .or_insert_with(|| MigrationStatus::idle(id_key));
            status.phase = crate::domain::entities::MigrationPhase::Failed;
            status.error = Some(error.clone());
            // 注意:不在 reducer 里直接 set toast,因为 toast 需要 generation 关联自动 dismiss。
            // 改由 effect 层在 MigrationFailed 处理时调 show_toast。
            s
        }
        Intent::MigrationCompleted { id, report } => {
            let mut s = state;
            let status = s
                .migrations
                .entry(id.clone())
                .or_insert_with(|| MigrationStatus::idle(id.clone()));
            status.phase = crate::domain::entities::MigrationPhase::Completed;
            status.copied_bytes = report.total_size;
            status.finished_at = Some(report.finished_at);
            s.migrated.insert(id, report);
            s
        }

        // ---- 系统 ----
        Intent::SetLoading { kind } => {
            let mut s = state;
            s.ui.loading = kind;
            s
        }
        Intent::ShowToast { kind, message, generation } => {
            let mut s = state;
            s.ui.toast = Some(crate::application::state::Toast { kind, message, generation });
            s
        }
        Intent::DismissToast { generation } => {
            // 只 dismiss 同 generation 的 toast:旧 toast 的延时 dismiss 不会误清新 toast
            let mut s = state;
            if let Some(t) = &s.ui.toast {
                if t.generation == generation {
                    s.ui.toast = None;
                }
            }
            s
        }

        // ---- 无状态变更的 Intent(由 effect 处理) ----
        Intent::ScanApps
        | Intent::ListDrives
        | Intent::ListMigrated
        | Intent::CalculateSizes
        | Intent::StartMigration { .. }
        | Intent::CancelMigration { .. }
        | Intent::Rollback { .. } => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{InstalledApp, MigrationPhase, MigrationReport};
    use crate::domain::value_objects::{AppId, AppPath, ByteSize};

    fn make_app(name: &str) -> InstalledApp {
        InstalledApp {
            id: AppId::new(),
            source: crate::domain::entities::AppSource::Hklm64,
            display_name: name.into(),
            publisher: None,
            display_version: None,
            install_location: AppPath::new("C:/Program Files/X").unwrap(),
            uninstall_string: None,
            display_icon: None,
            estimated_size: Some(ByteSize::MB * 100),
            actual_size: None,
            release_type: None,
            parent_key_name: None,
        }
    }

    #[test]
    fn select_toggles() {
        let mut s = AppState::new();
        s.apps = vec![make_app("A")];
        let id = s.apps[0].id.clone();
        s = reduce(s, Intent::SelectApp {
            id: id.clone(),
            selected: true,
        });
        assert!(s.ui.selected.contains(&id));
        s = reduce(s, Intent::SelectApp {
            id: id.clone(),
            selected: false,
        });
        assert!(!s.ui.selected.contains(&id));
    }

    #[test]
    fn set_target_drive() {
        let s = AppState::new();
        let s = reduce(
            s,
            Intent::SetTargetDrive {
                letter: crate::domain::value_objects::DriveLetter::raw("D:"),
            },
        );
        assert_eq!(s.ui.target_drive.unwrap().as_str(), "D:");
    }

    #[test]
    fn show_and_dismiss_toast() {
        let s = AppState::new();
        let s = reduce(
            s,
            Intent::ShowToast {
                kind: crate::application::state::ToastKind::Info,
                message: "hi".into(),
                generation: 1,
            },
        );
        assert!(s.ui.toast.is_some());
        let s = reduce(s, Intent::DismissToast { generation: 1 });
        assert!(s.ui.toast.is_none());
    }

    #[test]
    fn dismiss_toast_with_mismatched_generation_does_nothing() {
        // 模拟:旧 toast 的 DismissToast 触发,不能误清新 toast
        let s = AppState::new();
        let s = reduce(
            s,
            Intent::ShowToast {
                kind: crate::application::state::ToastKind::Info,
                message: "first".into(),
                generation: 1,
            },
        );
        // 第二个 toast 替换
        let s = reduce(
            s,
            Intent::ShowToast {
                kind: crate::application::state::ToastKind::Info,
                message: "second".into(),
                generation: 2,
            },
        );
        // 第一个 toast 的 DismissToast(generation=1)触发 — 不应该清掉第二个
        let s = reduce(s, Intent::DismissToast { generation: 1 });
        assert!(s.ui.toast.is_some());
        assert_eq!(s.ui.toast.as_ref().unwrap().message, "second");
        // 第二个 toast 的 DismissToast(generation=2)正确清掉
        let s = reduce(s, Intent::DismissToast { generation: 2 });
        assert!(s.ui.toast.is_none());
    }

    #[test]
    fn set_loading_idle() {
        let s = AppState::new();
        let s = reduce(
            s,
            Intent::SetLoading {
                kind: crate::application::state::LoadingKind::Migrating,
            },
        );
        assert_eq!(s.ui.loading, crate::application::state::LoadingKind::Migrating);
    }

    #[test]
    fn migration_phase_progress_state() {
        let mut s = AppState::new();
        let id = AppId::new();
        s.apps = vec![InstalledApp {
            id: id.clone(),
            source: crate::domain::entities::AppSource::Hklm64,
            display_name: "X".into(),
            publisher: None,
            display_version: None,
            install_location: AppPath::new("C:/X").unwrap(),
            uninstall_string: None,
            display_icon: None,
            estimated_size: Some(ByteSize::MB * 10),
            actual_size: None,
            release_type: None,
            parent_key_name: None,
        }];
        // 初始没有 migrations
        assert!(s.migrations.is_empty());
        // MigrationPhase 应创建 status
        s = reduce(
            s,
            Intent::MigrationPhase {
                id: id.clone(),
                phase: MigrationPhase::Checking,
            },
        );
        assert_eq!(s.migrations[&id].phase, MigrationPhase::Checking);
        // MigrationProgress 应更新 copied
        s = reduce(
            s,
            Intent::MigrationProgress {
                id: id.clone(),
                copied: ByteSize(1024),
                total: ByteSize(2048),
                speed_bps: 100,
            },
        );
        assert_eq!(s.migrations[&id].copied_bytes, ByteSize(1024));
        assert_eq!(s.migrations[&id].total, ByteSize(2048));
        assert_eq!(s.migrations[&id].speed_bps, 100);
    }

    #[test]
    fn migration_completed_moves_to_migrated() {
        let s = AppState::new();
        let id = AppId::new();
        let report = MigrationReport {
            app_id: id.clone(),
            source: AppPath::new("C:/X").unwrap(),
            target: AppPath::new("D:/X").unwrap(),
            backup_path: AppPath::new("C:/X_b").unwrap(),
            total_size: ByteSize(2048),
            duration_ms: 100,
            started_at: chrono::Utc::now(),
            finished_at: chrono::Utc::now(),
        };
        let s = reduce(
            s,
            Intent::MigrationCompleted {
                id: id.clone(),
                report: report.clone(),
            },
        );
        assert!(s.migrated.contains_key(&id));
        assert_eq!(s.migrated[&id].total_size, ByteSize(2048));
        assert_eq!(s.migrations[&id].phase, MigrationPhase::Completed);
    }

    #[test]
    fn migrated_loaded_inserts_unique_by_id() {
        let s = AppState::new();
        let id = AppId::new();
        let r1 = MigrationReport {
            app_id: id.clone(),
            source: AppPath::new("C:/A").unwrap(),
            target: AppPath::new("D:/A").unwrap(),
            backup_path: AppPath::new("C:/A_b").unwrap(),
            total_size: ByteSize(1),
            duration_ms: 0,
            started_at: chrono::Utc::now(),
            finished_at: chrono::Utc::now(),
        };
        let s = reduce(s, Intent::MigratedLoaded(vec![r1.clone()]));
        assert_eq!(s.migrated.len(), 1);
        let s2 = reduce(s, Intent::MigratedLoaded(vec![r1]));
        assert_eq!(s2.migrated.len(), 1);
    }

    #[test]
    fn migration_failed_sets_error() {
        let mut s = AppState::new();
        let id = AppId::new();
        s.apps = vec![InstalledApp {
            id: id.clone(),
            source: crate::domain::entities::AppSource::Hklm64,
            display_name: "X".into(),
            publisher: None,
            display_version: None,
            install_location: AppPath::new("C:/X").unwrap(),
            uninstall_string: None,
            display_icon: None,
            estimated_size: Some(ByteSize::MB * 10),
            actual_size: None,
            release_type: None,
            parent_key_name: None,
        }];
        s = reduce(
            s,
            Intent::MigrationFailed {
                id: id.clone(),
                error: "boom".into(),
            },
        );
        assert_eq!(s.migrations[&id].error.as_deref(), Some("boom"));
        assert_eq!(s.migrations[&id].phase, MigrationPhase::Failed);
    }

    #[test]
    fn drives_loaded_auto_picks_first_non_system() {
        use crate::domain::entities::DriveInfo;
        let s = AppState::new();
        let drives = vec![
            DriveInfo {
                letter: crate::domain::value_objects::DriveLetter::raw("C"),
                mount_point: "C:\\".into(),
                label: None,
                file_system: None,
                total: ByteSize(100),
                available: ByteSize(50),
                is_system: true,
            },
            DriveInfo {
                letter: crate::domain::value_objects::DriveLetter::raw("D"),
                mount_point: "D:\\".into(),
                label: None,
                file_system: None,
                total: ByteSize(100),
                available: ByteSize(80),
                is_system: false,
            },
        ];
        let s = reduce(s, Intent::DrivesLoaded(drives));
        assert_eq!(s.ui.target_drive.unwrap().as_str(), "D");
    }
}
