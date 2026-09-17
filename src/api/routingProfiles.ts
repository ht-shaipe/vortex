import api from './client'

export interface ProfileTarget {
  provider: string
  model: string
  connectionId?: string
}

export interface RoutingProfile {
  id: string
  name: string
  description?: string
  targets: ProfileTarget[]
  isActive: boolean
  createdAt: string
  updatedAt: string
}

export interface CreateRoutingProfile {
  name: string
  description?: string
  targets: ProfileTarget[]
  isActive?: boolean
}

export const routingProfilesApi = {
  list: () => api.get('/routing-profiles').then(r => r.data.profiles as RoutingProfile[]),
  create: (data: CreateRoutingProfile) => api.post('/routing-profiles', data).then(r => r.data as RoutingProfile),
  delete: (id: string) => api.delete(`/routing-profiles/${id}`),
}
