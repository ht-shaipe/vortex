/**
 * 网页版平台（浏览器自动化）授权 API。
 *
 * 封装 `/web-providers` 端点：平台列表、CDP 登录编排
 * （拉起本地 Chrome → 用户登录 → cookie 收集注入）。
 */
import api from './client'

/** CDP 登录会话状态快照（前端轮询展示） */
export interface CdpLoginStatus {
  /** launching / waiting_login / collecting / importing / success / failed / cancelled */
  phase: string
  /** Chrome 进程 PID（launch 后有效） */
  chromePid?: number
  /** 会话已运行的秒数 */
  elapsedSecs: number
  /** 导入成功的 cookie 条数（success 时有效） */
  imported?: number
  /** 导入后的登录态校验结果（success 时有效） */
  loggedIn?: boolean
  /** 浏览器侧是否已检测到登录凭证（可主动触发收集） */
  hasCredentials?: boolean
  /** 人类可读的当前进展 / 错误详情 */
  message: string
}

/** 网页版平台状态（来自 GET /web-providers） */
export interface WebPlatform {
  /** 平台标识（doubao / chatgpt / ...） */
  platform: string
  /** worker 是否已启动（未启动平台发起授权时按需拉起） */
  enabled: boolean
  /** worker 报告的登录态；null 表示未知（worker 忙或查询失败） */
  loggedIn: boolean | null
  /** 进行中或最近结束的 CDP 授权会话状态 */
  cdp?: CdpLoginStatus
}

/**
 * 列出网页版平台与状态（worker 可用性 + 登录态 + CDP 会话）。
 */
export async function listWebProviders() {
  const { data } = await api.get('/web-providers')
  return data as { platforms: WebPlatform[] }
}

/**
 * 开始指定平台的 CDP 授权（拉起本地 Chrome 打开平台登录页）。
 * @param platform - 平台标识（doubao / chatgpt / ...）
 */
export async function startCdpLogin(platform: string) {
  const { data } = await api.post(`/web-providers/${platform}/login/cdp`)
  return data as { platform: string; message: string }
}

/**
 * 查询指定平台 CDP 授权进度（前端轮询）。
 * 无进行中或已结束的会话时后端返回 404 —— 本函数将其归一化为 null。
 * @param platform - 平台标识
 */
export async function cdpLoginStatus(platform: string): Promise<CdpLoginStatus | null> {
  try {
    const { data } = await api.get(`/web-providers/${platform}/login/cdp`)
    return data as CdpLoginStatus
  } catch (e: unknown) {
    if (
      typeof e === 'object' &&
      e !== null &&
      'response' in e &&
      (e as { response?: { status?: number } }).response?.status === 404
    ) {
      return null
    }
    throw e
  }
}

/**
 * 手动触发 cookie 收集（用户确认已在 Chrome 中完成登录）。
 * @param platform - 平台标识
 */
export async function finishCdpLogin(platform: string) {
  const { data } = await api.post(`/web-providers/${platform}/login/cdp/finish`)
  return data as { platform: string; message: string }
}

/**
 * 取消 CDP 授权（关闭 Chrome、终止轮询）。
 * @param platform - 平台标识
 */
export async function cancelCdpLogin(platform: string) {
  const { data } = await api.post(`/web-providers/${platform}/login/cdp/cancel`)
  return data as { success: boolean }
}
