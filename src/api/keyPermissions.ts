/**
 * API Key 模型访问权限 API。
 *
 * 对应后端 `/api/key-permissions`：为单个 API Key 配置 allow/deny 规则，
 * 支持 `*` 通配（如 `gpt-*`）。规则按创建时间倒序匹配，首个命中生效。
 */
import api from './client'

/** 权限规则 */
export interface KeyPermission {
  id: string
  /** 规则所属的 API Key ID */
  apiKeyId: string
  /** `allow` 或 `deny` */
  ruleType: string
  /** 模型匹配模式，支持 `*` 通配 */
  modelPattern: string
  createdAt: string
}

export const keyPermissionsApi = {
  /** 列出所有权限规则 */
  list: () =>
    api.get('/key-permissions').then(r => r.data.rules as KeyPermission[]),
  /** 创建权限规则 */
  create: (apiKeyId: string, ruleType: string, modelPattern: string) =>
    api.post('/key-permissions', { apiKeyId, ruleType, modelPattern }).then(r => r.data.rule as KeyPermission),
  /** 删除权限规则 */
  delete: (id: string) =>
    api.delete(`/key-permissions/${id}`),
}
