import api from './client'

export interface KeyPermission {
  id: string
  apiKeyId: string
  ruleType: string
  modelPattern: string
  createdAt: string
}

export const keyPermissionsApi = {
  list: () =>
    api.get('/key-permissions').then(r => r.data.rules as KeyPermission[]),
  create: (apiKeyId: string, ruleType: string, modelPattern: string) =>
    api.post('/key-permissions', { apiKeyId, ruleType, modelPattern }).then(r => r.data.rule as KeyPermission),
  delete: (id: string) =>
    api.delete(`/key-permissions/${id}`),
}
