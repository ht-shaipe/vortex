import api from './client'

export interface CompressedContentEntry {
  hash: string
  contentSize: number
  savedTokens: number
  provider: string | null
  model: string | null
  createdAt: string
  expiresAt: string | null
}

export interface CompressedContentDetail extends CompressedContentEntry {
  originalContent: string
}

export const compressedContentApi = {
  list: () =>
    api.get('/compressed-content').then(r => r.data.entries as CompressedContentEntry[]),
  get: (hash: string) =>
    api.get(`/compressed-content/${hash}`).then(r => r.data.entry as CompressedContentDetail),
}
