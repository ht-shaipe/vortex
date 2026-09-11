/**
 * 提供商 CRUD API。
 *
 * 职责：封装与后端 `/providers` 端点交互的接口，提供 AI 提供商及其连接的
 * 列表查询、创建、读取、更新、删除、连通性测试与模型预览功能。
 */
import api from './client'

/** 提供商定义（描述某类提供商的元信息） */
export interface ProviderDef {
  id: string // 提供商唯一标识
  name: string // 提供商名称
  icon: string // 图标标识
  color: string // 主题色
  hasFree: boolean // 是否有免费额度
  noAuth: boolean // 是否无需鉴权
  authHint?: string // 鉴权提示文案
  serviceKinds: string[] // 支持的服务类型列表
  connections: number // 连接总数
  activeConnections: number // 活跃连接数
}

/** 提供商连接（一个提供商可配置多个连接实例） */
export interface ProviderConnection {
  id: string // 连接唯一标识
  provider: string // 提供商类型标识
  authType: string // 鉴权方式
  name: string // 连接名称
  email?: string // 关联邮箱
  /** API 密钥（已脱敏，仅末尾 4 位可见）。判断「是否配置」请用 hasApiKey，不要用本字段是否为空 */
  apiKey?: string
  /** 是否已配置 API 密钥。与 apiKey 掩码解耦，避免把「未配置」误判成「已配置」 */
  hasApiKey?: boolean
  /** 是否已配置 OAuth access token（OAuth 连接没有 apiKey，但凭据有效） */
  hasAccessToken?: boolean
  projectId?: string // 项目 ID
  isActive: boolean // 是否启用
  testStatus: string // 最近测试状态
  lastTestedAt?: string // 最近一次测试时间
  /** 最近一次测试的响应延迟（毫秒）。列表首列展示，未测试时为 undefined */
  lastLatencyMs?: number
  errorCode?: string // 错误码
  lastError?: string // 最近错误信息
  lastErrorAt?: string // 最近错误时间
  backoffLevel: number // 退避等级（用于限流恢复）
  rateLimitedUntil?: string // 限流截止时间
  healthCheckInterval: number // 健康检查间隔
  consecutiveUseCount: number // 连续使用计数
  rateLimitProtection: boolean // 是否启用限流保护
  groupName?: string // 分组名称
  maxConcurrent?: number // 最大并发数
  proxyEnabled: boolean // 是否启用代理
  priority: number // 优先级权重
  defaultModel?: string // 默认模型
  models?: ProviderModel[] // 挂载的模型列表
  displayName?: string // 展示名称
  baseUrl?: string // 自定义基础地址
  apiProtocol?: string // API 协议
  customProviderId?: string // 自定义提供商 ID
  providerSpecificData?: Record<string, unknown> // 提供商专属扩展数据
  createdAt: string // 创建时间
  updatedAt: string // 更新时间
}

/** 提供商挂载的模型定义 */
export interface ProviderModel {
  /** 实际发送给上游 API 的模型名 */
  id: string
  /** 用户自定义的展示名称（可选；留空时显示 id） */
  name?: string
}

/**
 * 列出所有提供商定义及其连接。
 * @returns 包含提供商定义数组和连接数组的对象
 */
export async function listProviders() {
  const { data } = await api.get('/providers')
  return data as { providers: ProviderDef[]; connections: ProviderConnection[] }
}

/**
 * 创建新的提供商连接。
 * @param params - 创建参数，包含提供商类型、名称、密钥等配置
 * @returns 新创建的连接对象
 */
export async function createProvider(params: {
  provider: string
  name: string
  apiKey?: string
  authType?: string
  baseUrl?: string
  priority?: number
  defaultModel?: string
  /** 挂载的模型列表（含自定义展示名）；为空时回退到 defaultModel */
  models?: { id: string; name?: string }[]
  groupName?: string
  email?: string
  projectId?: string
  maxConcurrent?: number
  rateLimitProtection?: boolean
  proxyEnabled?: boolean
  healthCheckInterval?: number
  displayName?: string
  apiProtocol?: string
  customProviderId?: string
}) {
  const { data } = await api.post('/providers', params)
  return data as ProviderConnection
}

/**
 * 获取指定 ID 的提供商连接详情。
 * @param id - 连接唯一标识
 * @returns 连接对象
 */
export async function getProvider(id: string) {
  const { data } = await api.get(`/providers/${id}`)
  return data as ProviderConnection
}

/**
 * 更新指定 ID 的提供商连接。
 * @param id - 连接唯一标识
 * @param updates - 需要更新的字段键值对
 * @returns 更新后的连接对象
 */
export async function updateProvider(id: string, updates: Record<string, unknown>) {
  const { data } = await api.patch(`/providers/${id}`, updates)
  return data as ProviderConnection
}

/**
 * 删除指定 ID 的提供商连接。
 * @param id - 连接唯一标识
 * @returns 包含操作是否成功的对象
 */
export async function deleteProvider(id: string) {
  const { data } = await api.delete(`/providers/${id}`)
  return data as { success: boolean }
}

/**
 * 测试指定提供商连接的连通性。
 * @param id - 连接唯一标识
 * @returns 包含状态、错误信息与延迟毫秒数的对象
 */
export async function testProvider(id: string) {
  const { data } = await api.post(`/providers/${id}/test`)
  return data as { status: string; error?: string; latencyMs?: number }
}

/**
 * 预览指定提供商在给定密钥下可用的模型列表。
 * @param params - 预览参数，包含提供商类型、密钥、基础地址等
 * @returns 包含模型列表与可选警告信息的对象
 */
export async function previewModels(params: {
  provider: string
  apiKey?: string
  baseUrl?: string
  apiProtocol?: string
}) {
  const { data } = await api.post('/providers/preview-models', params)
  return data as { models: string[]; warning?: string }
}
