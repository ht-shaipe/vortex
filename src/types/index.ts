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

export interface ApiKey {
  id: string
  name: string
  key: string
  isActive: boolean
  isBanned: boolean
  createdAt: string
}
