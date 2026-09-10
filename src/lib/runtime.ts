/**
 * 运行时环境检测。
 * 通过判断 window 上是否存在 Tauri 内部对象来区分桌面环境与 Web 环境。
 */

/** 运行时环境接口。 */
export interface Runtime {
  /** 环境类型：desktop（Tauri 桌面应用）或 web（浏览器） */
  kind: 'desktop' | 'web'
}

/**
 * 检测当前运行环境类型。
 * 通过 '__TAURI_INTERNALS__' 是否存在于 window 上来判断是否为桌面环境。
 * @returns 'desktop' 或 'web'
 */
function detectKind(): Runtime['kind'] {
  return '__TAURI_INTERNALS__' in window ? 'desktop' : 'web'
}

/** 全局运行时环境单例。 */
export const runtime: Runtime = { kind: detectKind() }
