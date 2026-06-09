//! `CalculateSizeUseCase` —— 计算单个应用目录的占用大小。

use crate::domain::repositories::SizeCalculator;
use crate::domain::value_objects::AppPath;
use crate::shared::AppResult;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct CalculateSizeUseCase {
    calc: Arc<dyn SizeCalculator>,
}

impl CalculateSizeUseCase {
    pub fn new(calc: Arc<dyn SizeCalculator>) -> Self {
        Self { calc }
    }

    /// 无进度版,直接返回总字节数。
    pub async fn execute(&self, path: &AppPath) -> AppResult<u64> {
        let size = self.calc.calculate(path).await?;
        Ok(size.as_bytes())
    }

    /// 带进度 + 取消支持版。
    pub async fn execute_with_progress(
        &self,
        path: &AppPath,
        tx: mpsc::Sender<crate::domain::repositories::SizeProgress>,
        cancel: Arc<CancellationToken>,
    ) -> AppResult<u64> {
        let size = self.calc.calculate_with_progress(path, tx, cancel).await?;
        Ok(size.as_bytes())
    }
}
