/**
 * 前端类型 —— 与 Rust 端 serde 类型一一对应。
 *
 * Rust serde 序列化规则:
 * - `#[serde(rename_all = "snake_case")]` → 字段名小写下划线
 * - `#[serde(tag = "type", rename_all = "snake_case")]` enum → 多态 discriminated union
 *
 * 因此 Intent 是 `{ type: "scan_apps" }` / `{ type: "set_target_drive", letter: "D:" }` 形式。
 */

// ============ Domain ============

export type AppSource = "hklm64" | "hklm32" | "hkcu";

export interface InstalledApp {
  id: string;
  source: AppSource;
  display_name: string;
  publisher: string | null;
  display_version: string | null;
  install_location: string;
  uninstall_string: string | null;
  display_icon: string | null;
  estimated_size: number | null; // bytes
  actual_size: number | null;
  /**
   * **Round 11 修复**:与 Rust 端 `InstalledApp` 字段对齐 —— release_type /
   * parent_key_name 是 R3/R5 引入用于 KB filter 的字段(R5 lessons:
   * "KB filtering must prioritize ParentKeyName/ReleaseType fields")。
   * 之前 TS 类型缺失,后端序列化的字段会被前端静默丢弃,KB filter 在
   * 前端无法直接用(只能通过 list_migrated 后端筛选)。
   */
  release_type: string | null;
  parent_key_name: string | null;
}

export interface DriveInfo {
  letter: string;
  mount_point: string;
  label: string | null;
  file_system: string | null;
  total: number; // bytes
  available: number;
  is_system: boolean;
}

export type MigrationPhase =
  | "idle"
  | "checking"
  | "copying"
  | "linking"
  | "verifying"
  | "completed"
  | "failed"
  | "rolling_back"
  | "rolled_back"
  | "rollback_failed"
  | "cancelled";

export interface MigrationStatus {
  app_id: string;
  phase: MigrationPhase;
  copied_bytes: number;
  total: number;
  speed_bps: number;
  error: string | null;
  started_at: string | null;
  finished_at: string | null;
}

export interface MigrationReport {
  app_id: string;
  source: string;
  target: string;
  total_size: number;
  duration_ms: number;
  started_at: string;
  finished_at: string;
}

// ============ UI State ============

export type FilterMode = "all" | "large_only" | "migratable";
export type SortMode = "name" | "size" | "publisher";
export type LoadingKind = "idle" | "scanning" | "loading_drives" | "calculating_size" | "migrating";
export type ToastKind = "info" | "success" | "warning" | "error";

export interface Toast {
  kind: ToastKind;
  message: string;
  /**
   * **Round 11 修复**:与 Rust 端 Toast 结构对齐,补全 generation 字段。
   * 后端 `state.json` 序列化时 Toast 包含 3 字段(kind / message / generation),
   * generation 单调递增用于 DismissToast 精准只 dismiss 自己触发的那次。
   * 之前 TS 类型只有 2 字段,虽然运行时不影响(TS 类型会被编译期擦除),
   * 但 IDE 提示会缺失该字段,后续前端若读 `toast.generation` 会编译失败。
   */
  generation: number;
}

export interface UiState {
  selected: string[];
  target_drive: string | null;
  search: string;
  filter: FilterMode;
  sort: SortMode;
  loading: LoadingKind;
  toast: Toast | null;
}

export interface AppState {
  apps: InstalledApp[];
  drives: DriveInfo[];
  migrations: Record<string, MigrationStatus>;
  migrated: Record<string, MigrationReport>;
  ui: UiState;
}

// ============ Intent (前端 → 后端) ============

export type Intent =
  // 扫描
  | { type: "scan_apps" }
  | { type: "list_drives" }
  | { type: "list_migrated" }
  // 选择 / 过滤
  | { type: "select_app"; id: string; selected: boolean }
  | { type: "select_all" }
  | { type: "clear_selection" }
  | { type: "set_target_drive"; letter: string }
  | { type: "set_search"; query: string }
  | { type: "set_filter"; mode: FilterMode }
  | { type: "set_sort"; mode: SortMode }
  // 迁移
  | { type: "calculate_sizes" }
  | { type: "start_migration"; ids: string[] }
  | { type: "cancel_migration"; id: string }
  | { type: "rollback"; id: string }
  // UI
  | { type: "set_loading"; kind: LoadingKind }
  | { type: "show_toast"; kind: ToastKind; message: string }
  | { type: "dismiss_toast" };

// ============ Tauri events ============

export interface CopyProgress {
  app_id: string;
  copied: number;
  total: number;
  speed_bps: number;
}
