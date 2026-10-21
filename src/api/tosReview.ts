/**
 * ToS 合规审查 API。
 *
 * 对应后端 `/api/tos-reviews`：只读展示各提供商的服务条款审查结论。
 */
import api from './client'

/** 一条 ToS 审查记录 */
export interface TosReview {
  id: string
  provider: string
  /** 审查结论（allowed / restricted / forbidden 等） */
  verdict: string
  notes?: string
  /** 审查日期 */
  reviewDate?: string
  /** 结论来源链接 */
  sourceUrl?: string
  createdAt: string
  updatedAt: string
}

export const tosReviewApi = {
  /** 列出全部审查记录 */
  list: () => api.get('/tos-reviews').then(r => r.data.reviews as TosReview[]),
}
