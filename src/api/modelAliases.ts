/**
 * 模型别名（虚拟模型映射）API 适配层。
 */
import api from './client'

export interface ModelAliasTarget {
  provider: string
  model: string
  connection_id?: string
}

export interface ModelAlias {
  id: string
  alias: string
  targets: ModelAliasTarget[]
  is_active: boolean
  created_at: string
  updated_at: string
}

export interface CreateModelAlias {
  alias: string
  targets: ModelAliasTarget[]
  is_active: boolean
}

export interface UpdateModelAlias {
  alias?: string
  targets?: ModelAliasTarget[]
  is_active?: boolean
}

/** 列出所有模型别名。 */
export async function listAliases(): Promise<ModelAlias[]> {
  const { data } = await api.get('/model-aliases')
  return data.aliases ?? []
}

/** 创建模型别名。 */
export async function createAlias(req: CreateModelAlias): Promise<ModelAlias> {
  const { data } = await api.post('/model-aliases', req)
  return data
}

/** 更新模型别名。 */
export async function updateAlias(id: string, req: UpdateModelAlias): Promise<ModelAlias> {
  const { data } = await api.patch(`/model-aliases/${id}`, req)
  return data
}

/** 删除模型别名。 */
export async function deleteAlias(id: string): Promise<void> {
  await api.delete(`/model-aliases/${id}`)
}
