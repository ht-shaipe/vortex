import api from './client'

export interface UsageStats {
  totalRequests: number
  totalTokensInput: number
  totalTokensOutput: number
  totalCost: number
  avgLatencyMs: number
  successRate: number
  byProvider: Record<string, number>
  byModel: Record<string, number>
}

export async function getUsageStats(since?: string) {
  const params = since ? { since } : {}
  const { data } = await api.get('/usage/stats', { params })
  return data as UsageStats
}

export async function getRecentUsage(limit = 100) {
  const { data } = await api.get('/usage', { params: { limit } })
  return data
}
