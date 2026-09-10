/**
 * 免费 Token API。
 *
 * 职责：封装与后端 `/free-tokens` 端点交互的接口，提供免费 Token 站点的
 * 列表查询、用户推荐提交与删除功能。
 */
import api from './client'

/** 站点所在区域：国内 / 海外 / 本地部署 */
export type SiteRegion = 'cn' | 'global' | 'local'

/** 站点来源：内置精选 / 用户推荐 */
export type SiteSource = 'builtin' | 'user'

/** 免费 Token 站点数据结构 */
export interface FreeTokenSite {
  id: string // 站点唯一标识
  name: string // 站点名称
  homeUrl: string // 站点主页地址
  applyUrl: string // 申请地址
  apiSupported: boolean // 是否支持 API 调用
  apiBase: string | null // API 基础地址
  apiFormat: string | null // API 协议格式
  freeQuota: string // 免费额度描述
  region: SiteRegion | string // 所在区域
  requiresCard: boolean // 是否需要绑定信用卡
  requiresVerify: boolean // 是否需要实名验证
  tags: string[] // 标签列表
  note: string | null // 备注说明
  providerId: string | null // 关联的提供商 ID
  source: SiteSource | string // 站点来源
  submitter: string | null // 提交者
  sortOrder: number // 排序权重
  createdAt: string // 创建时间
  updatedAt: string // 更新时间
}

/** 提交免费 Token 站点的参数 */
export interface SubmitSiteParams {
  name: string // 站点名称
  homeUrl?: string // 站点主页地址
  applyUrl?: string // 申请地址
  apiSupported?: boolean // 是否支持 API 调用
  apiBase?: string // API 基础地址
  apiFormat?: string // API 协议格式
  freeQuota?: string // 免费额度描述
  region?: string // 所在区域
  requiresCard?: boolean // 是否需要绑定信用卡
  requiresVerify?: boolean // 是否需要实名验证
  tags?: string[] // 标签列表
  note?: string // 备注说明
  providerId?: string // 关联的提供商 ID
  submitter?: string // 提交者
}

/**
 * 获取所有免费 Token 站点列表。
 * @returns 包含站点数组的对象
 */
export async function listFreeTokenSites() {
  const { data } = await api.get('/free-tokens')
  return data as { sites: FreeTokenSite[] }
}

/**
 * 提交一个新的免费 Token 站点（用户推荐）。
 * @param params - 站点信息参数
 * @returns 新创建的站点对象
 */
export async function submitFreeTokenSite(params: SubmitSiteParams) {
  const { data } = await api.post('/free-tokens', params)
  return data as FreeTokenSite
}

/**
 * 删除指定 ID 的免费 Token 站点。
 * @param id - 站点唯一标识
 * @returns 包含操作是否成功的对象
 */
export async function deleteFreeTokenSite(id: string) {
  const { data } = await api.delete(`/free-tokens/${id}`)
  return data as { success: boolean }
}
