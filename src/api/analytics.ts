import api from './client'

export interface CostByModel {
  provider: string | null
  model: string | null
  requests: number
  cost: number
  tokensInput: number
  tokensOutput: number
  savedTokens: number
}

export interface CostByProvider {
  provider: string | null
  requests: number
  cost: number
}

export interface CostByDay {
  day: string
  requests: number
  cost: number
  savedTokens: number
}

export interface CostAnalysis {
  totalCost: number
  totalSavedTokens: number
  totalRequests: number
  byModel: CostByModel[]
  byProvider: CostByProvider[]
  byDay: CostByDay[]
}

export interface Recommendation {
  type: string
  severity: string
  message: string
  model?: string
  provider?: string
  count?: number
  cost?: number
  requests?: number
  avgLatencyMs?: number
  savedTokens?: number
}

export const analyticsApi = {
  costAnalysis: (since?: string) =>
    api.get('/cost-analysis', { params: since ? { since } : {} }).then(r => r.data as CostAnalysis),
  recommendations: () =>
    api.get('/recommendations').then(r => r.data as { suggestions: Recommendation[] }),
}
