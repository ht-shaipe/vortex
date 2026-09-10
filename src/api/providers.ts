import api from './client'

export interface ProviderDef {
  id: string
  name: string
  icon: string
  color: string
  hasFree: boolean
  noAuth: boolean
  authHint?: string
  serviceKinds: string[]
  connections: number
  activeConnections: number
}

export interface ProviderConnection {
  id: string
  provider: string
  authType: string
  name: string
  apiKey?: string
  isActive: boolean
  testStatus: string
  priority: number
  defaultModel?: string
  createdAt: string
}

export async function listProviders() {
  const { data } = await api.get('/providers')
  return data as { providers: ProviderDef[]; connections: ProviderConnection[] }
}

export async function createProvider(params: {
  provider: string
  name: string
  apiKey?: string
  authType?: string
  priority?: number
  defaultModel?: string
}) {
  const { data } = await api.post('/providers', params)
  return data as ProviderConnection
}

export async function getProvider(id: string) {
  const { data } = await api.get(`/providers/${id}`)
  return data as ProviderConnection
}

export async function updateProvider(id: string, updates: Record<string, unknown>) {
  const { data } = await api.patch(`/providers/${id}`, updates)
  return data as ProviderConnection
}

export async function deleteProvider(id: string) {
  const { data } = await api.delete(`/providers/${id}`)
  return data as { success: boolean }
}

export async function testProvider(id: string) {
  const { data } = await api.post(`/providers/${id}/test`)
  return data as { status: string; error?: string; latencyMs?: number }
}
