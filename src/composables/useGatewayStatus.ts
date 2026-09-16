import { ref, onMounted, onUnmounted } from 'vue'
import { runtime } from '@/lib/runtime'

const healthOk = ref(false)
let _timer: ReturnType<typeof setInterval> | null = null
let _unlistenTray: (() => void) | undefined
let _refCount = 0

async function checkHealth() {
  try {
    const res = await fetch('http://localhost:10168/api/health')
    const data = await res.json()
    healthOk.value = data.status === 'ok'
  } catch {
    healthOk.value = false
  }
}

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