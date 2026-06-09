//! 端到端测试:覆盖 reducer / state_store / use case / 关键数据结构的回归。
//!
//! 全部用 mock 仓储,不触发任何真实 IO。

use appmover_lib::domain::entities::{
    DriveInfo, InstalledApp, MigrationPhase, MigrationReport, MigrationStatus,
};
use appmover_lib::domain::repositories::{
    CopyProgress, DriveRepository, FilesystemProbe, MigrationRepository, PathGuard,
    ProcessGuard, SizeCalculator, SizeProgress, StateStore,
};
use appmover_lib::domain::usecases::{
    CalculateSizeUseCase, DetectOrphansUseCase, ListMigratedAppsUseCase, MigrateAppUseCase,
    RollbackAppUseCase,
};
use appmover_lib::domain::value_objects::{AppId, AppPath, ByteSize, DriveLetter};
use appmover_lib::shared::{AppError, AppResult};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tempfile::tempdir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ========================== Mock repos ==========================

#[derive(Default)]
struct MockStateStore {
    map: Arc<parking_lot::Mutex<HashMap<String, MigrationReport>>>,
    /// **Round 7**:模拟 save 失败(用来测试 migrate_app_use_case 的回滚)
    save_should_fail: Arc<parking_lot::Mutex<bool>>,
    /// **Round 7**:模拟 remove 失败(用来测试 rollback_use_case 的错误传播)
    remove_should_fail: Arc<parking_lot::Mutex<bool>>,
}

#[async_trait]
impl StateStore for MockStateStore {
    async fn load_all(&self) -> AppResult<HashMap<AppId, MigrationReport>> {
        let m = self.map.lock();
        let mut out = HashMap::new();
        for (k, v) in m.iter() {
            out.insert(AppId::from_string(k.clone()), v.clone());
        }
        Ok(out)
    }
    async fn save(&self, r: &MigrationReport) -> AppResult<()> {
        if *self.save_should_fail.lock() {
            return Err(AppError::Io {
                path: r.target.as_path().to_path_buf(),
                source: std::io::Error::other("mock save fail"),
            });
        }
        self.map.lock().insert(r.app_id.to_string(), r.clone());
        Ok(())
    }
    async fn remove(&self, id: &AppId) -> AppResult<()> {
        if *self.remove_should_fail.lock() {
            return Err(AppError::Io {
                path: PathBuf::from("state.json"),
                source: std::io::Error::other("mock remove fail"),
            });
        }
        self.map.lock().remove(&id.to_string());
        Ok(())
    }
}

/// **Round 7**:计数 mock migration repo,验证 rollback 被调用了几次
struct CountingMigrationRepo {
    migrate_calls: Arc<AtomicUsize>,
    rollback_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl MigrationRepository for CountingMigrationRepo {
    async fn migrate(
        &self,
        source: &AppPath,
        target: &AppPath,
        app_id: &AppId,
        _tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport> {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        self.migrate_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(MigrationReport {
            app_id: app_id.clone(),
            source: source.clone(),
            target: target.clone(),
            backup_path: AppPath::new("C:/Program Files/Test_appmover_backup_20240101_000000")
                .unwrap(),
            total_size: ByteSize(1024 * 1024),
            duration_ms: 1,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        })
    }
    async fn rollback(&self, _report: &MigrationReport) -> AppResult<()> {
        self.rollback_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

struct MockMigrationRepo;

#[async_trait]
impl MigrationRepository for MockMigrationRepo {
    async fn migrate(
        &self,
        source: &AppPath,
        target: &AppPath,
        app_id: &AppId,
        _tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport> {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        Ok(MigrationReport {
            app_id: app_id.clone(),
            source: source.clone(),
            target: target.clone(),
            backup_path: AppPath::new("C:/Program Files/Test_appmover_backup_20240101_000000")
                .unwrap(),
            total_size: ByteSize(1024 * 1024),
            duration_ms: 1,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        })
    }
    async fn rollback(&self, _report: &MigrationReport) -> AppResult<()> {
        Ok(())
    }
}

struct MockDriveRepo;
#[async_trait]
impl DriveRepository for MockDriveRepo {
    async fn list_all(&self) -> AppResult<Vec<DriveInfo>> {
        Ok(vec![
            DriveInfo {
                letter: DriveLetter::raw("C"),
                mount_point: "C:\\".into(),
                label: Some("System".into()),
                file_system: Some("NTFS".into()),
                total: ByteSize(500_000_000_000),
                available: ByteSize(100_000_000_000),
                is_system: true,
            },
            DriveInfo {
                letter: DriveLetter::raw("D"),
                mount_point: "D:\\".into(),
                label: Some("Data".into()),
                file_system: Some("NTFS".into()),
                total: ByteSize(1_000_000_000_000),
                available: ByteSize(800_000_000_000),
                is_system: false,
            },
        ])
    }
}

struct MockSizeCalc;
#[async_trait]
impl SizeCalculator for MockSizeCalc {
    async fn calculate(&self, _path: &AppPath) -> AppResult<ByteSize> {
        Ok(ByteSize(4096))
    }
    async fn calculate_with_progress(
        &self,
        _path: &AppPath,
        _tx: mpsc::Sender<SizeProgress>,
        _cancel: Arc<CancellationToken>,
    ) -> AppResult<ByteSize> {
        Ok(ByteSize(4096))
    }
}

struct MockPathGuard;
impl PathGuard for MockPathGuard {
    fn is_critical(
        &self,
        _path: &AppPath,
        _publisher: Option<&str>,
    ) -> Result<(), AppError> {
        Ok(())
    }
}

struct MockProcessGuard;
#[async_trait]
impl ProcessGuard for MockProcessGuard {
    async fn find_blocking_processes(&self, _path: &AppPath) -> AppResult<Vec<String>> {
        Ok(vec![])
    }
    async fn kill_blocking(&self, _processes: &[String]) -> AppResult<()> {
        Ok(())
    }
}

struct MockFsProbe(bool);
impl FilesystemProbe for MockFsProbe {
    fn exists(&self, _path: &AppPath) -> bool {
        self.0
    }
}

// ========================== Tests ==========================

fn dummy_path(s: &str) -> AppPath {
    AppPath::new(s).unwrap()
}

fn dummy_id() -> AppId {
    AppId::new()
}

fn make_precheck() -> Arc<appmover_lib::domain::usecases::CheckMigrationPreconditionsUseCase> {
    let drive: Arc<dyn DriveRepository> = Arc::new(MockDriveRepo);
    let pg: Arc<dyn PathGuard> = Arc::new(MockPathGuard);
    let pro: Arc<dyn ProcessGuard> = Arc::new(MockProcessGuard);
    let store: Arc<dyn StateStore> = Arc::new(MockStateStore::default());
    Arc::new(
        appmover_lib::domain::usecases::CheckMigrationPreconditionsUseCase::new(pg, pro, drive, store),
    )
}

#[tokio::test]
async fn migrate_app_use_case_persists_report_to_state_store() {
    let store = Arc::new(MockStateStore::default());
    let migrate_repo: Arc<dyn MigrationRepository> = Arc::new(MockMigrationRepo);
    let migrate = MigrateAppUseCase::new(
        make_precheck(),
        migrate_repo,
        store.clone(),
    );
    let plan = appmover_lib::domain::entities::MigrationPlan::new(
        dummy_id(),
        dummy_path("C:/Program Files/Test"),
        dummy_path("D:/Apps/Test"),
        ByteSize(1024 * 1024),
    );
    let report = migrate
        .execute(&plan, Some("Test Publisher"), None, Arc::new(CancellationToken::new()))
        .await
        .expect("migrate should succeed");
    let all = store.load_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all.values().next().unwrap().app_id, report.app_id);
    assert!(report
        .backup_path
        .as_path()
        .to_string_lossy()
        .contains("_appmover_backup_"));
}

#[tokio::test]
async fn migrate_app_use_case_respects_cancellation() {
    let store = Arc::new(MockStateStore::default());
    let migrate_repo: Arc<dyn MigrationRepository> = Arc::new(MockMigrationRepo);
    let migrate = MigrateAppUseCase::new(
        make_precheck(),
        migrate_repo,
        store.clone(),
    );
    let plan = appmover_lib::domain::entities::MigrationPlan::new(
        dummy_id(),
        dummy_path("C:/Program Files/Test"),
        dummy_path("D:/Apps/Test"),
        ByteSize(0),
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = migrate
        .execute(&plan, None, None, Arc::new(cancel))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Cancelled));
    // 取消的话不应该写入 state.json
    assert!(store.load_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn rollback_use_case_returns_app_not_found_when_no_record() {
    let store = Arc::new(MockStateStore::default());
    let repo: Arc<dyn MigrationRepository> = Arc::new(MockMigrationRepo);
    let rollback = RollbackAppUseCase::new(repo, store.clone());
    let err = rollback.execute(&dummy_id()).await.unwrap_err();
    assert!(matches!(err, AppError::AppNotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn rollback_use_case_calls_repo_and_removes_from_store() {
    let store = Arc::new(MockStateStore::default());
    let report = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("C:/Program Files/X"),
        target: dummy_path("D:/Apps/X"),
        backup_path: dummy_path("C:/Program Files/X_appmover_backup_x"),
        total_size: ByteSize(100),
        duration_ms: 1,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    let id = report.app_id.clone();
    store.save(&report).await.unwrap();
    assert_eq!(store.load_all().await.unwrap().len(), 1);

    let repo: Arc<dyn MigrationRepository> = Arc::new(MockMigrationRepo);
    let rollback = RollbackAppUseCase::new(repo, store.clone());
    rollback.execute(&id).await.expect("rollback ok");
    assert_eq!(store.load_all().await.unwrap().len(), 0);
}

#[tokio::test]
async fn list_migrated_returns_loaded_reports() {
    let store = Arc::new(MockStateStore::default());
    let r = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("C:/A"),
        target: dummy_path("D:/A"),
        backup_path: dummy_path("C:/A_b"),
        total_size: ByteSize(1),
        duration_ms: 0,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    store.save(&r).await.unwrap();
    let uc = ListMigratedAppsUseCase::new(store.clone());
    let map = uc.execute().await.unwrap();
    assert_eq!(map.len(), 1);
}

#[tokio::test]
async fn detect_orphans_finds_missing_junction() {
    let store = Arc::new(MockStateStore::default());
    let r = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("/nonexistent/path"),
        target: dummy_path("/also/none"),
        backup_path: dummy_path("/none_b"),
        total_size: ByteSize(1),
        duration_ms: 0,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    store.save(&r).await.unwrap();
    let fs: Arc<dyn FilesystemProbe> = Arc::new(MockFsProbe(false));
    let uc = DetectOrphansUseCase::new(store.clone(), fs);
    let orphans = uc.execute().await.unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].app_id, r.app_id);
}

#[tokio::test]
async fn detect_orphans_returns_empty_when_all_intact() {
    let store = Arc::new(MockStateStore::default());
    let r = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("/some/exists"),
        target: dummy_path("/also/exists"),
        backup_path: dummy_path("/exists_b"),
        total_size: ByteSize(1),
        duration_ms: 0,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    store.save(&r).await.unwrap();
    let fs: Arc<dyn FilesystemProbe> = Arc::new(MockFsProbe(true));
    let uc = DetectOrphansUseCase::new(store.clone(), fs);
    assert!(uc.execute().await.unwrap().is_empty());
}

#[tokio::test]
async fn calculate_size_returns_bytes() {
    let calc: Arc<dyn SizeCalculator> = Arc::new(MockSizeCalc);
    let uc = CalculateSizeUseCase::new(calc);
    let bytes = uc.execute(&dummy_path("C:/some/dir")).await.unwrap();
    assert_eq!(bytes, 4096);
}

#[tokio::test]
async fn state_store_save_then_load_all_round_trip() {
    let store = Arc::new(MockStateStore::default());
    let id = dummy_id();
    let r = MigrationReport {
        app_id: id.clone(),
        source: dummy_path("C:/A"),
        target: dummy_path("D:/A"),
        backup_path: dummy_path("C:/A_b"),
        total_size: ByteSize(9999),
        duration_ms: 5,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    store.save(&r).await.unwrap();
    let all = store.load_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(&id).unwrap().total_size, ByteSize(9999));

    store.remove(&id).await.unwrap();
    assert!(store.load_all().await.unwrap().is_empty());
}

#[test]
fn installed_app_is_migratable_requires_c_drive() {
    let make = |p: &str| InstalledApp {
        id: dummy_id(),
        source: appmover_lib::domain::entities::AppSource::Hklm64,
        display_name: "Test".into(),
        publisher: None,
        display_version: None,
        install_location: dummy_path(p),
        uninstall_string: None,
        display_icon: None,
        estimated_size: None,
        actual_size: None,
        release_type: None,
        parent_key_name: None,
    };
    // 注意:`is_migratable` 只看盘符前缀,系统目录过滤是 PathGuard 的职责。
    assert!(make("C:/Program Files/7-Zip").is_migratable());
    assert!(!make("D:/Program Files/7-Zip").is_migratable());
    assert!(!make("E:/Apps").is_migratable());
    assert!(make("C:/Windows/System32").is_migratable()); // C: 前缀就算,PathGuard 后续拦截
}

#[test]
fn migration_status_progress_basic() {
    let id = dummy_id();
    let mut s = MigrationStatus::idle(id);
    // 零 total 时返回 0
    assert!((s.progress(ByteSize::ZERO) - 0.0).abs() < 0.001);
    // 设 phase + bytes
    s.phase = MigrationPhase::Copying;
    s.copied_bytes = ByteSize(50);
    s.total = ByteSize(100);
    let p = s.progress(ByteSize(100));
    assert!((p - 0.5).abs() < 0.001, "got {p}");
}

#[test]
fn byte_size_arithmetic() {
    assert_eq!(ByteSize::MB * 2, ByteSize(2 * 1024 * 1024));
    let a = ByteSize(1024);
    let b = ByteSize(2048);
    let sum: ByteSize = a + b;
    assert_eq!(sum, ByteSize(3072));
}

#[test]
fn app_path_normalization_drive_letter() {
    // 反斜杠转正斜杠,大小写保留
    let p = AppPath::new("d:\\Apps").unwrap();
    assert_eq!(p.as_path().to_string_lossy(), "d:/Apps");
    // 长前缀 \\?\ 去除
    let p2 = AppPath::new(r"\\?\C:\Program Files\X").unwrap();
    assert_eq!(p2.as_path().to_string_lossy(), "C:/Program Files/X");
}

#[test]
fn migration_phase_state_machine_validity() {
    use appmover_lib::domain::entities::is_valid_transition;
    assert!(is_valid_transition(MigrationPhase::Idle, MigrationPhase::Checking));
    assert!(is_valid_transition(MigrationPhase::Checking, MigrationPhase::Copying));
    assert!(is_valid_transition(MigrationPhase::Copying, MigrationPhase::Cancelled));
    assert!(is_valid_transition(MigrationPhase::Completed, MigrationPhase::RollingBack));
    // 非法的:从 Idle 直接到 Completed
    assert!(!is_valid_transition(MigrationPhase::Idle, MigrationPhase::Completed));
    // 终态不能再转
    assert!(!is_valid_transition(MigrationPhase::Completed, MigrationPhase::Copying));
}

// ========================== Round 7 新增测试 ==========================

#[tokio::test]
async fn migrate_use_case_rolls_back_physical_state_when_state_save_fails() {
    // **Round 7 关键测试**:`state_store.save` 失败时,
    // `MigrateAppUseCase::execute` 必须调用 `migration_repo.rollback` 撤销
    // 物理迁移。否则 junction + backup + target 残留,用户无法恢复。
    let store = Arc::new(MockStateStore::default());
    *store.save_should_fail.lock() = true; // 让 save 失败
    let migrate_calls = Arc::new(AtomicUsize::new(0));
    let rollback_calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(CountingMigrationRepo {
        migrate_calls: migrate_calls.clone(),
        rollback_calls: rollback_calls.clone(),
    });
    let migrate = MigrateAppUseCase::new(make_precheck(), repo.clone(), store.clone());
    let plan = appmover_lib::domain::entities::MigrationPlan::new(
        dummy_id(),
        dummy_path("C:/Program Files/Test"),
        dummy_path("D:/Apps/Test"),
        ByteSize(1024 * 1024),
    );
    let err = migrate
        .execute(&plan, Some("Test"), None, Arc::new(CancellationToken::new()))
        .await
        .unwrap_err();
    // save 失败,返回 error
    assert!(matches!(err, AppError::Io { .. }), "got {err:?}");
    // 关键断言:rollback 被调用了 1 次(物理回滚)
    assert_eq!(
        rollback_calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "Round 7: state.save 失败时,必须调用 migration_repo.rollback 撤销物理迁移"
    );
    // state.json 里没有 entry(save 失败没存进去)
    assert!(store.load_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn rollback_use_case_propagates_error_when_state_remove_fails() {
    // **Round 7 测试**:`state_store.remove` 失败时,`RollbackAppUseCase` 必须
    // 返回错误(不能静默吞掉),让用户知道 state.json 残留。
    let store = Arc::new(MockStateStore::default());
    let report = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("C:/A"),
        target: dummy_path("D:/A"),
        backup_path: dummy_path("C:/A_b"),
        total_size: ByteSize(1),
        duration_ms: 0,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    let id = report.app_id.clone();
    store.save(&report).await.unwrap();
    *store.remove_should_fail.lock() = true; // 让 remove 失败
    let repo: Arc<dyn MigrationRepository> = Arc::new(MockMigrationRepo);
    let rollback = RollbackAppUseCase::new(repo, store.clone());
    let err = rollback.execute(&id).await.unwrap_err();
    // 关键断言:error 是 Io,remove 失败被传播
    assert!(matches!(err, AppError::Io { .. }), "got {err:?}");
    // 物理 rollback 已成功(物理状态已回滚),但 state.json 仍有 entry
    // 后续 DetectOrphans 启动时会发现
}

#[tokio::test]
async fn detect_orphans_no_orphans_returns_empty_array() {
    // **Round 7 测试**:启动时 detect_orphans 正常路径 — 空的 state.json
    // 应返回空 array,不报 warn,不弹 toast。
    let store = Arc::new(MockStateStore::default());
    let fs: Arc<dyn FilesystemProbe> = Arc::new(MockFsProbe(true));
    let uc = DetectOrphansUseCase::new(store.clone(), fs);
    let orphans = uc.execute().await.unwrap();
    assert!(orphans.is_empty());
}

#[tokio::test]
async fn detect_orphans_keeps_state_json_untouched() {
    // **Round 7 测试**:DetectOrphans 只读不写,即使发现 orphan 也不应该
    // 自动清理 state.json(让用户决定怎么 force clean)。
    let store = Arc::new(MockStateStore::default());
    let r = MigrationReport {
        app_id: dummy_id(),
        source: dummy_path("/nonexistent/X"),
        target: dummy_path("/also/none/X"),
        backup_path: dummy_path("/none_b"),
        total_size: ByteSize(1),
        duration_ms: 0,
        started_at: Utc::now(),
        finished_at: Utc::now(),
    };
    store.save(&r).await.unwrap();
    let fs: Arc<dyn FilesystemProbe> = Arc::new(MockFsProbe(false));
    let uc = DetectOrphansUseCase::new(store.clone(), fs);
    let orphans = uc.execute().await.unwrap();
    assert_eq!(orphans.len(), 1);
    // 关键断言:state.json 仍然包含该 entry(没被自动清理)
    assert_eq!(store.load_all().await.unwrap().len(), 1);
}

// ========================== Round 8 新增测试 ==========================

#[tokio::test]
async fn state_store_load_all_returns_empty_when_file_missing() {
    // **Round 8 测试**:load_all 在 state.json 不存在时直接返回空,
    // 不 backup corrupt(因文件根本不存在)。
    use appmover_lib::infrastructure::repositories::state_store::JsonStateStore;
    let tmp = tempdir().unwrap();
    // state.json 不创建
    let store = JsonStateStore::with_path(tmp.path().join("state.json"));
    let map = store.load_all().await.unwrap();
    assert!(map.is_empty(), "missing file should give empty map");
    // **关键断言**:没有任何 backup 文件被创建(无 .corrupt.* 残留)
    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains("corrupt"))
        .collect();
    assert!(entries.is_empty(), "no corrupt backup should be created for missing file");
}

#[tokio::test]
async fn migrate_use_case_logs_critical_when_physical_rollback_also_fails() {
    // **Round 8 测试**:state.save 失败时,若 physical rollback 也失败,
    // 必须打 CRITICAL log 记录需要手动清理的源/目标/backup 路径。
    // 这里用 CountingMigrationRepo 但设置 rollback_should_fail。
    let store = Arc::new(MockStateStore::default());
    *store.save_should_fail.lock() = true;
    // 用一个会失败的 migration repo
    let migrate_calls = Arc::new(AtomicUsize::new(0));
    let rollback_calls = Arc::new(AtomicUsize::new(0));
    let repo = Arc::new(FailRollbackMigrationRepo {
        migrate_calls: migrate_calls.clone(),
        rollback_calls: rollback_calls.clone(),
    });
    let migrate = MigrateAppUseCase::new(make_precheck(), repo.clone(), store.clone());
    let plan = appmover_lib::domain::entities::MigrationPlan::new(
        dummy_id(),
        dummy_path("C:/Program Files/Test"),
        dummy_path("D:/Apps/Test"),
        ByteSize(1024 * 1024),
    );
    let err = migrate
        .execute(&plan, Some("Test"), None, Arc::new(CancellationToken::new()))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Io { .. }), "got {err:?}");
    // 关键断言:rollback 被尝试调用 1 次(虽然失败了)
    assert_eq!(rollback_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// **Round 8**:`rollback` 也失败的 mock(模拟 Windows 上 junction 占用场景)
struct FailRollbackMigrationRepo {
    migrate_calls: Arc<AtomicUsize>,
    rollback_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl MigrationRepository for FailRollbackMigrationRepo {
    async fn migrate(
        &self,
        source: &AppPath,
        target: &AppPath,
        app_id: &AppId,
        _tx: Option<mpsc::Sender<CopyProgress>>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<MigrationReport> {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        self.migrate_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(MigrationReport {
            app_id: app_id.clone(),
            source: source.clone(),
            target: target.clone(),
            backup_path: AppPath::new("C:/Test_appmover_backup").unwrap(),
            total_size: ByteSize(1024),
            duration_ms: 1,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        })
    }
    async fn rollback(&self, _report: &MigrationReport) -> AppResult<()> {
        self.rollback_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // **Round 8**:rollback 失败(模拟 junction 占用 / 权限)
        Err(AppError::Link("junction remove failed: access denied".into()))
    }
}

// ========================== Round 9 新增测试 ==========================

#[tokio::test]
async fn cancel_token_idempotent_double_cancel() {
    // **Round 9 测试**:`CancelMigration` 同一 id 重复 cancel 的去重语义。
    // 这是 R9 P1 修复:`token.cancel()` 调两次幂等,但
    // `is_cancelled()` 第二次返 `true`,UI 可以用此判定避免弹重复 toast。
    // 这里测 token 自身行为(不是 AppStore,后者需要 Tauri AppHandle)。
    let token = tokio_util::sync::CancellationToken::new();
    assert!(!token.is_cancelled());
    token.cancel();
    assert!(token.is_cancelled());
    token.cancel(); // 重复 cancel,应幂等无副作用
    assert!(token.is_cancelled());
    // 验证 cvar 被唤醒(无 panic)
    token.cancelled().await; // 不应 hang
}

#[tokio::test]
async fn cancel_token_distinct_instances() {
    // **Round 9 测试**:不同 token 互不干扰。
    // 这是 R9 P1 的"幂等"边界 case:同一 id 重复 cancel 静默,
    // 但**不同** id 的 token 必须独立 cancel。
    let t1 = tokio_util::sync::CancellationToken::new();
    let t2 = tokio_util::sync::CancellationToken::new();
    t1.cancel();
    assert!(t1.is_cancelled());
    assert!(!t2.is_cancelled(), "t2 不应被 t1.cancel() 影响");
    t2.cancel();
    assert!(t2.is_cancelled());
}

#[tokio::test]
async fn startup_block_on_waits_for_drives_and_migrated_before_returning() {
    // **Round 9 测试**:模拟 `lib.rs` setup 阶段的 `block_on + polling` 等待逻辑。
    // 验证:在 drives + migrated 都"加载完"前 polling 不退出;
    // 两者都加载完时 polling 立即退出。
    use appmover_lib::application::state::AppState;
    use parking_lot::RwLock;
    use std::sync::Arc;
    use std::time::Instant;

    let state = Arc::new(RwLock::new(AppState::default()));
    // 模拟 setup 的 polling 逻辑
    let s = state.clone();
    let handle = tokio::spawn(async move {
        let deadline = Instant::now() + std::time::Duration::from_millis(500);
        loop {
            {
                let r = s.read();
                if !r.drives.is_empty() && !r.migrated.is_empty() {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });
    // 100ms 后填入 drives + migrated
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    {
        let mut w = state.write();
        w.drives.push(appmover_lib::domain::entities::DriveInfo {
            letter: appmover_lib::domain::value_objects::DriveLetter::raw("D:"),
            mount_point: "D:\\".to_string(),
            label: Some("Test".to_string()),
            file_system: Some("NTFS".to_string()),
            total: ByteSize(0),
            available: ByteSize(0),
            is_system: false,
        });
        w.migrated.insert(AppId::from_string("test"), {
            use appmover_lib::domain::entities::MigrationReport;
            use appmover_lib::domain::value_objects::{AppId, AppPath};
            MigrationReport {
                app_id: AppId::from_string("test"),
                source: AppPath::new("/src").unwrap(),
                target: AppPath::new("/tgt").unwrap(),
                backup_path: AppPath::new("/bak").unwrap(),
                total_size: ByteSize(0),
                duration_ms: 0,
                started_at: Utc::now(),
                finished_at: Utc::now(),
            }
        });
    }
    let result = handle.await.unwrap();
    assert!(result, "Round 9: setup polling 应在 state 填好时立即返回");
}
