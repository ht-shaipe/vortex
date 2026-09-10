/**
 * 通知 API。
 *
 * 职责：从远程通知中心（hub）拉取站内通知列表，供前端通知中心展示。
 */
import axios from 'axios'

/** 单条通知项数据结构 */
export interface NotificationItem {
  id: string // 通知唯一标识
  title: string // 通知标题
  body: string // 通知正文
  type: 'info' | 'success' | 'warning' // 通知类型：信息 / 成功 / 警告
  url?: string // 可选跳转链接
  createdAt: string // 创建时间
}

/** 通知列表响应数据结构 */
export interface NotificationResponse {
  notifications: NotificationItem[] // 通知数组
}

// 通知中心 API 基础地址
const HUB_BASE = 'https://hub.htui.cc/api'

// 创建专用于通知中心的 axios 实例（独立超时配置）
const hubClient = axios.create({
  baseURL: HUB_BASE,
  timeout: 15000, // 通知请求超时时间：15 秒
  headers: { 'Content-Type': 'application/json' },
})

/**
 * 拉取通知列表。
 * @param since - 可选起始时间，仅拉取该时间之后的通知
 * @returns 通知项数组，拉取失败时返回空数组
 */
export async function fetchNotifications(since?: string): Promise<NotificationItem[]> {
  const params: Record<string, string> = {}
  if (since) params.since = since // 附带增量拉取的时间参数
  const { data } = await hubClient.get<NotificationResponse>('/notifications', { params })
  return data.notifications ?? [] // 兜底返回空数组
}
