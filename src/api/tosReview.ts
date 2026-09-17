import api from './client'

export interface TosReview {
  id: string
  provider: string
  verdict: string
  notes?: string
  reviewDate?: string
  sourceUrl?: string
  createdAt: string
  updatedAt: string
}

export const tosReviewApi = {
  list: () => api.get('/tos-reviews').then(r => r.data.reviews as TosReview[]),
}
