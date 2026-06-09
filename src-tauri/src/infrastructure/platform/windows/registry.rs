//! Windows 注册表读取(枚举 Uninstall 项)。
//!
//! 3 个 hive:
//! - `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
//! - `HKLM\SOFTWARE\WOW6432Node\...\Uninstall`
//! - `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`

#[cfg(windows)]
use crate::domain::entities::{AppSource, InstalledApp};
#[cfg(windows)]
use crate::domain::repositories::AppRepository;
#[cfg(windows)]
use crate::domain::value_objects::{AppId, AppPath, ByteSize};
#[cfg(windows)]
use crate::shared::AppResult;
#[cfg(windows)]
use async_trait::async_trait;
#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
pub struct RegistryAppRepository;

#[cfg(windows)]
impl RegistryAppRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[cfg(windows)]
#[async_trait]
impl AppRepository for RegistryAppRepository {
    /// **Round 3 修复**:winreg 全是同步阻塞 IO,直接放在 `async fn` 里会卡死
    /// tokio worker。包 `spawn_blocking` 把阻塞操作丢到 blocking thread pool。
    /// 这是 tokio 官方对阻塞 IO 的标准处理。
    async fn scan_all(&self) -> AppResult<Vec<InstalledApp>> {
        tokio::task::spawn_blocking(|| -> AppResult<Vec<InstalledApp>> {
            let mut all = Vec::new();
            for (hive, sub, source) in [
                (
                    HKEY_LOCAL_MACHINE,
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                    AppSource::Hklm64,
                ),
                (
                    HKEY_LOCAL_MACHINE,
                    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
                    AppSource::Hklm32,
                ),
                (
                    HKEY_CURRENT_USER,
                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                    AppSource::Hkcu,
                ),
            ] {
                if let Err(e) = scan_hive(hive, sub, source).map(|items| all.extend(items)) {
                    tracing::warn!(target: "appmover", "scan_hive {sub} failed: {e}");
                }
            }
            // **Round 3 修复**:启发式过滤 KB 补丁项
            all.retain(|app| !is_patch_entry(app));
            // 同 InstallLocation 去重,保留 display_name 最长者
            let mut dedup: HashMap<String, InstalledApp> = HashMap::new();
            for app in all {
                let key = app.install_location.to_string();
                match dedup.get(&key) {
                    Some(existing) if existing.display_name.len() >= app.display_name.len() => {}
                    _ => {
                        dedup.insert(key, app);
                    }
                }
            }
            Ok(dedup.into_values().collect())
        })
        .await
        .map_err(|e| crate::shared::AppError::UseCase(format!("scan_all join: {e}")))?
    }
}

#[cfg(windows)]
/// **Round 4 修复**:严格 KB 过滤。
/// Round 3 的 `n.starts_with("KB") && n[2..]` 会误杀真应用名("KB-Test" / "KBS" / "Keyboard Studio")。
/// 新规则(按优先级):
/// 1. `ParentKeyName` 存在 → 子补丁,过滤
/// 2. `ReleaseType` 是 "Security Update" / "Update Rollup" / "Hotfix" → 过滤
/// 3. display_name **整段**(trim 后)必须正好匹配 `KB\d+`,才视为 KB 补丁
///    ("KB12345" / "KB12345 - 一些标题" 也算补丁)
fn is_patch_entry(app: &InstalledApp) -> bool {
    // 1. ParentKeyName 存在 → 子补丁
    if app.parent_key_name.is_some() {
        return true;
    }
    // 2. ReleaseType 存在且命中补丁模式
    if let Some(rt) = &app.release_type {
        let lower = rt.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "security update" | "update rollup" | "hotfix" | "service pack"
        ) {
            return true;
        }
    }
    // 3. display_name 整段(或整段前缀)匹配 KB + 数字
    let n = app.display_name.trim().trim_start_matches('[').trim();
    // 取前两段("KB12345 - xxx" / "KB12345:xxx"),看是否 KB + 数字
    let head: String = n.chars().take_while(|c| *c != '-' && *c != ':').collect();
    let head = head.trim();
    if head.len() >= 4
        && head.starts_with("KB")
        && head[2..].chars().all(|c| c.is_ascii_digit())
    {
        return true;
    }
    false
}

#[cfg(windows)]
fn scan_hive(hive: HKEY, subkey: &str, source: AppSource) -> AppResult<Vec<InstalledApp>> {
    let root = RegKey::predef(hive);
    let Ok(uninstall) = root.open_subkey(subkey) else {
        return Ok(vec![]);
    };

    let mut out = Vec::new();
    for key_result in uninstall.enum_keys() {
        let Ok(name) = key_result else { continue };
        let Ok(sub) = uninstall.open_subkey(&name) else { continue };
        if let Ok(Some(app)) = parse_entry(&sub, source) {
            out.push(app);
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn parse_entry(key: &RegKey, source: AppSource) -> AppResult<Option<InstalledApp>> {
    // SystemComponent = 1 跳过
    if let Ok(sys_comp) = key.get_value::<u32, _>("SystemComponent") {
        if sys_comp == 1 {
            return Ok(None);
        }
    }
    // KB 更新无 DisplayName
    let display_name: String = match key.get_value("DisplayName") {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if display_name.is_empty() {
        return Ok(None);
    }
    let install_location: String = key.get_value("InstallLocation").unwrap_or_default();
    if install_location.is_empty() {
        return Ok(None);
    }
    let Ok(path) = AppPath::new(&install_location) else {
        return Ok(None);
    };

    let publisher = key.get_value("Publisher").ok();
    let version = key.get_value("DisplayVersion").ok();
    let uninstall_string = key.get_value("UninstallString").ok();
    let display_icon = key.get_value("DisplayIcon").ok();
    let release_type = key.get_value("ReleaseType").ok();
    let parent_key_name = key.get_value("ParentKeyName").ok();
    let estimated_size = key
        .get_value::<u32, _>("EstimatedSize")
        .ok()
        .map(|kb| ByteSize::new((kb as u64) * 1024));

    Ok(Some(InstalledApp {
        id: AppId::new(),
        source,
        display_name,
        publisher,
        display_version: version,
        install_location: path,
        uninstall_string,
        display_icon,
        estimated_size,
        actual_size: None,
        release_type,
        parent_key_name,
    }))
}
