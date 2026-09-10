import { ref, readonly } from 'vue'
import { runtime } from '@/lib/runtime'

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'up-to-date'
  | 'downloading'
  | 'installing'
  | 'error'

interface UpdateInfo {
  version: string
  body: string
  date: string
}

const status = ref<UpdateStatus>('idle')
const updateInfo = ref<UpdateInfo | null>(null)
const errorMsg = ref<string>('')
const downloadProgress = ref(0)
const currentVersion = ref<string>('')

let _update: any = null

async function checkForUpdate(silent = false): Promise<boolean> {
  if (runtime.kind !== 'desktop') {
    if (!silent) errorMsg.value = '桌面环境才支持自动更新'
    return false
  }

  status.value = 'checking'
  errorMsg.value = ''
  updateInfo.value = null
  _update = null

  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const { getVersion } = await import('@tauri-apps/api/app')
    currentVersion.value = await getVersion()

    const update = await check()
    if (update) {
      _update = update
      updateInfo.value = {
        version: update.version,
        body: update.body || '',
        date: update.date || '',
      }
      status.value = 'available'
      return true
    } else {
      status.value = 'up-to-date'
      return false
    }
  } catch (e) {
    status.value = 'error'
    errorMsg.value = e instanceof Error ? e.message : String(e)
    return false
  }
}

async function downloadAndInstall(): Promise<void> {
  if (!_update) {
    errorMsg.value = '没有可用的更新'
    status.value = 'error'
    return
  }

  status.value = 'downloading'
  downloadProgress.value = 0
  errorMsg.value = ''

  try {
    let total = 0
    let downloaded = 0

    await _update.downloadAndInstall((event: any) => {
      switch (event.event) {
        case 'Started':
          total = event.data.contentLength ?? 0
          break
        case 'Progress':
          downloaded += event.data.chunkLength
          if (total > 0) {
            downloadProgress.value = Math.round((downloaded / total) * 100)
          }
          break
        case 'Finished':
          downloadProgress.value = 100
          status.value = 'installing'
          break
      }
    })

    status.value = 'installing'

    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  } catch (e) {
    status.value = 'error'
    errorMsg.value = e instanceof Error ? e.message : String(e)
  }
}

async function checkOnStartup(): Promise<void> {
  if (runtime.kind !== 'desktop') return
  try {
    const hasUpdate = await checkForUpdate(true)
    if (hasUpdate && updateInfo.value) {
      const { ElNotification } = await import('element-plus')
      ElNotification({
        title: `发现新版本 v${updateInfo.value.version}`,
        message: '点击"检查更新"页面查看详情并安装',
        type: 'info',
        duration: 8000,
        position: 'bottom-right',
      })
    }
  } catch {
    // 静默失败，不打扰用户
  }
}

export function useUpdater() {
  return {
    status: readonly(status),
    updateInfo: readonly(updateInfo),
    errorMsg: readonly(errorMsg),
    downloadProgress: readonly(downloadProgress),
    currentVersion: readonly(currentVersion),
    checkForUpdate,
    downloadAndInstall,
    checkOnStartup,
  }
}
