/**
 * 速率限制 API。
 *
 * 对应后端 `/api/rate-limits`：管理 provider/model 维度的 RPM/RPD/TPM/TPD
 * 上限，并查看当前滑动窗口用量。
 */
import api from './client'

/** 速率上限配置 */
export interface RateLimitCap {
  id: string
  provider: string
  model: string
  connectionId?: string
  rpm?: number
  rpd?: number
  tpm?: number
  tpd?: number
  isActive: boolean
  /** 配置来源（manual / catalog 等） */
  source: string
  createdAt: string
  updatedAt: string
}

/** 新增或更新速率上限的请求体 */
export interface UpsertRateLimitCap {
  provider: string
  model: string
  connectionId?: string
  rpm?: number
  rpd?: number
  tpm?: number
  tpd?: number
  isActive?: boolean
}

/** 当前窗口用量与上限快照 */
export interface RateUsageSnapshot {
  provider: string
  model: string
  connectionId?: string
  currentRpm: number
  currentRpd: number
  currentTpm: number
  currentTpd: number
  capRpm?: number
  capRpd?: number
  capTpm?: number
  capTpd?: number
}

export const rateLimitsApi = {
  /** 列出全部速率上限 */
  list: () => api.get('/rate-limits').then(r => r.data.caps as RateLimitCap[]),
  /** 新增或更新速率上限 */
  upsert: (data: UpsertRateLimitCap) => api.post('/rate-limits', data).then(r => r.data as RateLimitCap),
  /** 删除速率上限 */
  delete: (id: string) => api.delete(`/rate-limits/${id}`),
  /** 查看当前窗口用量快照 */
  usage: () => api.get('/rate-limits/usage').then(r => r.data.snapshots as RateUsageSnapshot[]),
  /** 清理过期用量记录，返回删除条数 */
  cleanup: () => api.post('/rate-limits/cleanup').then(r => r.data.deleted as number),
}
