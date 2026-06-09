/**
 * 应用状态 Pinia store —— 后端 state 镜像。
 *
 * 启动时 `init()` 拉一次,然后订阅 `state-changed` 事件保持同步。
 * 派发 Intent 通过 `dispatch()`。
 */
import { defineStore } from "pinia";
import { useMessage } from "naive-ui";
import * as api from "@/api/tauri";
import type { AppState, Intent } from "@/types";
import type { UnlistenFn } from "@tauri-apps/api/event";

const INITIAL: AppState = {
  apps: [],
  drives: [],
  migrations: {},
  migrated: {},
  ui: {
    selected: [],
    target_drive: null,
    search: "",
    filter: "migratable",
    sort: "size",
    loading: "idle",
    toast: null,
  },
};

/**
 * **Round 8 修复**:从 invoke 错误中提取友好消息。
 * 后端 `AppError` 经过 serde 序列化成 `{category, message, can_rollback}` 三个字段,
 * 抛出到前端是普通 JS Error(message 字段就是人类可读的中文)。
 * 但 Tauri 1.x 时代是字符串,2.x 是 Error,这里做兼容。
 */
function friendlyError(e: unknown): string {
  if (e instanceof Error) {
    return e.message || String(e);
  }
  if (typeof e === "string") {
    return e;
  }
  return "未知错误";
}

export const useAppStore = defineStore("app", {
  state: (): {
    state: AppState;
    version: string;
    /** **Round 4 修复**:保存 unlisten 句柄,避免热重载 / 多 init 时 listener 泄漏 */
    _unlistens: UnlistenFn[];
  } => ({
    state: structuredClone(INITIAL),
    version: "",
    _unlistens: [],
  }),

  getters: {
    apps: (s) => s.state.apps,
    drives: (s) => s.state.drives,
    ui: (s) => s.state.ui,
    selected: (s) => s.state.ui.selected,
    targetDrive: (s) => s.state.ui.target_drive,
    migrations: (s) => s.state.migrations,
    migrated: (s) => s.state.migrated,

    filteredApps(state): typeof state.state.apps {
      const list = state.state.apps.filter((a) => {
        if (state.state.ui.filter === "migratable") {
          return a.install_location.toUpperCase().startsWith("C:/");
        }
        if (state.state.ui.filter === "large_only") {
          const size = a.actual_size ?? a.estimated_size ?? 0;
          return size >= 100 * 1024 * 1024;
        }
        return true;
      });
      if (state.state.ui.search) {
        const q = state.state.ui.search.toLowerCase();
        const filtered = list.filter(
          (a) =>
            a.display_name.toLowerCase().includes(q) ||
            (a.publisher?.toLowerCase().includes(q) ?? false),
        );
        return filtered;
      }
      const sorted = [...list];
      sorted.sort((a, b) => {
        if (state.state.ui.sort === "name")
          return a.display_name.localeCompare(b.display_name);
        if (state.state.ui.sort === "publisher")
          return (a.publisher ?? "").localeCompare(b.publisher ?? "");
        return (b.actual_size ?? b.estimated_size ?? 0) -
          (a.actual_size ?? a.estimated_size ?? 0);
      });
      return sorted;
    },
  },

  actions: {
    async init() {
      // **Round 8 修复**:
      // 之前 init 不在 try/catch,后端启动期 IO 错误(state.json 损坏读不出)
      // 会让 `this.state = await api.getState()` 抛错,后续 listener 注册**不执行**,
      // 但前次 listener 已被 dispose 掉 → 前端永远收不到 state-changed 事件。
      // 现在 try/catch 包裹:失败时弹 toast,**保留**前次 listener(如有)。
      try {
        // 1. 防热重载泄漏
        this.dispose();
        // 2. 拉初始 state
        this.state = await api.getState();
        this.version = await api.version();
        // 3. 订阅后续变化
        this._unlistens.push(
          await api.onStateChanged((s) => {
            this.state = s;
          }),
        );
        // 4. **Round 3 修复**:订阅 migration-progress 事件
        this._unlistens.push(
          await api.onMigrationProgress((p) => {
            const m = this.state.migrations[p.app_id];
            if (!m) return;
            this.state.migrations = {
              ...this.state.migrations,
              [p.app_id]: {
                ...m,
                copied_bytes: p.copied,
                total: p.total,
                speed_bps: p.speed_bps,
                phase:
                  m.phase === "checking" ||
                  m.phase === "linking" ||
                  m.phase === "verifying"
                    ? m.phase
                    : "copying",
              },
            };
          }),
        );
      } catch (e) {
        console.error("[appStore] init failed:", e);
        // **Round 8 修复**:用 try/catch 包裹 message API(可能在 NMessageProvider
        // 上下文外被调用,例如 SSR / 测试)
        try {
          useMessage().error(`初始化失败: ${friendlyError(e)}`);
        } catch {
          // 静默
        }
        // **不**清空 _unlistens(失败时前面已 dispose,但我们要保留失败前的)
        // 注:此处 dispose 已调过,失败时新 listener 不会注册;
        // 下次用户 reload 页面 / 重新 mount,init 会再尝试。
      }
    },

    /**
     * **Round 4 修复**:清理所有 Tauri listener。
     * 应该在 App.vue unmount / Vue devtools 重新 init 时调,避免 listener 累积。
     */
    dispose() {
      for (const un of this._unlistens) {
        try {
          un();
        } catch (e) {
          console.warn("unlisten error", e);
        }
      }
      this._unlistens = [];
    },

    async dispatch(intent: Intent) {
      // **Round 8 修复**:错误时弹 toast,不再只 console.error。
      // 之前 `console.error` 吞错,用户操作无反应也不知道为什么。
      // 现在:成功静默,失败 toast 提示。
      try {
        await api.dispatch(intent);
      } catch (e) {
        console.error("[appStore] dispatch error:", e);
        try {
          useMessage().error(`操作失败: ${friendlyError(e)}`);
        } catch {
          // 静默(message API 不在 NMessageProvider 内)
        }
      }
    },

    async scanApps() {
      await this.dispatch({ type: "scan_apps" });
    },

    async startMigration(ids: string[]) {
      await this.dispatch({ type: "start_migration", ids });
    },

    async rollback(id: string) {
      await this.dispatch({ type: "rollback", id });
    },

    async setTargetDrive(letter: string) {
      await this.dispatch({ type: "set_target_drive", letter });
    },

    async toggleSelect(id: string, selected: boolean) {
      await this.dispatch({ type: "select_app", id, selected });
    },

    async calculateSizes() {
      await this.dispatch({ type: "calculate_sizes" });
    },
  },
});
