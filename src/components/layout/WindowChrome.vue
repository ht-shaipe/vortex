<template>
  <!-- 仅保留拖拽区，窗口控制交给系统标题栏（Overlay 模式下交通灯按钮在左上角） -->
  <div v-if="runtime.kind === 'desktop'" class="window-chrome" data-tauri-drag-region />
</template>

<script setup lang="ts">
/**
 * WindowChrome.vue — 窗口拖拽区域
 * 职责：在桌面端渲染顶部拖拽区域（data-tauri-drag-region），覆盖整个窗口顶部，
 * 使标题栏区域可拖动窗体。窗口控制按钮交由 macOS Overlay 标题栏的系统交通灯处理。
 */
import { runtime } from '@/lib/runtime'
</script>

<style scoped>
.window-chrome {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 38px;
  z-index: 20;
  user-select: none;
  -webkit-user-select: none;
  -webkit-app-region: drag;
}
</style>
