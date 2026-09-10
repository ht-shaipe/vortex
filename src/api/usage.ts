/**
 * 用量统计 API。
 *
 * 职责：封装与后端 `/usage` 端点交互的接口，提供用量统计汇总与最近用量明细查询。
 */
import api from './client'

/** 用量统计数据结构 */
export interface UsageStats {
  totalRequests: number // 总请求数
  totalTokensInput: number // 总输入 Token 数
  totalTokensOutput: number // 总输出 Token 数
  totalCost: number // 总花费
  avgLatencyMs: number // 平均延迟（毫秒）
  successRate: number // 成功率
  byProvider: Record<string, number> // 按提供商分组的请求数
  byModel: Record<string, number> // 按模型分组的请求数
}

/**
 * 获取用量统计汇总。
 * @param since - 可选起始时间，仅统计该时间之后的用量
 * @returns 用量统计汇总数据
 */
export async function getUsageStats(since?: string) {
  const params = since ? { since } : {} // 有起始时间则附带查询参数
  const { data } = await api.get('/usage/stats', { params })
  return data as UsageStats
}

/**
 * 获取最近的用量明细记录。
 * @param limit - 返回记录上限，默认 100 条
 * @returns 最近用量明细列表
 */
export async function getRecentUsage(limit = 100) {
  const { data } = await api.get('/usage', { params: { limit } })
  return data
}
