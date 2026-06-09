//! Mock 实现 —— 在非 Windows 平台(macOS / Linux)上 `cargo check` / 跑单测时使用。
//!
//! 行为:
//! - 注册表扫描:返回空
//! - Junction:返回 `unimplemented` 风格错误
//! - 进程占用:返回空

use crate::domain::entities::InstalledApp;
use crate::domain::repositories::AppRepository;
use crate::shared::AppResult;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MockAppRepository;

impl MockAppRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl AppRepository for MockAppRepository {
    async fn scan_all(&self) -> AppResult<Vec<InstalledApp>> {
        Ok(vec![])
    }
}
