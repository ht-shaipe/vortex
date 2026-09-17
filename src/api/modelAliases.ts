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
  source: string
  created_at: string
  updated_at: string
  sort_order?: number
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
  sort_order?: number
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

/** 自动归纳：按模型家族分组生成虚拟别名。 */
export async function autoGenerate(): Promise<{ total: number }> {
  const { data } = await api.post('/model-aliases/auto-generate')
  return data
}
/** 批量重排序模型别名。 */
export async function reorderAliases(orders: { id: string; sort_order: number }[]): Promise<void> {
  await api.post('/model-aliases/reorder', { orders })
}
