//! Mock 进程占用检测 —— 始终返回空(便于单测)。

use crate::domain::repositories::ProcessGuard;
use crate::domain::value_objects::AppPath;
use crate::shared::AppResult;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MockProcessGuard;

impl MockProcessGuard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl ProcessGuard for MockProcessGuard {
    async fn find_blocking_processes(&self, _path: &AppPath) -> AppResult<Vec<String>> {
        Ok(vec![])
    }

    async fn kill_blocking(&self, _processes: &[String]) -> AppResult<()> {
        Ok(())
    }
}
