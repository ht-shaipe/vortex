/**
 * 对话页布局状态。
 * 会话列表折叠 + 输入框高度/展开态，持久化到 localStorage。
 */
import { defineStore } from 'pinia'

/** 顶拖条 + 底栏（小发送钮）占位。 */
export const COMPOSER_CHROME_PX = 50
/** 单行文字区最小高度。 */
export const COMPOSER_LINE_PX = 32
/** 拖拽/外壳下限：单行文字不被裁切。 */
export const COMPOSER_MIN_PX = COMPOSER_CHROME_PX + COMPOSER_LINE_PX // 68
/** 输入框外壳最大高度。 */
export const COMPOSER_MAX_PX = 320
/** 输入框外壳默认高度。 */
export const COMPOSER_DEFAULT_PX = COMPOSER_MIN_PX
/** 展开态占主列高度比例。 */
export const COMPOSER_EXPAND_RATIO = 0.55

/**
 * 将输入框高度限制在 [最小值, 最大值] 范围内并取整。
 * @param px - 期望高度（像素）
 * @param maxPx - 上限高度，默认 COMPOSER_MAX_PX
 * @returns 钳制后的高度
 */
export function clampComposerHeight(px: number, maxPx: number = COMPOSER_MAX_PX): number {
  return Math.min(maxPx, Math.max(COMPOSER_MIN_PX, Math.round(px)))
}

/** 内容区高度 + chrome → 外壳高度。 */
export function shellFromContent(contentPx: number): number {
  return clampComposerHeight(contentPx + COMPOSER_CHROME_PX)
}

/** localStorage 持久化键名。 */
const KEY = 'vortex-chat-layout'

/** 持久化数据结构。 */
interface Persisted {
  /** 会话列表是否折叠 */
  topicListCollapsed: boolean
  /** 输入框外壳高度（像素） */
  composerHeightPx: number
  /** 输入框是否展开 */
  composerExpanded: boolean
}

/**
 * 从 localStorage 读取布局状态，解析失败时返回默认值。
 * @returns 持久化的布局状态
 */
function read(): Persisted {
  const fallback: Persisted = {
    topicListCollapsed: false,
    composerHeightPx: COMPOSER_DEFAULT_PX,
    composerExpanded: false,
  }
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return fallback
    // 合并默认值与已存储值，保证字段完整
    return { ...fallback, ...(JSON.parse(raw) as Partial<Persisted>) }
  } catch {
    return fallback
  }
}

export const useChatLayoutStore = defineStore('chatLayout', {
  // 初始状态从 localStorage 读取
  state: (): Persisted => read(),
  actions: {
    /** 将当前状态写入 localStorage 持久化。 */
    persist(): void {
      try {
        localStorage.setItem(KEY, JSON.stringify(this.$state))
      } catch {
        /* ignore */
      }
    },
    /** 切换会话列表折叠/展开状态并持久化。 */
    toggleTopicList(): void {
      this.topicListCollapsed = !this.topicListCollapsed
      this.persist()
    },
    /**
     * 设置输入框外壳高度并持久化。
     * @param px - 期望高度（像素），会被钳制到合法范围
     */
    setComposerHeightPx(px: number): void {
      this.composerHeightPx = clampComposerHeight(px)
      this.persist()
    },
    /**
     * 设置输入框展开/收起状态并持久化。
     * @param v - 是否展开
     */
    setComposerExpanded(v: boolean): void {
      this.composerExpanded = v
      this.persist()
    },
  },
})
