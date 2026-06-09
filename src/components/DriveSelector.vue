<script setup lang="ts">
import { computed } from "vue";
import { NSelect, NSpace, NText } from "naive-ui";
import { useAppStore } from "@/stores/appStore";
import type { DriveInfo } from "@/types";

const store = useAppStore();

const options = computed(() =>
  store.drives.map((d: DriveInfo) => ({
    label: `${d.letter}  ${d.label ?? ""}  (${formatBytes(d.available)} / ${formatBytes(d.total)} 可用)${d.is_system ? "  · 系统盘" : ""}`,
    value: d.letter,
    disabled: d.is_system,
  })),
);

async function onChange(letter: string) {
  await store.setTargetDrive(letter);
}

function formatBytes(n: number): string {
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(0)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
</script>

<template>
  <NSpace align="center" :wrap="false">
    <NText style="font-size: 13px">目标盘:</NText>
    <NSelect
      :value="store.targetDrive"
      :options="options"
      @update:value="onChange"
      placeholder="选择目标盘"
      style="width: 420px"
      :consistent-menu-width="false"
    />
  </NSpace>
</template>
