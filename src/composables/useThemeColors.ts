/**
 * 把 cc-theme.css 的 CSS 变量解析成具体色值，供 echarts / canvas 这类
 * 无法直接吃 `var(--x)` 的渲染器使用；并在明暗主题切换时自动更新。
 */
import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

export interface ThemeColors {
  accent: string
  ink: string
  ink3: string
  ink4: string
  line: string
  surface: string
  surface3: string
  ok: string
  err: string
  seq: [string, string, string, string]
}

function read(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return v || fallback
}

function snapshot(): ThemeColors {
  return {
    accent: read('--accent', '#6d5bd0'),
    ink: read('--ink', '#1a1a22'),
    ink3: read('--ink-3', '#6b6b76'),
    ink4: read('--ink-4', '#9a9aa6'),
    line: read('--line', '#e4e4ec'),
    surface: read('--surface', '#ffffff'),
    surface3: read('--surface-3', '#f0f0f4'),
    ok: read('--ok', '#3f9c6a'),
    err: read('--err', '#c0453a'),
    seq: [
      read('--seq-1', '#b8a8e0'),
      read('--seq-2', '#9d86d4'),
      read('--seq-3', '#7d63c8'),
      read('--seq-4', '#5e42b0'),
    ],
  }
}

/** 响应式主题色。挂载后监听 `html` 的 class 变化（明暗切换）刷新取值。 */
export function useThemeColors(): Ref<ThemeColors> {
  const colors = ref<ThemeColors>(snapshot())
  let observer: MutationObserver | null = null

  onMounted(() => {
    colors.value = snapshot()
    observer = new MutationObserver(() => {
      colors.value = snapshot()
    })
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
  })

  onBeforeUnmount(() => {
    observer?.disconnect()
    observer = null
  })

  return colors
}
