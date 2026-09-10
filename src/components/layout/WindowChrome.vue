<template>
  <!-- 仅保留拖拽区，窗口控制交给系统标题栏（tauri.conf.json 未关闭 decorations） -->
  <div v-if="runtime.kind === 'desktop'" class="window-chrome" data-tauri-drag-region />
</template>

<script setup lang="ts">
import { runtime } from '@/lib/runtime'
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
