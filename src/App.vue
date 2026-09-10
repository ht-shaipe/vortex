<template>
  <!-- 根路由出口，渲染当前路由匹配的页面 -->
  <router-view />
</template>

<script setup lang="ts">
/**
 * 根组件。
 * 在桌面环境下监听托盘菜单事件（启动/停止代理），
 * 并在应用启动时检查更新、开启通知轮询。
 */
import { onMounted, onUnmounted } from 'vue'
import { ElMessage } from 'element-plus'
import { runtime } from '@/lib/runtime'
import { useUpdater } from '@/composables/useUpdater'
import { useNotifications } from '@/composables/useNotifications'

// 托盘事件监听的取消函数
let unlisten: (() => void) | undefined

onMounted(async () => {
  // 仅在桌面环境（Tauri）下监听托盘事件
  if (runtime.kind !== 'desktop') return
  const { listen } = await import('@tauri-apps/api/event')
  const { invoke } = await import('@tauri-apps/api/core')
  // 监听托盘菜单发出的 'tray-action' 事件
  unlisten = await listen<string>('tray-action', async (e) => {
    try {
      if (e.payload === 'start') {
        // 启动代理
        await invoke('start_proxy')
        ElMessage.success('代理已启动')
      } else if (e.payload === 'stop') {
        // 停止代理
        await invoke('stop_proxy')
        ElMessage.success('代理已停止')
      }
    } catch (err) {
      ElMessage.error(err instanceof Error ? err.message : String(err))
    }
  })

  // 启动时静默检查是否有新版本
  const { checkOnStartup } = useUpdater()
  checkOnStartup()
})

onMounted(() => {
  // 启动通知轮询
  const { startPolling } = useNotifications()
  startPolling()
})

onUnmounted(() => {
  // 组件卸载时取消托盘事件监听
  unlisten?.()
  // 停止通知轮询
  const { stopPolling } = useNotifications()
  stopPolling()
})
</script>

<style>
/* 暗色模式下设置 color-scheme，让浏览器原生控件也呈暗色 */
html.dark {
  color-scheme: dark;
}
</style>
