//! 日志初始化。
//!
//! - 写 `tracing` 结构化日志到 stderr(开发期)
//! - 生产期可追加 `tracing-appender` 写文件(此处用占位)
//! - 用 `RUST_LOG` 环境变量控制级别,默认 `info`

use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: OnceLock<()> = OnceLock::new();

/// 初始化全局 subscriber,只能调一次。
pub fn init() {
    INIT.get_or_init(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,appmover=debug"));

        let layer = fmt::layer()
            .with_target(true)
            .with_level(true)
            .with_thread_ids(false)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .init();
    });
}
