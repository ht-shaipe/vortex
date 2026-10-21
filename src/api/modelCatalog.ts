/**
 * 模型目录 API。
 *
 * 对应后端 `/api/model-catalog`：展示远程同步的模型元数据（上下文窗口、
 * 速率限制、能力标签等），并支持从已配置连接实时拉取 `/v1/models`。
 */
import api from './client'

/** 模型目录条目 */
export interface CatalogEntry {
  id: string
  provider: string
  model: string
  name?: string
  /** 上下文窗口（Token） */
  contextWindow?: number
  rpm?: number
  rpd?: number
  tpm?: number
  tpd?: number
  /** 免费额度说明 */
  freeQuota?: string
  supportsTools: boolean
  supportsStreaming: boolean
  supportsVision: boolean
  capabilities: any[]
  /** 来源 feed 标识 */
  sourceFeed?: string
  isActive: boolean
  lastSynced?: string
  createdAt: string
  updatedAt: string
}

/** 单个连接刷新结果 */
export interface ConnectionRefreshResult {
  connectionName: string
  provider: string
  modelsUrl: string
  modelCount: number
  /** 刷新失败时的错误信息 */
  error?: string
}

export const modelCatalogApi = {
  /** 列出模型目录 */
  list: () => api.get('/model-catalog').then(r => r.data.entries as CatalogEntry[]),
  /** 从已配置的活跃连接自动拉取 /v1/models 端点 */
  refreshFromConnections: () => api.post('/model-catalog/refresh-from-connections').then(r => r.data as {
    successConnections: number
    totalModels: number
    results: ConnectionRefreshResult[]
  }),
}
