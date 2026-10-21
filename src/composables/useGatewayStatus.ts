/**
 * 网关健康状态组合式函数。
 *
 * 维护一个模块级共享的轮询器：每 30 秒请求 `/api/health`，并在桌面端
 * 监听托盘「启动/停止」事件。多个组件同时使用时通过引用计数复用同一个
 * 定时器，最后一个组件卸载后自动清理。
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { runtime } from '@/lib/runtime'
import { gatewayBase } from '@/lib/gateway'

/** 网关健康状态（模块级共享，所有调用方看到同一份状态） */
const healthOk = ref(false)
/** 共享轮询定时器 */
let _timer: ReturnType<typeof setInterval> | null = null
/** 托盘事件取消监听函数 */
let _unlistenTray: (() => void) | undefined
/** 当前使用该组合式函数的组件数量 */
let _refCount = 0

/** 请求一次健康检查并按 `status === 'ok'` 更新状态；网络异常视为不健康。 */
async function checkHealth() {
  try {
    const res = await fetch(`${gatewayBase()}/api/health`)
    const data = await res.json()
    healthOk.value = data.status === 'ok'
  } catch {
    healthOk.value = false
  }
}

/**
 * 订阅网关健康状态。
 *
 * 仅在引用计数从 0 变为 1 时启动轮询与托盘监听，
 * 组件卸载后按引用计数统一释放。
 */
export function useGatewayStatus() {
  onMounted(async () => {
    _refCount++
    if (_refCount === 1) {
      checkHealth()
      _timer = setInterval(checkHealth, 30000)
      if (runtime.kind === 'desktop') {
        const { listen } = await import('@tauri-apps/api/event')
        _unlistenTray = await listen<string>('tray-action', (e) => {
          if (e.payload === 'started') healthOk.value = true
          else if (e.payload === 'stopped') healthOk.value = false
        })
      }
    }
  })

  onUnmounted(() => {
    _refCount--
    if (_refCount === 0) {
      if (_timer) clearInterval(_timer)
      _unlistenTray?.()
      _timer = null
      _unlistenTray = undefined
    }
  })

  return { healthOk }
}