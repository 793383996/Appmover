//! `ListMigratedAppsUseCase` —— 列出已迁移应用(state.json 里的)。

use crate::domain::entities::MigrationReport;
use crate::domain::repositories::StateStore;
use crate::shared::AppResult;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ListMigratedAppsUseCase {
    state_store: Arc<dyn StateStore>,
}

impl ListMigratedAppsUseCase {
    pub fn new(state_store: Arc<dyn StateStore>) -> Self {
        Self { state_store }
    }

    pub async fn execute(&self) -> AppResult<HashMap<String, MigrationReport>> {
        let map = self.state_store.load_all().await?;
        Ok(map
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect())
    }
}
