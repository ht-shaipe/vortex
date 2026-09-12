/**
 * 自动更新组合式函数。
 * 在桌面环境（Tauri）下检查、下载并安装应用更新，
 * 提供更新状态、进度、版本信息等响应式数据。
 */
import { ref, readonly, defineComponent, h } from 'vue'
import { runtime } from '@/lib/runtime'

/** 更新流程状态枚举。 */
export type UpdateStatus =
  | 'idle'           // 空闲
  | 'checking'       // 检查中
  | 'available'      // 有可用更新
  | 'up-to-date'     // 已是最新版本
  | 'downloading'    // 下载中
  | 'ready'          // 下载完成，可重启安装
  | 'error'          // 出错

/** 更新信息接口。 */
interface UpdateInfo {
  /** 新版本号 */
  version: string
  /** 更新说明正文 */
  body: string
  /** 发布日期 */
  date: string
}

// 更新状态
const status = ref<UpdateStatus>('idle')
// 更新信息
const updateInfo = ref<UpdateInfo | null>(null)
// 错误消息
const errorMsg = ref<string>('')
// 下载进度（0-100）
const downloadProgress = ref(0)
// 当前应用版本号
const currentVersion = ref<string>('')

// Tauri updater 插件返回的更新对象
let _update: any = null

/**
 * 检查是否有可用更新。
 * @param silent - 是否静默模式（静默时不显示非桌面环境提示）
 * @returns 是否有可用更新
 */
async function checkForUpdate(silent = false): Promise<boolean> {
  // 非桌面环境不支持自动更新
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
    // 获取当前应用版本
    currentVersion.value = await getVersion()

    // 检查更新
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

/**
 * 下载并安装更新。
 * 下载过程中实时更新 downloadProgress，完成后状态变为 'ready'。
 */
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

    // 下载并安装，监听进度事件
    await _update.downloadAndInstall((event: any) => {
      switch (event.event) {
        case 'Started':
          // 下载开始，获取总大小
          total = event.data.contentLength ?? 0
          break
        case 'Progress':
          // 下载进行中，累加已下载量并计算百分比
          downloaded += event.data.chunkLength
          if (total > 0) {
            downloadProgress.value = Math.round((downloaded / total) * 100)
          }
          break
        case 'Finished':
          // 下载完成
          downloadProgress.value = 100
          break
      }
    })

    status.value = 'ready'
  } catch (e) {
    status.value = 'error'
    errorMsg.value = e instanceof Error ? e.message : String(e)
  }
}

/**
 * 重启应用以完成更新安装。
 */
async function relaunchApp(): Promise<void> {
  try {
    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  } catch (e) {
    status.value = 'error'
    errorMsg.value = e instanceof Error ? e.message : String(e)
  }
}

/** 下载进度通知内容组件（响应式读取 downloadProgress）。 */
const ProgressContent = defineComponent({
  setup() {
    return () => h('div', { style: 'display: flex; flex-direction: column; gap: 8px; width: 240px;' }, [
      h('span', { style: 'font-size: 12px; color: var(--ink-3);' }, `下载进度：${downloadProgress.value}%`),
      h('div', { style: 'height: 4px; background: var(--surface-3); border-radius: 2px; overflow: hidden;' }, [
        h('div', {
          style: `height: 100%; background: var(--accent); border-radius: 2px; width: ${downloadProgress.value}%; transition: width 0.2s ease;`,
        }),
      ]),
    ])
  },
})

/**
 * 从通知栏直接启动更新流程。
 * 下载完成后弹出确认框，用户确认后自动重启应用。
 */
async function startUpdateFromNotification(): Promise<void> {
  if (status.value === 'downloading') return

  try {
    const { ElNotification, ElMessageBox } = await import('element-plus')

    const downloadPromise = downloadAndInstall()

    const progressNotification = ElNotification({
      title: '正在下载更新',
      message: h(ProgressContent),
      type: 'info',
      duration: 0,
      position: 'bottom-right',
    })

    await downloadPromise
    progressNotification.close()

    if (status.value === 'ready') {
      try {
        await ElMessageBox.confirm(
          '更新已下载完成，是否立即重启应用以完成安装？',
          '更新完成',
          {
            confirmButtonText: '立即重启',
            cancelButtonText: '稍后',
            type: 'success',
          },
        )
        await relaunchApp()
      } catch {
        // 用户选择"稍后"，可稍后在"关于"页面重启
      }
    } else if (status.value === 'error') {
      ElNotification({
        title: '更新下载失败',
        message: errorMsg.value || '下载更新时出错',
        type: 'error',
        duration: 5000,
        position: 'bottom-right',
      })
    }
  } catch {
    // 整个流程出错，静默处理
  }
}

/**
 * 应用启动时静默检查更新。
 * 发现新版本时弹出通知，带"立即更新"按钮可直接下载安装。
 */
async function checkOnStartup(): Promise<void> {
  if (runtime.kind !== 'desktop') return
  try {
    const hasUpdate = await checkForUpdate(true)
    if (hasUpdate && updateInfo.value) {
      const { ElNotification } = await import('element-plus')

      const notification = ElNotification({
        title: `发现新版本 v${updateInfo.value.version}`,
        message: h('div', { style: 'display: flex; flex-direction: column; gap: 8px;' }, [
          h('p', { style: 'margin: 0; font-size: 12px; color: var(--ink-3);' }, '点击"立即更新"直接下载安装'),
          h('button', {
            class: 'btn primary sm',
            style: 'align-self: flex-start; cursor: pointer;',
            onClick: () => {
              notification.close()
              void startUpdateFromNotification()
            },
          }, '立即更新'),
        ]),
        type: 'info',
        duration: 0,
        position: 'bottom-right',
      })
    }
  } catch {
    // 静默失败，不打扰用户
  }
}

/**
 * 自动更新组合式函数。
 * @returns 包含更新状态、信息、进度等只读 ref 及操作方法的对象
 */
export function useUpdater() {
  return {
    status: readonly(status),
    updateInfo: readonly(updateInfo),
    errorMsg: readonly(errorMsg),
    downloadProgress: readonly(downloadProgress),
    currentVersion: readonly(currentVersion),
    checkForUpdate,
    downloadAndInstall,
    relaunchApp,
    checkOnStartup,
  }
}
