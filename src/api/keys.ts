import api from './client'

export interface ApiKey {
  id: string
  name: string
  key: string
  isActive: boolean
  isBanned: boolean
  allowedModels: unknown
  allowedCombos: unknown
  rateLimits: unknown
  usageLimits: unknown
  createdAt: string
}

export async function listKeys() {
  const { data } = await api.get('/keys')
  return data as { keys: ApiKey[] }
}

export async function createKey(params: { name: string; allowedModels?: unknown; allowedCombos?: unknown }) {
  const { data } = await api.post('/keys', params)
  return data as ApiKey
}

export async function deleteKey(id: string) {
  const { data } = await api.delete(`/keys/${id}`)
  return data as { success: boolean }
}
