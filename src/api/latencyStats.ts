import api from './client'

export interface LatencySummary {
  p50: number
  p95: number
  p99: number
  avg: number
  min: number
  max: number
  count: number
}

export interface TtftSummary {
  p50: number
  p95: number
  avg: number
}

export interface ProviderLatency {
  provider: string
  p50: number
  p95: number
  avg: number
  count: number
}

export interface LatencyStatsResponse {
  latency: LatencySummary
  ttft: TtftSummary
  byProvider: ProviderLatency[]
}

export const latencyStatsApi = {
  get: (since?: string) =>
    api.get('/latency-stats', { params: since ? { since } : {} }).then(r => r.data as LatencyStatsResponse),
}
