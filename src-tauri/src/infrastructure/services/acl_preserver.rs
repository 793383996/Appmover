//! ACL 保留 —— Windows 上用 `icacls /save` / `icacls /restore`,
//! 其他平台用 `std::fs::set_permissions` 兜底。

use crate::shared::AppResult;

pub struct AclPreserver;

impl Default for AclPreserver {
    fn default() -> Self {
        Self::new()
    }
}

impl AclPreserver {
    pub fn new() -> Self {
        Self
    }

    #[cfg(windows)]
    pub async fn preserve(&self, from: &std::path::Path, to: &std::path::Path) -> AppResult<()> {
        use tokio::process::Command;
        use tokio::io::AsyncWriteExt;
        // **Round 2 修复**:真的把 `from` 的 ACL 应用到 `to`。
        // 1. `icacls <from> /save <tmp>` 把 ACL 序列化到文件
        // 2. `icacls <to> /restore <tmp>` 应用到目标
        // 3. 兜底:给 Administrators / Users 完整权限,避免被锁
        let tmp = std::env::temp_dir().join(format!(
            "appmover_acl_{}_{}.acl",
            std::process::id(),
            chrono::Utc::now().timestamp_millis()
        ));
        let tmp_str = tmp.to_string_lossy().to_string();
        let save_out = Command::new("icacls")
            .arg(from)
            .arg("/save")
            .arg(&tmp_str)
            .arg("/T")
            .arg("/C")
            .output()
            .await
            .map_err(|e| crate::shared::AppError::Io {
                path: from.to_path_buf(),
                source: e,
            })?;
        if !save_out.status.success() {
            tracing::warn!(
                target: "appmover",
                "icacls save failed ({}): {}",
                String::from_utf8_lossy(&save_out.stdout),
                String::from_utf8_lossy(&save_out.stderr)
            );
        }
        let restore_out = Command::new("icacls")
            .arg(to)
            .arg("/restore")
            .arg(&tmp_str)
            .output()
            .await
            .map_err(|e| crate::shared::AppError::Io {
                path: to.to_path_buf(),
                source: e,
            })?;
        if !restore_out.status.success() {
            tracing::warn!(
                target: "appmover",
                "icacls restore failed ({}): {}",
                String::from_utf8_lossy(&restore_out.stdout),
                String::from_utf8_lossy(&restore_out.stderr)
            );
        }
        // 兜底:加基础继承 + Administrators 完整权限
        let _ = Command::new("icacls")
            .arg(to)
            .arg("/inheritance:e")
            .arg("/grant:r")
            .arg("*S-1-5-32-544:(OI)(CI)F")
            .output()
            .await;
        // 清理 tmp
        if let Ok(mut f) = tokio::fs::File::open(&tmp).await {
            let _ = f.shutdown().await;
        }
        let _ = tokio::fs::remove_file(&tmp).await;
        Ok(())
    }

    #[cfg(not(windows))]
    #[allow(clippy::permissions_set_readonly_false)]
    pub async fn preserve(&self, _from: &std::path::Path, to: &std::path::Path) -> AppResult<()> {
        // macOS / Linux 上:仅设 umask
        let meta = tokio::fs::metadata(to).await.map_err(|e| crate::shared::AppError::Io {
            path: to.to_path_buf(),
            source: e,
        })?;
        let mut perm = meta.permissions();
        perm.set_readonly(false);
        tokio::fs::set_permissions(to, perm)
            .await
            .map_err(|e| crate::shared::AppError::Io {
                path: to.to_path_buf(),
                source: e,
            })?;
        Ok(())
    }
}
