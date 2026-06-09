//! 路径守卫:系统关键目录 / 黑名单发布者保护。
//!
//! 业务规则写在 domain 层,具体匹配规则集中在本文件,
//! 方便单测覆盖(不依赖任何 IO)。
//!
//! 策略:
//! 1. 系统关键目录:Windows / ProgramData / Boot / Recovery / WindowsApps / Default User / Public
//! 2. 驱动/固件类发布者:NVIDIA / Intel / AMD / Realtek / VMware / Oracle(JVM)/ Broadcom
//! 3. 不再拦截"Microsoft Corporation"全家桶(避免误伤 VS Code、PowerToys 等合法应用);
//!    真正的"系统组件"已通过 SystemComponent 注册表项 + Windows 目录路径拦截。

use crate::domain::value_objects::AppPath;
use crate::shared::AppError;

pub trait PathGuard: Send + Sync {
    /// 该路径是否不可迁移(系统组件、UWP、驱动类)。
    fn is_critical(&self, path: &AppPath, publisher: Option<&str>) -> Result<(), AppError>;
}

/// 默认实现:硬编码列表。
pub struct DefaultPathGuard {
    critical_prefixes: Vec<String>,
    blocked_publishers: Vec<String>,
}

impl DefaultPathGuard {
    pub fn new() -> Self {
        Self {
            critical_prefixes: vec![
                // Windows 系统目录
                "C:/Windows".into(),
                "C:/ProgramData".into(),
                "C:/Boot".into(),
                "C:/Recovery".into(),
                // **Round 5 修复**:加上 32-bit 程序目录
                "C:/Program Files (x86)".into(),
                // UWP 应用沙箱
                "C:/Program Files/WindowsApps".into(),
                // 用户配置文件,系统管理
                "C:/Users/Default".into(),
                "C:/Users/Public".into(),
            ],
            // 只拦截"驱动/固件/虚拟化/JVM"类 — 这些是真正不该搬的
            blocked_publishers: vec![
                "NVIDIA Corporation".into(),
                "Intel".into(),
                "Intel Corporation".into(),
                "AMD".into(),
                "Advanced Micro Devices".into(),
                "Realtek".into(),
                "Realtek Semiconductor".into(),
                "VMware".into(),
                "Oracle Corporation".into(),
                "Broadcom".into(),
            ],
        }
    }
}

impl Default for DefaultPathGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PathGuard for DefaultPathGuard {
    fn is_critical(&self, path: &AppPath, publisher: Option<&str>) -> Result<(), AppError> {
        let upper = path.as_path().to_string_lossy().to_ascii_uppercase();
        for p in &self.critical_prefixes {
            if upper.starts_with(&p.to_ascii_uppercase()) {
                return Err(AppError::PathGuardViolation {
                    reason: format!("system path prefix: {p}"),
                });
            }
        }
        if let Some(pub_) = publisher {
            for b in &self.blocked_publishers {
                if pub_.to_ascii_uppercase().contains(&b.to_ascii_uppercase()) {
                    return Err(AppError::PathGuardViolation {
                        reason: format!("blocked publisher: {b}"),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> AppPath {
        AppPath::new(s).unwrap()
    }

    #[test]
    fn blocks_windows_dir() {
        let g = DefaultPathGuard::new();
        assert!(g.is_critical(&p("C:/Windows/System32"), None).is_err());
    }

    #[test]
    fn blocks_nvidia_publisher() {
        let g = DefaultPathGuard::new();
        assert!(g
            .is_critical(&p("C:/Program Files/SomeApp"), Some("NVIDIA Corporation"))
            .is_err());
    }

    #[test]
    fn allows_microsoft_publisher() {
        // VS Code / PowerToys 都是 Microsoft Corporation,应该放行
        let g = DefaultPathGuard::new();
        assert!(g
            .is_critical(&p("C:/Program Files/VS Code"), Some("Microsoft Corporation"))
            .is_ok());
    }

    #[test]
    fn allows_normal_app() {
        let g = DefaultPathGuard::new();
        assert!(g
            .is_critical(
                &p("C:/Program Files/7-Zip"),
                Some("Igor Pavlov")
            )
            .is_ok());
    }
}
