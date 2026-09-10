<template>
  <!-- 仅保留拖拽区，窗口控制交给系统标题栏（tauri.conf.json 未关闭 decorations） -->
  <div v-if="runtime.kind === 'desktop'" class="window-chrome" data-tauri-drag-region />
</template>

<script setup lang="ts">
/**
 * WindowChrome.vue — 窗口控制按钮
 * 职责：在桌面端渲染顶部拖拽区域（data-tauri-drag-region），窗口控制按钮交由系统标题栏处理。
 */
import { runtime } from '@/lib/runtime' // 运行时环境信息（desktop / web 等）
</script>

<style scoped>
.window-chrome {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 32px;
  z-index: 20;
  user-select: none;
  -webkit-user-select: none;
}
</style>
