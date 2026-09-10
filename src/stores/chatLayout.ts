/**
 * 对话页布局状态（移植自 ccMesh layout store 的 chat 切片）。
 * 会话列表折叠 + 输入框高度/展开态，持久化到 localStorage。
 */
import { defineStore } from 'pinia'

/** 顶拖条 + 底栏（小发送钮）占位。 */
export const COMPOSER_CHROME_PX = 50
/** 单行文字区最小高度。 */
export const COMPOSER_LINE_PX = 32
/** 拖拽/外壳下限：单行文字不被裁切。 */
export const COMPOSER_MIN_PX = COMPOSER_CHROME_PX + COMPOSER_LINE_PX // 68
export const COMPOSER_MAX_PX = 320
export const COMPOSER_DEFAULT_PX = COMPOSER_MIN_PX
/** 展开态占主列高度比例。 */
export const COMPOSER_EXPAND_RATIO = 0.55

export function clampComposerHeight(px: number, maxPx: number = COMPOSER_MAX_PX): number {
  return Math.min(maxPx, Math.max(COMPOSER_MIN_PX, Math.round(px)))
}

/** 内容区高度 + chrome → 外壳高度。 */
export function shellFromContent(contentPx: number): number {
  return clampComposerHeight(contentPx + COMPOSER_CHROME_PX)
}

const KEY = 'vortex-chat-layout'

interface Persisted {
  topicListCollapsed: boolean
  composerHeightPx: number
  composerExpanded: boolean
}

function read(): Persisted {
  const fallback: Persisted = {
    topicListCollapsed: false,
    composerHeightPx: COMPOSER_DEFAULT_PX,
    composerExpanded: false,
  }
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return fallback
    return { ...fallback, ...(JSON.parse(raw) as Partial<Persisted>) }
  } catch {
    return fallback
  }
}

export const useChatLayoutStore = defineStore('chatLayout', {
  state: (): Persisted => read(),
  actions: {
    persist(): void {
      try {
        localStorage.setItem(KEY, JSON.stringify(this.$state))
      } catch {
        /* ignore */
      }
    },
    toggleTopicList(): void {
      this.topicListCollapsed = !this.topicListCollapsed
      this.persist()
    },
    setComposerHeightPx(px: number): void {
      this.composerHeightPx = clampComposerHeight(px)
      this.persist()
    },
    setComposerExpanded(v: boolean): void {
      this.composerExpanded = v
      this.persist()
    },
  },
})
