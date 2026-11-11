<template>
  <!-- 根路由出口，渲染当前路由匹配的页面 -->
  <router-view />
</template>

<script setup lang="ts">
/**
 * 根组件。
 * 在桌面环境下监听托盘菜单事件（启动/停止代理），
 * 并在应用启动时检查更新、开启周期性更新复查与通知轮询。
 */
import { onMounted, onUnmounted } from 'vue'
import { ElMessage } from 'element-plus'
import { runtime } from '@/lib/runtime'
import { useUpdater } from '@/composables/useUpdater'
import { useNotifications, type PushEventPayload } from '@/composables/useNotifications'

// 托盘事件监听的取消函数
let unlisten: (() => void) | undefined
// hub 推送事件监听的取消函数
let unlistenPush: (() => void) | undefined
// 停止周期性更新检查的函数
let _stopPeriodicUpdateCheck: (() => void) | undefined

onMounted(async () => {
  // 回填启动/刷新期间错过的推送进收件箱（两种运行模式都执行：
  // 网关内存缓冲经 GET /api/push-events 回填，已读去重不重弹；
  // Web 模式无实时通道，加载时回填是收件箱的唯一来源）
  const { loadRecentPush } = useNotifications()
  void loadRecentPush()

  // 仅在桌面环境（Tauri）下监托盘事件与后端实时推送
  if (runtime.kind !== 'desktop') return
  const { listen } = await import('@tauri-apps/api/event')
  // 监听托盘菜单发出的 'tray-action' 事件（后端已直接执行启停，前端仅提示）
  unlisten = await listen<string>('tray-action', async (e) => {
    if (e.payload === 'started') {
      ElMessage.success('代理已启动')
    } else if (e.payload === 'stopped') {
      ElMessage.success('代理已停止')
    }
  })

  // 监听中央 hub 的 channel 实时推送（活动通知），弹 toast 并进收件箱
  const { ingestPush } = useNotifications()
  unlistenPush = await listen<PushEventPayload>('hub-push', (e) => ingestPush(e.payload))

  // 启动时静默检查是否有新版本，并在应用持续运行期间周期性复查（默认每 6 小时）
  const { checkOnStartup, startPeriodicCheck, stopPeriodicCheck } = useUpdater()
  checkOnStartup()
  startPeriodicCheck()
  _stopPeriodicUpdateCheck = stopPeriodicCheck
})

onMounted(() => {
  // 启动通知轮询
  const { startPolling } = useNotifications()
  startPolling()
})

onUnmounted(() => {
  // 组件卸载时取消托盘事件监听
  unlisten?.()
  // 取消 hub 推送事件监听
  unlistenPush?.()
  // 停止周期性更新检查
  _stopPeriodicUpdateCheck?.()
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
