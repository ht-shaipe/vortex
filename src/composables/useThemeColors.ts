/**
 * 把 theme.css 的 CSS 变量解析成具体色值，供 echarts / canvas 这类
 * 无法直接吃 `var(--x)` 的渲染器使用；并在明暗主题切换时自动更新。
 */
import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

/** 主题色集合接口。 */
export interface ThemeColors {
  /** 强调色 */
  accent: string
  /** 主文字色 */
  ink: string
  /** 三级文字色 */
  ink3: string
  /** 四级文字色 */
  ink4: string
  /** 分割线色 */
  line: string
  /** 表面色 */
  surface: string
  /** 三级表面色 */
  surface3: string
  /** 成功色 */
  ok: string
  /** 错误色 */
  err: string
  /** 序列色（4 色，用于图表系列区分） */
  seq: [string, string, string, string]
}

/**
 * 读取指定 CSS 变量的值，取不到时返回 fallback。
 * @param name - CSS 变量名（如 '--accent'）
 * @param fallback - 取值失败时的回退色值
 * @returns 具体色值字符串
 */
function read(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return v || fallback
}

/**
 * 采集当前主题下所有需要的色值快照。
 * @returns 包含全部主题色的 ThemeColors 对象
 */
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
  // 初始化色值快照
  const colors = ref<ThemeColors>(snapshot())
  let observer: MutationObserver | null = null

  onMounted(() => {
    // 挂载时重新采集一次（确保 DOM 就绪后取值准确）
    colors.value = snapshot()
    // 监听 <html> class 属性变化，主题切换时刷新色值
    observer = new MutationObserver(() => {
      colors.value = snapshot()
    })
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
  })

  onBeforeUnmount(() => {
    // 卸载时断开观察器
    observer?.disconnect()
    observer = null
  })

  return colors
}
