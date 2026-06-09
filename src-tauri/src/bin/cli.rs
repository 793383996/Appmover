//! AppMover 命令行工具(轻量版,供脚本化场景使用)。
//!
//! 子命令:
//! - `list`:列出已安装应用(简表)
//! - `migrate <app_id> <target>`:迁移单个
//! - `rollback <app_id>`:回滚
//! - `status`:显示已迁移清单
//!
//! Windows 上需要管理员;macOS / Linux 提示"Windows only"。

fn main() {
    println!("AppMover CLI — Windows only. Run on Windows for full functionality.");
    appmover_lib::shared::logger::init();
}
