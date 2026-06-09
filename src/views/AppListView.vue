<script setup lang="ts">
import { computed, h, ref } from "vue";
import {
  NButton,
  NCard,
  NCheckbox,
  NDataTable,
  NEmpty,
  NInput,
  NSelect,
  NSpace,
  NTag,
  NText,
  NTooltip,
  type DataTableColumns,
} from "naive-ui";
import { useAppStore } from "@/stores/appStore";
import type { InstalledApp, MigrationPhase } from "@/types";
import DriveSelector from "@/components/DriveSelector.vue";

const store = useAppStore();

const filterOptions = [
  { label: "可迁移", value: "migratable" },
  { label: "大于 100MB", value: "large_only" },
  { label: "全部", value: "all" },
];
const sortOptions = [
  { label: "按大小", value: "size" },
  { label: "按名称", value: "name" },
  { label: "按厂商", value: "publisher" },
];

async function onFilter(value: string) {
  await store.dispatch({ type: "set_filter", mode: value as any });
}
async function onSort(value: string) {
  await store.dispatch({ type: "set_sort", mode: value as any });
}
async function onSearch(value: string) {
  await store.dispatch({ type: "set_search", query: value });
}
async function onSelect(id: string, checked: boolean) {
  await store.toggleSelect(id, checked);
}
async function onSelectAll() {
  await store.dispatch({ type: "select_all" });
}
async function onClearSelection() {
  await store.dispatch({ type: "clear_selection" });
}
async function onScan() {
  await store.scanApps();
}
async function onCalculateSizes() {
  await store.calculateSizes();
}
async function onStart() {
  if (store.selected.length === 0) return;
  if (!store.targetDrive) {
    await store.dispatch({
      type: "show_toast",
      kind: "warning",
      message: "请先选择目标盘",
    });
    return;
  }
  await store.startMigration([...store.selected]);
}
async function onRollback(id: string) {
  await store.rollback(id);
}

function formatBytes(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

const phaseColor: Record<MigrationPhase, string> = {
  idle: "default",
  checking: "info",
  copying: "info",
  linking: "warning",
  verifying: "info",
  completed: "success",
  failed: "error",
  rolling_back: "warning",
  rolled_back: "default",
  rollback_failed: "error",
  cancelled: "default",
};

const columns = computed<DataTableColumns<InstalledApp>>(() => [
  {
    title: () =>
      h(NCheckbox
        , {
          checked: store.selected.length === store.filteredApps.length && store.filteredApps.length > 0,
          indeterminate: store.selected.length > 0 && store.selected.length < store.filteredApps.length,
          "onUpdate:checked": (v: boolean) => (v ? onSelectAll() : onClearSelection()),
        }),
    key: "select",
    width: 50,
    render: (row) =>
      h(NCheckbox, {
        checked: store.selected.includes(row.id),
        "onUpdate:checked": (v: boolean) => onSelect(row.id, v),
      }),
  },
  {
    title: "应用",
    key: "display_name",
    render: (row) =>
      h(NSpace, { vertical: true, size: 2 }, () => [
        h(NText, { strong: true }, () => row.display_name),
        h(NText, { depth: 3, style: "font-size: 11px" }, () => row.publisher ?? "—"),
      ]),
  },
  {
    title: "安装位置",
    key: "install_location",
    render: (row) => h(NText, { code: true, style: "font-size: 11px" }, () => row.install_location),
  },
  {
    title: "占用",
    key: "size",
    // **Round 3 修复**:移除 sorter,否则会覆盖 filteredApps 内的 sort,
    // 导致 Naive UI 内部排序和我们 store 排序重复,产生视觉跳动。
    render: (row) =>
      h(NSpace, { vertical: true, size: 0 }, () => [
        h(NText, { style: "font-size: 12px" }, () => formatBytes(row.actual_size ?? row.estimated_size)),
        row.actual_size
          ? h(NText, { depth: 3, style: "font-size: 10px" }, () => "实测")
          : h(NText, { depth: 3, style: "font-size: 10px" }, () => "注册表估算"),
      ]),
  },
  {
    title: "状态",
    key: "phase",
    render: (row) => {
      const m = store.migrations[row.id];
      if (!m) return h(NText, { depth: 3, style: "font-size: 11px" }, () => "—");
      return h(NTag, { type: phaseColor[m.phase] as any, size: "small" }, () => m.phase);
    },
  },
  {
    title: "操作",
    key: "action",
    width: 100,
    render: (row) => {
      const m = store.migrations[row.id];
      if (m?.phase === "completed") {
        return h(NButton, { size: "tiny", quaternary: true, type: "warning", onClick: () => onRollback(row.id) }, () => "回滚");
      }
      return null;
    },
  },
]);
</script>

<template>
  <NSpace vertical :size="12">
    <!-- 工具栏 -->
    <NCard size="small" :bordered="false">
      <NSpace align="center" :wrap="true">
        <NButton type="primary" @click="onScan" :loading="store.ui.loading === 'scanning'">
          扫描应用
        </NButton>
        <NButton @click="onCalculateSizes" :disabled="store.apps.length === 0" :loading="store.ui.loading === 'calculating_size'">
          计算大小
        </NButton>
        <NSelect
          :value="store.ui.filter"
          :options="filterOptions"
          @update:value="onFilter"
          style="width: 140px"
        />
        <NSelect
          :value="store.ui.sort"
          :options="sortOptions"
          @update:value="onSort"
          style="width: 120px"
        />
        <NInput
          :value="store.ui.search"
          @update:value="onSearch"
          placeholder="搜索应用名 / 厂商"
          clearable
          :maxlength="256"
          show-count
          style="width: 240px"
        />
        <DriveSelector />
        <!--
          **Round 12 修复**:"开始迁移" 按钮同样加 loading 期间的 disabled。
          否则用户在第一批 4 个迁移未完时再点,新 batch 会 join 队列
          (id 数受 MAX_BATCH_MIGRATIONS=32 限制),但浪费 4 个新 permit
          等待。
        -->
        <NButton
          type="success"
          :disabled="store.selected.length === 0 || store.ui.loading !== 'idle'"
          @click="onStart"
        >
          开始迁移 ({{ store.selected.length }})
        </NButton>
      </NSpace>
    </NCard>

    <!-- 选中汇总 -->
    <NText v-if="store.selected.length > 0" style="font-size: 12px" depth="2">
      已选 {{ store.selected.length }} 个应用
    </NText>

    <!-- 列表 -->
    <NDataTable
      :columns="columns"
      :data="store.filteredApps"
      :bordered="false"
      :single-line="false"
      size="small"
      :max-height="540"
      :row-key="(row: InstalledApp) => row.id"
      :empty-render="() => h(NEmpty, { description: '点击「扫描应用」开始' })"
    />
  </NSpace>
</template>
