/**
 * 全局 TypeScript 类型定义。
 * 定义 AI 提供商、连接、API 密钥等核心业务实体的接口类型。
 */

/** 提供商定义（描述一类 AI 服务提供商的元信息）。 */
export interface ProviderDef {
  /** 提供商唯一标识 */
  id: string
  /** 提供商名称 */
  name: string
  /** 图标标识 */
  icon: string
  /** 主题色 */
  color: string
  /** 是否提供免费额度 */
  hasFree: boolean
  /** 是否无需鉴权 */
  noAuth: boolean
  /** 鉴权提示文案 */
  authHint?: string
  /** 支持的服务类型列表 */
  serviceKinds: string[]
  /** 总连接数 */
  connections: number
  /** 活跃连接数 */
  activeConnections: number
}

/** 提供商模型定义。 */
export interface ProviderModel {
  /** 实际发送给上游 API 的模型名 */
  id: string
  /** 用户自定义的展示名称（可选；留空时显示 id） */
  name?: string
}

/** 提供商连接（一个已配置的具体连接实例）。 */
export interface ProviderConnection {
  /** 连接唯一标识 */
  id: string
  /** 所属提供商 ID */
  provider: string
  /** 鉴权类型 */
  authType: string
  /** 连接名称 */
  name: string
  /** 关联邮箱 */
  email?: string
  /** API 密钥 */
  apiKey?: string
  /** 项目 ID */
  projectId?: string
  /** 是否活跃 */
  isActive: boolean
  /** 连接测试状态 */
  testStatus: string
  /** 错误码 */
  errorCode?: string
  /** 最近错误信息 */
  lastError?: string
  /** 最近错误时间 */
  lastErrorAt?: string
  /** 退避等级（用于限流保护） */
  backoffLevel: number
  /** 限流截止时间 */
  rateLimitedUntil?: string
  /** 健康检查间隔（秒） */
  healthCheckInterval: number
  /** 连续使用计数 */
  consecutiveUseCount: number
  /** 是否启用限流保护 */
  rateLimitProtection: boolean
  /** 分组名称 */
  groupName?: string
  /** 最大并发数 */
  maxConcurrent?: number
  /** 是否启用代理 */
  proxyEnabled: boolean
  /** 优先级 */
  priority: number
  /** 默认模型 */
  defaultModel?: string
  /** 该连接下挂载的模型列表（含自定义展示名）；为空时回退到 defaultModel */
  models?: ProviderModel[]
  /** 展示名称 */
  displayName?: string
  /** 自定义基础 URL */
  baseUrl?: string
  /** API 协议 */
  apiProtocol?: string
  /** 自定义提供商 ID */
  customProviderId?: string
  /** 提供商特有数据 */
  providerSpecificData?: Record<string, unknown>
  /** 创建时间 */
  createdAt: string
  /** 更新时间 */
  updatedAt: string
}

/** API 密钥（用于客户端访问代理服务的密钥）。 */
export interface ApiKey {
  /** 密钥唯一标识 */
  id: string
  /** 密钥名称 */
  name: string
  /** 密钥值 */
  key: string
  /** 是否活跃 */
  isActive: boolean
  /** 是否被封禁 */
  isBanned: boolean
  /** 创建时间 */
  createdAt: string
}
