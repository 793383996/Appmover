//! AppMover 桌面应用主入口。
//!
//! Windows 上需要管理员权限(创建 junction + 写 `C:\Program Files`)。
//! 提权由 `tauri.conf.json` 的 manifest 处理。

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

fn main() {
    appmover_lib::run();
}
