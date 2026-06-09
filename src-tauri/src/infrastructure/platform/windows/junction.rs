//! Windows Directory Junction / Symbolic Link 实现。
//!
//! 实际调用 `CreateSymbolicLinkW` Win32 API,加 `SYMBOLIC_LINK_FLAG_DIRECTORY` 实现 junction。
//! 删除用普通 `fs::remove_dir`(junction 是目录 reparse point)。

#[cfg(windows)]
use crate::domain::value_objects::AppPath;
#[cfg(windows)]
use crate::shared::{AppError, AppResult};

#[cfg(windows)]
pub fn create_junction(link: &AppPath, target: &AppPath) -> AppResult<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        CreateSymbolicLinkW, SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let link_w = to_wide(link.as_path());
    let target_w = to_wide(target.as_path());

    unsafe {
        CreateSymbolicLinkW(
            PCWSTR(link_w.as_ptr()),
            PCWSTR(target_w.as_ptr()),
            SYMBOLIC_LINK_FLAG_DIRECTORY,
        )
        .map_err(AppError::from)?;
    }
    Ok(())
}

#[cfg(windows)]
fn to_wide(p: &std::path::Path) -> Vec<u16> {
    p.to_string_lossy().encode_utf16().chain([0]).collect()
}
