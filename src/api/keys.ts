/**
 * API 密钥管理 API。
 *
 * 职责：封装与后端 `/keys` 端点交互的接口，包括列出、创建、删除 API 密钥。
 */
import api from './client'

/** API 密钥数据结构 */
export interface ApiKey {
  id: string // 密钥唯一标识
  name: string // 密钥名称（用户可读）
  key: string // 密钥实际值
  isActive: boolean // 是否启用
  isBanned: boolean // 是否被封禁
  allowedModels: unknown // 允许访问的模型列表
  rateLimits: unknown // 速率限制配置
  usageLimits: unknown // 用量限制配置
  createdAt: string // 创建时间
}

/**
 * 获取所有 API 密钥列表。
 * @returns 包含密钥数组的对象
 */
export async function listKeys() {
  const { data } = await api.get('/keys')
  return data as { keys: ApiKey[] }
}

/**
 * 创建新的 API 密钥。
 * @param params - 创建参数，包含名称与可选的允许模型配置
 * @returns 新创建的密钥对象
 */
export async function createKey(params: { name: string; allowedModels?: unknown }) {
  const { data } = await api.post('/keys', params)
  return data as ApiKey
}

/**
 * 删除指定 ID 的 API 密钥。
 * @param id - 密钥唯一标识
 * @returns 包含操作是否成功的对象
 */
export async function deleteKey(id: string) {
  const { data } = await api.delete(`/keys/${id}`)
  return data as { success: boolean }
}
