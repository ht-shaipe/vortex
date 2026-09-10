import api from './client'

/** 站点所在区域：国内 / 海外 / 本地部署 */
export type SiteRegion = 'cn' | 'global' | 'local'

/** 站点来源：内置精选 / 用户推荐 */
export type SiteSource = 'builtin' | 'user'

export interface FreeTokenSite {
  id: string
  name: string
  homeUrl: string
  applyUrl: string
  apiSupported: boolean
  apiBase: string | null
  apiFormat: string | null
  freeQuota: string
  region: SiteRegion | string
  requiresCard: boolean
  requiresVerify: boolean
  tags: string[]
  note: string | null
  providerId: string | null
  source: SiteSource | string
  submitter: string | null
  sortOrder: number
  createdAt: string
  updatedAt: string
}

export interface SubmitSiteParams {
  name: string
  homeUrl?: string
  applyUrl?: string
  apiSupported?: boolean
  apiBase?: string
  apiFormat?: string
  freeQuota?: string
  region?: string
  requiresCard?: boolean
  requiresVerify?: boolean
  tags?: string[]
  note?: string
  providerId?: string
  submitter?: string
}

export async function listFreeTokenSites() {
  const { data } = await api.get('/free-tokens')
  return data as { sites: FreeTokenSite[] }
}

export async function submitFreeTokenSite(params: SubmitSiteParams) {
  const { data } = await api.post('/free-tokens', params)
  return data as FreeTokenSite
}

export async function deleteFreeTokenSite(id: string) {
  const { data } = await api.delete(`/free-tokens/${id}`)
  return data as { success: boolean }
}
