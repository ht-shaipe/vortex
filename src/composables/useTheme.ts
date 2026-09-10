import { ref, watchEffect } from 'vue'

const THEME_KEY = 'vortex-theme'

export type ThemeMode = 'light' | 'dark' | 'system'

const stored = (localStorage.getItem(THEME_KEY) as ThemeMode | null) || 'system'

const mode = ref<ThemeMode>(stored)

function systemDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function apply(): void {
  const dark = mode.value === 'dark' || (mode.value === 'system' && systemDark())
  document.documentElement.classList.toggle('dark', dark)
}

watchEffect(apply)

export function useTheme() {
  function setMode(next: ThemeMode): void {
    mode.value = next
    localStorage.setItem(THEME_KEY, next)
    apply()
  }
  const isDark = () => document.documentElement.classList.contains('dark')
  return { mode, setMode, isDark }
}