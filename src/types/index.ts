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

export interface ComboStep {
  modelStr: string
  provider: string
  weight?: number
  label?: string
  connectionId?: string
}

export interface ComboData {
  strategy: string
  models: ComboStep[]
  config?: Record<string, unknown>
  systemMessage?: string
}

export interface Combo {
  id: string
  name: string
  data: ComboData
  sortOrder: number
  contextCacheProtection: boolean
  createdAt: string
  updatedAt: string
}

export interface ApiKey {
  id: string
  name: string
  key: string
  isActive: boolean
  isBanned: boolean
  createdAt: string
}
