<template>
  <router-view />
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { ElMessage } from 'element-plus'
import { runtime } from '@/lib/runtime'
import { useUpdater } from '@/composables/useUpdater'

let unlisten: (() => void) | undefined

onMounted(async () => {
  if (runtime.kind !== 'desktop') return
  const { listen } = await import('@tauri-apps/api/event')
  const { invoke } = await import('@tauri-apps/api/core')
  unlisten = await listen<string>('tray-action', async (e) => {
    try {
      if (e.payload === 'start') {
        await invoke('start_proxy')
        ElMessage.success('代理已启动')
      } else if (e.payload === 'stop') {
        await invoke('stop_proxy')
        ElMessage.success('代理已停止')
      }
    } catch (err) {
      ElMessage.error(err instanceof Error ? err.message : String(err))
    }
  })

  const { checkOnStartup } = useUpdater()
  checkOnStartup()
})

onUnmounted(() => unlisten?.())
</script>

<style>
html.dark {
  color-scheme: dark;
}
</style>
