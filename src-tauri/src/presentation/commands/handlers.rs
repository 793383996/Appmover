//! Tauri commands —— 表现层。
//!
//! 单一入口 `dispatch_intent(Intent)`,前端把 Intent 序列化后发来,后端投递到 AppStore。
//! 配合 `get_state()` 启动时拉取初始 state。

use crate::application::di::AppDeps;
use crate::application::effect::AppStore;
use crate::application::intent::Intent;
use crate::application::state::AppState;
use crate::shared::{AppError, AppResult};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// 全局 store handle。`tauri::Builder::manage()` 注册。
pub struct StoreHandle(pub Arc<AppStore>);

/// **Round 10 边界**:搜索框 query 长度上限,防止恶意/手滑输入 1GB 字符串
/// 触发 reducer 内 `state.ui.search = query` 内存复制 / IPC 反序列化 DoS。
/// 256 字符覆盖中文 / 英文 / 路径搜索,远超人类使用需要;与前端 `<NInput maxlength>`
/// 保持一致(后端兜底,前端是 UX 提示)。
pub const MAX_SEARCH_QUERY_LEN: usize = 256;

/// **Round 10 边界**:toast message 长度上限,防止 effect 层误把后端长堆栈
/// (e.g. `format!("io error at {path}: {source}")`)整段塞进 toast。1024 字符
/// 已足够表达人类可读消息,超出应记日志(后端 effect 已 warn)而非显示。
pub const MAX_TOAST_MESSAGE_LEN: usize = 1024;

/// 单 Intent 派发。
///
/// **Round 6 验证**:`SetTargetDrive` 必须落到 `state.drives` 真实存在的盘
/// 字母(忽略大小写),否则忽略并记录 warn。前端可能发来老盘符或过期状态,
/// 此处是最后防线(redundant with 前端 disabled=is_system 选项)。
///
/// **Round 10 边界**:文本字段长度验证(`SetSearch.query` / `ShowToast.message`),
/// 防止恶意/手滑输入触发的内存放大 / IPC DoS。
#[tauri::command]
pub async fn dispatch_intent(
    app: AppHandle,
    store: State<'_, StoreHandle>,
    intent: Intent,
) -> AppResult<()> {
    // **Round 6 验证**:SetTargetDrive 必须是已知盘(已在 drives 列表里)
    if let Intent::SetTargetDrive { ref letter } = intent {
        let known: Vec<String> = store
            .0
            .state
            .read()
            .drives
            .iter()
            .map(|d| d.letter.as_str().to_string())
            .collect();
        if !known.is_empty()
            && !known
                .iter()
                .any(|l| l.eq_ignore_ascii_case(letter.as_str()))
        {
            tracing::warn!(
                target: "appmover",
                "SetTargetDrive rejected: letter={} not in drives={:?}",
                letter.as_str(),
                known
            );
            return Err(AppError::UseCase(format!(
                "目标盘 {} 不在已加载磁盘列表中",
                letter.as_str()
            )));
        }
    }
    // **Round 10 验证**:SetSearch.query 长度上限
    if let Intent::SetSearch { ref query } = intent {
        if query.chars().count() > MAX_SEARCH_QUERY_LEN {
            tracing::warn!(
                target: "appmover",
                "SetSearch rejected: query len={} chars exceeds limit={}",
                query.chars().count(),
                MAX_SEARCH_QUERY_LEN
            );
            return Err(AppError::UseCase(format!(
                "搜索词过长 ({} 字符,上限 {})",
                query.chars().count(),
                MAX_SEARCH_QUERY_LEN
            )));
        }
    }
    // **Round 10 验证**:ShowToast.message 长度上限(防止 effect 把长堆栈塞进 toast)
    if let Intent::ShowToast { ref message, .. } = intent {
        if message.chars().count() > MAX_TOAST_MESSAGE_LEN {
            tracing::warn!(
                target: "appmover",
                "ShowToast rejected: message len={} chars exceeds limit={}",
                message.chars().count(),
                MAX_TOAST_MESSAGE_LEN
            );
            return Err(AppError::UseCase(format!(
                "toast 消息过长 ({} 字符,上限 {})",
                message.chars().count(),
                MAX_TOAST_MESSAGE_LEN
            )));
        }
    }
    store.0.dispatch(&app, intent);
    Ok(())
}

/// 拉取当前完整 state(前端启动时用)。
#[tauri::command]
pub async fn get_state(store: State<'_, StoreHandle>) -> AppResult<AppState> {
    Ok(store.0.state.read().clone())
}

/// 健康检查 / 版本号。
#[tauri::command]
pub async fn version() -> String {
    format!("AppMover {}", env!("CARGO_PKG_VERSION"))
}

/// DI 容器版本(给前端确认依赖就绪)。
#[tauri::command]
pub async fn deps_info(_deps: State<'_, Arc<AppDeps>>) -> AppResult<String> {
    Ok("deps ready".to_string())
}
