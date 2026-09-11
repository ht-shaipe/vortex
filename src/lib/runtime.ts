/**
 * 运行时环境检测。
 * 通过多重检测判断当前是否运行在 Tauri 桌面环境中。
 */

/** 运行时环境接口。 */
export interface Runtime {
  /** 环境类型：desktop（Tauri 桌面应用）或 web（浏览器） */
  kind: 'desktop' | 'web'
}

/**
 * 检测当前运行环境类型。
 * 多重检测：Tauri 内部对象、Tauri 协议、用户代理。
 * @returns 'desktop' 或 'web'
 */
function detectKind(): Runtime['kind'] {
  // 检测 Tauri 注入的内部对象（最可靠）
  if ('__TAURI_INTERNALS__' in window) return 'desktop'
  if ('__TAURI__' in window) return 'desktop'
  // 检测 Tauri 协议
  if (window.location.protocol === 'tauri://') return 'desktop'
  // 用户代理检测（兜底）
  if (/Tauri/i.test(navigator.userAgent)) return 'desktop'
  return 'web'
}

/** 全局运行时环境单例。 */
export const runtime: Runtime = { kind: detectKind() }
