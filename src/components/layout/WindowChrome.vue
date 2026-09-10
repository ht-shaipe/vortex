<template>
  <div v-if="runtime.kind === 'desktop'" class="window-chrome" data-tauri-drag-region>
    <div class="win-controls">
      <button type="button" aria-label="最小化" @click="win('minimize')">
        <svg width="14" height="14" viewBox="0 0 14 14"><line x1="1" y1="7" x2="13" y2="7" stroke="currentColor" stroke-width="1.2"/></svg>
      </button>
      <button type="button" aria-label="最大化" @click="win('toggleMaximize')">
        <svg width="14" height="14" viewBox="0 0 14 14"><rect x="1.5" y="1.5" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>
      </button>
      <button type="button" class="close" aria-label="关闭" @click="win('close')">
        <svg width="14" height="14" viewBox="0 0 14 14"><line x1="2" y1="2" x2="12" y2="12" stroke="currentColor" stroke-width="1.2"/><line x1="12" y1="2" x2="2" y2="12" stroke="currentColor" stroke-width="1.2"/></svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { runtime } from '@/lib/runtime'

async function win(action: 'minimize' | 'toggleMaximize' | 'close') {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const w = getCurrentWindow()
    if (action === 'minimize') await w.minimize()
    else if (action === 'toggleMaximize') await w.toggleMaximize()
    else await w.close()
  } catch {
    /* web 或非 tauri 环境忽略 */
  }
}
</script>

<style scoped>
.window-chrome {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 32px;
  display: flex;
  justify-content: flex-end;
  z-index: 20;
  user-select: none;
  -webkit-user-select: none;
}
.win-controls { display: flex; height: 100%; }
.win-controls button {
  width: 46px;
  height: 100%;
  border: none;
  border-radius: 0;
  background: transparent;
  color: var(--ink-2);
  padding: 0;
  display: grid;
  place-items: center;
  transition: background 0.12s, color 0.12s;
}
.win-controls button:hover { background: var(--surface-3); color: var(--ink); }
.win-controls button.close:hover { background: #c42b1c; color: #fff; }
</style>