/**
 * 路由配置（命名回退链）API。
 *
 * 对应后端 `/api/routing-profiles`：一组有序目标，请求失败时按顺序回退。
 */
import api from './client'

/** 单个回退目标 */
export interface ProfileTarget {
  provider: string
  model: string
  connectionId?: string
}

/** 路由配置 */
export interface RoutingProfile {
  id: string
  /** 配置名称（请求中通过该名称引用） */
  name: string
  description?: string
  /** 有序回退目标列表 */
  targets: ProfileTarget[]
  isActive: boolean
  createdAt: string
  updatedAt: string
}

/** 创建路由配置请求体 */
export interface CreateRoutingProfile {
  name: string
  description?: string
  targets: ProfileTarget[]
  /** 缺省为启用 */
  isActive?: boolean
}

export const routingProfilesApi = {
  /** 列出全部路由配置 */
  list: () => api.get('/routing-profiles').then(r => r.data.profiles as RoutingProfile[]),
  /** 创建路由配置 */
  create: (data: CreateRoutingProfile) => api.post('/routing-profiles', data).then(r => r.data as RoutingProfile),
  /** 删除路由配置 */
  delete: (id: string) => api.delete(`/routing-profiles/${id}`),
}
