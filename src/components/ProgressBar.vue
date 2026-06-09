<script setup lang="ts">
import { computed } from "vue";
import { NProgress, NText, NSpace } from "naive-ui";
import { useAppStore } from "@/stores/appStore";

const store = useAppStore();

const activeMigrations = computed(() => {
  return Object.values(store.migrations).filter(
    (m) =>
      m.phase === "copying" ||
      m.phase === "checking" ||
      m.phase === "linking" ||
      m.phase === "verifying",
  );
});

const totalProgress = computed(() => {
  const list = activeMigrations.value;
  if (list.length === 0) return null;
  // **Round 8 修复**(revised):
  // 1. 跳过 `total === 0` 的 app(算大小失败 / 未估),避免 `0/1 = 0` 让该 app 拖累平均
  // 2. 总进度用 byte-weighted 平均 `sum(copied) / sum(total)`:
  //    - 100MB / (50GB+100MB) = 0.2%(而不是 50%!这是符合"已完成工作量"的语义)
  //    - 这个算法**不会**出现"单 app 完成时 100%"的伪问题(因为 sum(copied)
  //      是 N 个 app 的累计,sum(total) 也是 N 个 app 的累计,只有全部完成才到 100%)
  // 3. 之前 Round 8 draft 改成"max(per-app-percent)"是**错的**:fastest app
  //    100% 时 stuck at 100% 即便其他 app 没开始
  const valid = list.filter((m) => m.total > 0);
  if (valid.length === 0) return null;
  const totalCopied = valid.reduce((s, m) => s + m.copied_bytes, 0);
  const totalSize = valid.reduce((s, m) => s + m.total, 0);
  const percent = Math.min(100, Math.round((totalCopied / totalSize) * 100));
  // 速度仍按"所有 active 累加"(这个是正确的,代表总 IO 吞吐)
  const totalSpeed = list.reduce((s, m) => s + m.speed_bps, 0);
  return {
    percent,
    speed: (totalSpeed / 1024 / 1024).toFixed(2),
  };
});

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
</script>

<template>
  <NSpace align="center" :wrap="false" v-if="totalProgress">
    <NText style="font-size: 12px">迁移中</NText>
    <div style="flex: 1; max-width: 600px">
      <NProgress
        type="line"
        :percentage="totalProgress.percent"
        :show-indicator="false"
        :height="6"
        processing
      />
    </div>
    <NText style="font-size: 12px" depth="2">
      {{ totalProgress.percent }}% · {{ totalProgress.speed }} MB/s
    </NText>
  </NSpace>
  <NText v-else depth="3" style="font-size: 12px">
    空闲 · 已迁移 {{ Object.keys(store.migrated).length }} 个应用
  </NText>
</template>
