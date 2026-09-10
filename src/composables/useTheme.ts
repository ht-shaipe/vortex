/**
 * 主题管理组合式函数。
 * 支持浅色（light）、暗色（dark）、跟随系统（system）三种模式，
 * 通过切换 <html> 元素的 'dark' class 实现主题切换，
 * 并将用户选择持久化到 localStorage。
 */
import { ref, watchEffect } from 'vue'

/** localStorage 中存储主题模式的键名。 */
const THEME_KEY = 'vortex-theme'

/** 主题模式类型：浅色 / 暗色 / 跟随系统。 */
export type ThemeMode = 'light' | 'dark' | 'system'

// 从 localStorage 读取已保存的主题模式，默认跟随系统
const stored = (localStorage.getItem(THEME_KEY) as ThemeMode | null) || 'system'

// 当前主题模式的响应式引用
const mode = ref<ThemeMode>(stored)

/**
 * 检测系统是否偏好暗色主题。
 * @returns 系统是否为暗色模式
 */
function systemDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

/**
 * 根据当前模式应用主题：切换 <html> 的 'dark' class。
 * 'system' 模式下跟随系统偏好。
 */
function apply(): void {
  const dark = mode.value === 'dark' || (mode.value === 'system' && systemDark())
  document.documentElement.classList.toggle('dark', dark)
}

// 初始化时立即应用一次
apply()

// 监听系统主题变化，'system' 模式下自动响应
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if (mode.value === 'system') apply()
})

// 响应式监听 mode 变化自动应用主题
watchEffect(apply)

/**
 * 主题管理组合式函数。
 * @returns 包含当前模式 ref、设置模式方法和暗色判断方法的对象
 */
export function useTheme() {
  /**
   * 设置主题模式并持久化。
   * @param next - 目标主题模式
   */
  function setMode(next: ThemeMode): void {
    mode.value = next
    localStorage.setItem(THEME_KEY, next)
    apply()
  }

  /** 判断当前是否为暗色主题。 */
  const isDark = () => document.documentElement.classList.contains('dark')

  return { mode, setMode, isDark }
}
