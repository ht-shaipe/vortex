import api from './client'

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
  source: string
  createdAt: string
  updatedAt: string
}

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
  list: () => api.get('/rate-limits').then(r => r.data.caps as RateLimitCap[]),
  upsert: (data: UpsertRateLimitCap) => api.post('/rate-limits', data).then(r => r.data as RateLimitCap),
  delete: (id: string) => api.delete(`/rate-limits/${id}`),
  usage: () => api.get('/rate-limits/usage').then(r => r.data.snapshots as RateUsageSnapshot[]),
  cleanup: () => api.post('/rate-limits/cleanup').then(r => r.data.deleted as number),
}
