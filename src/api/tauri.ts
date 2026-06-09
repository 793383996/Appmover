/**
 * Tauri 桥 —— 封装 invoke / listen。
 *
 * 单一约定:所有前端 → 后端的命令通过 `dispatch(intent)` 走,后端按 Intent 分发。
 * 状态同步:启动时 `getState()` 拉取一次,之后订阅 `appmover://state-changed` 事件。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppState, CopyProgress, Intent } from "@/types";

export const EVT_STATE_CHANGED = "appmover://state-changed";
export const EVT_MIGRATION_PROGRESS = "appmover://migration-progress";
export const EVT_MIGRATION_COMPLETED = "appmover://migration-completed";
export const EVT_LOG = "appmover://log";

/** 派发 Intent 到后端。 */
export async function dispatch(intent: Intent): Promise<void> {
  await invoke("dispatch_intent", { intent });
}

/** 拉取完整 state(启动时调用)。 */
export async function getState(): Promise<AppState> {
  return await invoke<AppState>("get_state");
}

/** 订阅状态变更。 */
export async function onStateChanged(
  cb: (state: AppState) => void,
): Promise<UnlistenFn> {
  return await listen<AppState>(EVT_STATE_CHANGED, (e) => cb(e.payload));
}

/** 订阅复制进度。 */
export async function onMigrationProgress(
  cb: (p: CopyProgress) => void,
): Promise<UnlistenFn> {
  return await listen<CopyProgress>(EVT_MIGRATION_PROGRESS, (e) =>
    cb(e.payload),
  );
}

/** 订阅迁移完成事件。 */
export async function onMigrationCompleted(
  cb: (id: string) => void,
): Promise<UnlistenFn> {
  return await listen<string>(EVT_MIGRATION_COMPLETED, (e) => cb(e.payload));
}

/** 版本号。 */
export async function version(): Promise<string> {
  return await invoke<string>("version");
}
