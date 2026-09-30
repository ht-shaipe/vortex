/**
 * 网关地址单点配置。
 *
 * 职责：全站唯一的网关 base 地址（scheme://host:port）来源，供 axios 实例、
 * SSE 流、健康检查与「接入信息」展示共用。避免各处硬编码 http://localhost:10168，
 * 使 HTTPS（VORTEX_TLS）模式下前端自动跟随。
 *
 * 优先级：
 * 1. localStorage 覆盖（key: vortex-gateway-base，调试/特殊部署用）
 * 2. 环境变量注入（构建期/启动期通过 window.__VORTEX_GATEWAY_BASE__ 注入，
 *    桌面端 main.ts 启动时经 Tauri 命令 gateway_info 填充）
 * 3. Web 部署模式：页面本身由网关同源服务时，直接用 location.origin
 * 4. 兜底 http://localhost:10168
 */

/** 兜底网关地址（与后端默认端口一致）。 */
const DEFAULT_BASE = 'http://localhost:10168'

/** localStorage 覆盖键名。 */
const OVERRIDE_KEY = 'vortex-gateway-base'

/** 启动期注入的网关地址（main.ts 中赋值）。 */
let injectedBase: string | null = null

/** 是否已完成启动期初始化（避免 Tauri invoke 竞争）。 */
let initialized = false

/** 判断给定字符串是否为合法的 origin（scheme://host[:port]）。 */
function isOrigin(v: string): boolean {
  return /^https?:\/\/[\w.-]+(:\d+)?$/.test(v.trim())
}

/** 启动期注入：桌面端经 Tauri 命令获取，仅调用一次。 */
export async function initGatewayBase(): Promise<void> {
  if (initialized) return
  initialized = true
  try {
    // localStorage 手动覆盖优先
    const local = localStorage.getItem(OVERRIDE_KEY)
    if (local && isOrigin(local)) {
      injectedBase = local.trim()
      return
    }
    // 桌面端：从 Tauri 命令取真实配置（scheme/port）
    if ('__TAURI_INTERNALS__' in window || '__TAURI__' in window) {
      const mod = await import('@tauri-apps/api/core')
      const base = await mod.invoke<string>('gateway_base_url')
      if (base && isOrigin(base)) {
        injectedBase = base.trim()
        return
      }
    }
    // Web 部署：页面与网关同源（vortex-server 静态托管）时跟随页面协议
    if (window.location.protocol.startsWith('http') && window.location.port === '10168') {
      injectedBase = window.location.origin
    }
  } catch {
    /* 任何失败都落到默认值 */
  }
}

/**
 * 获取网关 base 地址（不含路径尾斜杠）。
 * @returns 形如 `http://localhost:10168` 或 `https://localhost:10168`
 */
export function gatewayBase(): string {
  const local = typeof localStorage !== 'undefined' ? localStorage.getItem(OVERRIDE_KEY) : null
  if (local && isOrigin(local)) return local.trim()
  if (injectedBase && isOrigin(injectedBase)) return injectedBase
  if (typeof window !== 'undefined' && window.location.port === '10168') {
    return window.location.origin
  }
  return DEFAULT_BASE
}
