/**
 * 开机自启管理组合式函数。
 * 直接通过 Tauri invoke 调用 autostart 插件命令，
 * 避免依赖独立的 @tauri-apps/plugin-autostart 包。
 */
import { ref } from 'vue'

const enabled = ref(false)
let _initialized = false

/** 初始化：读取当前自启状态。仅在桌面环境调用一次。 */
async function init() {
  if (_initialized) return
  _initialized = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    enabled.value = await invoke<boolean>('plugin:autostart|is_enabled')
  } catch {
    /* 非桌面环境或插件未注册 */
  }
}

/** 启用开机自启。 */
async function enable() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('plugin:autostart|enable')
    enabled.value = true
  } catch {
    /* ignore */
  }
}

/** 禁用开机自启。 */
async function disable() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('plugin:autostart|disable')
    enabled.value = false
  } catch {
    /* ignore */
  }
}

/** 切换开机自启状态。 */
async function toggle() {
  if (enabled.value) {
    await disable()
  } else {
    await enable()
  }
}

export function useAutoStart() {
  return { enabled, init, enable, disable, toggle }
}
