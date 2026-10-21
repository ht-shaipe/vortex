/**
 * 延迟统计 API。
 *
 * 对应后端 `/api/latency-stats`：基于成功请求的 `latency_ms` / `ttft_ms`
 * 计算分位数，并按提供商汇总平均延迟。
 */
import api from './client'

/** 延迟分位数汇总（毫秒） */
export interface LatencySummary {
  p50: number
  p95: number
  p99: number
  avg: number
  min: number
  max: number
  /** 样本数 */
  count: number
}

/** 首 Token 延迟（TTFT）分位数汇总（毫秒） */
export interface TtftSummary {
  p50: number
  p95: number
  avg: number
}

/** 单个提供商的平均延迟 */
export interface ProviderLatency {
  provider: string
  p50: number
  p95: number
  avg: number
  count: number
}

/** 延迟统计响应 */
export interface LatencyStatsResponse {
  latency: LatencySummary
  ttft: TtftSummary
  byProvider: ProviderLatency[]
}

export const latencyStatsApi = {
  /** 查询延迟统计；`since` 为 RFC3339 起始时间，缺省统计全部历史 */
  get: (since?: string) =>
    api.get('/latency-stats', { params: since ? { since } : {} }).then(r => r.data as LatencyStatsResponse),
}
