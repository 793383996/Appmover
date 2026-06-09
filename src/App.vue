<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { NConfigProvider, NMessageProvider, NDialogProvider, NLayout, NLayoutHeader, NLayoutContent, NLayoutFooter, NSpace, NText, darkTheme } from "naive-ui";
import { useAppStore } from "@/stores/appStore";
import AppListView from "@/views/AppListView.vue";
import ProgressBar from "@/components/ProgressBar.vue";
import zhCN from "@/i18n/zh-CN";

const store = useAppStore();

onMounted(async () => {
  await store.init();
});

// **Round 4 修复**:组件卸载时 dispose Tauri listener,避免内存泄漏
onUnmounted(() => {
  store.dispose();
});
</script>

<template>
  <NConfigProvider :theme="darkTheme" :locale="zhCN as any">
    <NMessageProvider>
      <NDialogProvider>
        <NLayout style="height: 100vh">
          <NLayoutHeader bordered style="padding: 12px 20px">
            <NSpace align="center" justify="space-between" :wrap="false">
              <NSpace align="center">
                <NText strong style="font-size: 20px">📦 AppMover</NText>
                <NText depth="3" style="font-size: 12px">v{{ store.version || "…" }}</NText>
              </NSpace>
              <NSpace>
                <NText v-if="store.ui.loading !== 'idle'" depth="3" style="font-size: 12px">
                  {{ store.ui.loading }}…
                </NText>
              </NSpace>
            </NSpace>
          </NLayoutHeader>

          <NLayoutContent style="padding: 16px 20px; overflow: auto">
            <AppListView />
          </NLayoutContent>

          <NLayoutFooter bordered style="padding: 8px 20px">
            <ProgressBar />
          </NLayoutFooter>
        </NLayout>
      </NDialogProvider>
    </NMessageProvider>
  </NConfigProvider>
</template>
