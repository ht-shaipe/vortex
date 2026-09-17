import api from './client'

export interface CatalogEntry {
  id: string
  provider: string
  model: string
  name?: string
  contextWindow?: number
  rpm?: number
  rpd?: number
  tpm?: number
  tpd?: number
  freeQuota?: string
  supportsTools: boolean
  supportsStreaming: boolean
  supportsVision: boolean
  capabilities: any[]
  sourceFeed?: string
  isActive: boolean
  lastSynced?: string
  createdAt: string
  updatedAt: string
}

export interface ConnectionRefreshResult {
  connectionName: string
  provider: string
  modelsUrl: string
  modelCount: number
  error?: string
}

export const modelCatalogApi = {
  list: () => api.get('/model-catalog').then(r => r.data.entries as CatalogEntry[]),
  /** 同步：导入内置快照 +（可选）拉取远程 feed；失败时返回 warning 说明 */
  sync: () => api.post('/model-catalog/sync').then(r => r.data as { synced: number; warning?: string }),
  /** 从已配置的活跃连接自动拉取 /v1/models 端点 */
  refreshFromConnections: () => api.post('/model-catalog/refresh-from-connections').then(r => r.data as {
    successConnections: number
    totalModels: number
    results: ConnectionRefreshResult[]
  }),
}
