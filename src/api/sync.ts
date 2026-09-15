/**
 * 同步适配层。
 *
 * 背景：vortex 后端目前没有同步接口。按既定移植范围：
 * - **本机可直接完成**的能力（配置导出/导入、cc-switch 文件预览）在前端真实实现；
 * - **必须后端参与**的能力（WebDAV 连通性测试、云端备份列表/上传/恢复）
 *   保留数据形状与调用签名，但抛出明确 TODO 错误，UI 降级提示。
 *
 * 后端补齐后，只需替换本文件里 `throwTodo(...)` 的实现，组件层无需改动。
 */
import { listKeys } from './keys'
import {
  listProviders,
  createProvider,
  updateProvider,
  getApiKey,
  type ProviderConnection,
} from './providers'
import { getSettings } from './settings'

/* ============ 通用 ============ */

/** 统一抛出"后端未实现"错误。 */
function throwTodo(what: string): never {
  throw new Error(`后端未实现：${what}（vortex 暂无对应接口）`)
}

/** 把任意错误对象转为可读字符串。 */
export const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e))

/* ============ WebDAV 配置 ============ */

/** WebDAV 云同步配置 */
export interface WebDavConfig {
  url: string // WebDAV 服务地址
  username: string // 用户名
  password: string // 密码
  configPath: string // 配置文件存储路径
  statsPath: string // 统计数据存储路径
}

// WebDAV 配置在 localStorage 中的存储键
const WEBDAV_KEY = 'vortex-webdav-v1'

// WebDAV 配置默认空值
const EMPTY_WEBDAV: WebDavConfig = {
  url: '',
  username: '',
  password: '',
  configPath: '/vortex',
  statsPath: '/vortex/stats',
}

/** webdavApi：WebDAV 云同步接口（部分功能待后端实现） */
export const webdavApi = {
  async getConfig(): Promise<WebDavConfig> {
    try {
      const raw = localStorage.getItem(WEBDAV_KEY)
      if (!raw) return { ...EMPTY_WEBDAV }
      return { ...EMPTY_WEBDAV, ...(JSON.parse(raw) as Partial<WebDavConfig>) }
    } catch {
      return { ...EMPTY_WEBDAV }
    }
  },

  /** 仅落本机 localStorage：vortex 后端没有 config 表，等接口补齐后改走 PATCH /settings。 */
  async saveConfig(cfg: WebDavConfig): Promise<void> {
    localStorage.setItem(WEBDAV_KEY, JSON.stringify(cfg))
  },

  async test(_cfg: WebDavConfig): Promise<{ success: boolean; message: string }> {
    throwTodo('WebDAV 连通性测试')
  },

  async listBackups(): Promise<BackupFile[]> {
    throwTodo('云端备份列表')
  },
  async backup(): Promise<string> {
    throwTodo('上传备份到 WebDAV')
  },
  async restore(_filename: string): Promise<void> {
    throwTodo('从 WebDAV 恢复备份')
  },
  async deleteBackup(_filename: string): Promise<void> {
    throwTodo('删除云端备份')
  },
}

/** 云端备份文件信息 */
export interface BackupFile {
  filename: string // 文件名
  size: number // 文件大小（字节）
  modTime: string // 最后修改时间
}

/* ============ 本地配置导出 / 导入 ============ */

/** 导入策略：跳过已存在 / 覆盖已存在 */
export type ImportStrategy = 'skip' | 'overwrite'

/** 导入结果汇总 */
export interface ImportSummary {
  endpointsAdded: number // 新增端点数
  endpointsUpdated: number // 更新端点数
  endpointsSkipped: number // 跳过端点数
  settingsKeys: number // 导入的设置键数
}

/** vortex 配置备份文件结构 */
export interface VortexBackup {
  version: 1 // 备份格式版本
  app: 'vortex' // 应用标识
  exportedAt: string // 导出时间
  endpoints: ProviderConnection[] // 端点（连接）列表
  keys: { id: string; name: string; isActive: boolean }[] // 密钥摘要列表
  settings: { general: Record<string, unknown> } | null // 设置数据
}

/** 弹出文件选择框并解析为文本；用户取消返回 null。 */
function pickTextFile(accept = 'application/json'): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.style.display = 'none'
    document.body.appendChild(input)
    let settled = false
    const done = (v: string | null) => {
      if (settled) return
      settled = true
      document.body.removeChild(input)
      resolve(v)
    }
    input.addEventListener('change', () => {
      const f = input.files?.[0]
      if (!f) return done(null)
      const reader = new FileReader()
      reader.onload = () => done(String(reader.result ?? ''))
      reader.onerror = () => done(null)
      reader.readAsText(f)
    })
    // 部分浏览器取消时不触发 change，这里用窗口聚焦兜底
    window.addEventListener(
      'focus',
      () => window.setTimeout(() => done(null), 500),
      { once: true },
    )
    input.click()
  })
}

/** 触发浏览器下载 JSON 文件。 */
function downloadJson(filename: string, data: unknown): void {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

/**
 * 判断字符串是否像脱敏掩码（如 `***xxxx`、`******`）。
 * 列表接口返回的 apiKey 是掩码，这类值不能写回数据库。
 */
function looksMasked(v?: string): boolean {
  if (!v) return false
  return v.includes('***') || /^\*+$/.test(v)
}

/**
 * 由备份条目构造 provider 创建/更新载荷。
 *
 * 导出文件里的连接是列表接口返回的「平铺 + 密钥脱敏」形态。导入时若只还原
 * name/apiKey/authType/priority/defaultModel，会丢掉自定义提供方赖以工作的
 * baseUrl / chatPath / apiProtocol / customId / models，导致导入后连接回退到
 * 默认端点，测试或调用必然报错。这里把可还原字段一并带上。
 *
 * 另：chatPath 不在 `CreateProviderRequest` 的顶层字段里，创建路径**只能**靠
 * `providerSpecificData` 携带（后端 create 会整体合并该 JSON）。因此这里必须
 * 原样回传 providerSpecificData，否则 chatPath/customId 会在创建时丢失。
 *
 * 仅当字段有值时才写入，避免用备份里的缺省值覆盖本机已有配置。
 */
function buildEndpointPayload(
  ep: ProviderConnection,
  realApiKey: string | undefined,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {
    provider: ep.provider,
    name: ep.name,
  }
  if (ep.authType) payload.authType = ep.authType
  if (ep.priority !== undefined) payload.priority = ep.priority
  if (ep.defaultModel != null) payload.defaultModel = ep.defaultModel
  if (realApiKey !== undefined) payload.apiKey = realApiKey

  // 自定义提供方关键字段：baseUrl / chatPath / 协议 / 自定义 ID / 模型列表
  if (ep.baseUrl) payload.baseUrl = ep.baseUrl
  if (ep.chatPath) payload.chatPath = ep.chatPath
  if (ep.apiProtocol) payload.apiProtocol = ep.apiProtocol
  if (ep.customProviderId) payload.customProviderId = ep.customProviderId
  if (Array.isArray(ep.models) && ep.models.length > 0) payload.models = ep.models
  if (ep.displayName) payload.displayName = ep.displayName

  // 其余可还原的普通字段
  if (ep.groupName) payload.groupName = ep.groupName
  if (ep.email) payload.email = ep.email
  if (ep.projectId) payload.projectId = ep.projectId
  if (ep.maxConcurrent != null) payload.maxConcurrent = ep.maxConcurrent

  // 原样回传扩展数据（内含 chatPath / customId / baseUrl / models 等）
  if (ep.providerSpecificData && Object.keys(ep.providerSpecificData).length > 0) {
    payload.providerSpecificData = ep.providerSpecificData
  }
  return payload
}

/** backupApi：本地配置导出 / 导入接口 */
export const backupApi = {
  /** 导出端点（连接）+ 密钥名 + 设置到 JSON，浏览器下载。 */
  async exportConfig(): Promise<string> {
    const [providersRes, keysRes] = await Promise.all([listProviders(), listKeys()])
    const settings = await getSettings().catch(() => null)

    // 列表接口返回的 apiKey 已是脱敏掩码，导出前用真实密钥接口逐个还原明文，
    // 否则导入时会把 `***xxxx` 这类掩码写回数据库导致连接失效。
    const conns = await Promise.all(
      (providersRes.connections ?? []).map(async (c: ProviderConnection) => {
        if (c.hasApiKey && c.id) {
          try {
            const { apiKey } = await getApiKey(c.id)
            if (apiKey) return { ...c, apiKey }
          } catch {
            /* 取真实密钥失败则保留原掩码值，不阻断导出 */
          }
        }
        return c
      }),
    )

    const bundle: VortexBackup = {
      version: 1,
      app: 'vortex',
      exportedAt: new Date().toISOString(),
      endpoints: conns,
      keys: (keysRes.keys ?? []).map((k) => ({ id: k.id, name: k.name, isActive: k.isActive })),
      settings,
    }
    const name = `vortex-backup-${new Date().toISOString().slice(0, 10)}.json`
    downloadJson(name, bundle)
    return name
  },

  /** 选择 JSON 备份并按策略导入端点。 */
  async importConfig(strategy: ImportStrategy): Promise<ImportSummary | null> {
    const text = await pickTextFile()
    if (text === null) return null
    let bundle: Partial<VortexBackup>
    try {
      bundle = JSON.parse(text) as Partial<VortexBackup>
    } catch {
      throw new Error('不是合法的 JSON 文件')
    }
    if (!Array.isArray(bundle.endpoints)) throw new Error('文件缺少 endpoints 字段')

    const current = await listProviders()
    const existing = current.connections ?? []

    const summary: ImportSummary = {
      endpointsAdded: 0,
      endpointsUpdated: 0,
      endpointsSkipped: 0,
      settingsKeys: 0,
    }

    for (const ep of bundle.endpoints) {
      if (!ep?.provider || !ep?.name) {
        summary.endpointsSkipped += 1
        continue
      }
      // 历史备份里 apiKey 可能是脱敏掩码，写回会污染数据库，跳过该字段
      const realApiKey = looksMasked(ep.apiKey) ? undefined : ep.apiKey
      // 还原全部配置字段（baseUrl / chatPath / apiProtocol / customId / models 等），
      // 否则自定义提供方导入后会退化为默认端点，测试/调用报错。
      const payload = buildEndpointPayload(ep, realApiKey)
      const hit = existing.find((c) => c.provider === ep.provider && c.name === ep.name)
      if (hit) {
        if (strategy === 'overwrite') {
          await updateProvider(hit.id, payload)
          summary.endpointsUpdated += 1
        } else {
          summary.endpointsSkipped += 1
        }
        continue
      }
      await createProvider(payload as Parameters<typeof createProvider>[0])
      summary.endpointsAdded += 1
    }

    summary.settingsKeys = Object.keys(bundle.settings?.general ?? {}).length
    return summary
  },
}

/* ============ cc-switch 配置迁移 ============ */

/** 预览项状态：可导入 / 已跳过 */
export type PreviewStatus = 'ok' | 'skipped'

/** cc-switch 配置预览项 */
export interface PreviewItem {
  /** cc-switch 里的供应商 id，用作勾选键。 */
  ccSwitchId: string
  name: string
  apiUrl?: string
  apiKeyMasked?: string
  apiKey?: string
  /** claude / codex / openai … */
  appType: string
  /** 导入到 vortex 后的 provider kind。 */
  transformer: string
  status: PreviewStatus
  skipReason?: string
}

// 应用类型 → vortex 提供商类型映射
const APP_TO_PROVIDER: Record<string, string> = {
  claude: 'anthropic',
  anthropic: 'anthropic',
  codex: 'openai',
  openai: 'openai',
  gemini: 'gemini',
  google: 'gemini',
}

/** 对 API 密钥做掩码处理（保留首尾各 4 位，中间用 •••• 代替）。 */
function mask(key?: string): string {
  if (!key) return ''
  if (key.length <= 8) return '••••'
  return `${key.slice(0, 4)}••••${key.slice(-4)}`
}

/**
 * 从任意形如 `{ providers: [...] }` / `{ claude: { providers: [] } }` /
 * 顶层数组的 cc-switch 风格 JSON 中归一化出预览项。
 */
function normalizePreview(raw: unknown): PreviewItem[] {
  const buckets: unknown[] = []
  const walk = (node: unknown) => {
    if (Array.isArray(node)) buckets.push(...node)
    else if (node && typeof node === 'object') {
      const o = node as Record<string, unknown>
      if (Array.isArray(o.providers)) buckets.push(...o.providers)
      if (Array.isArray(o.endpoints)) buckets.push(...o.endpoints)
      for (const v of Object.values(o)) {
        if (v && typeof v === 'object' && !Array.isArray(v)) walk(v)
      }
    }
  }
  walk(raw)

  const seen = new Set<string>()
  const out: PreviewItem[] = []
  for (const b of buckets) {
    if (!b || typeof b !== 'object') continue
    const o = b as Record<string, unknown>
    const name = String(o.name ?? o.id ?? o.title ?? '').trim()
    if (!name) continue
    const id = String(o.id ?? name)
    if (seen.has(id)) continue
    seen.add(id)

    const appType = String(o.appType ?? o.app ?? o.type ?? 'openai').toLowerCase()
    const apiUrl = o.apiUrl ?? o.baseUrl ?? o.base_url ?? o.url
    const apiKey = o.apiKey ?? o.api_key ?? o.key
    const keyStr = typeof apiKey === 'string' ? apiKey : ''

    let status: PreviewStatus = 'ok'
    let skipReason: string | undefined
    if (typeof apiUrl !== 'string' || !/^https?:\/\//i.test(apiUrl)) {
      status = 'skipped'
      skipReason = 'no_url'
    } else if (!keyStr) {
      status = 'skipped'
      skipReason = 'no_key'
    } else if (/^oauth/i.test(keyStr) || o.authType === 'oauth') {
      status = 'skipped'
      skipReason = 'oauth_managed'
    }

    out.push({
      ccSwitchId: id,
      name,
      apiUrl: typeof apiUrl === 'string' ? apiUrl : undefined,
      apiKey: keyStr,
      apiKeyMasked: mask(keyStr),
      appType,
      transformer: APP_TO_PROVIDER[appType] ?? 'openai',
      status,
      skipReason,
    })
  }
  return out
}

/** cc-switch 导入结果汇总 */
export interface CcSwitchImportSummary {
  imported: number // 成功导入数
  skipped: number // 跳过数
}

/** ccSwitchApi：cc-switch 配置迁移接口 */
export const ccSwitchApi = {
  /** 选择 cc-switch 导出的 JSON，返回可迁移项预览。 */
  async preview(): Promise<PreviewItem[]> {
    const text = await pickTextFile()
    if (text === null) return []
    try {
      return normalizePreview(JSON.parse(text) as unknown)
    } catch {
      throw new Error('不是合法的 JSON 文件')
    }
  },

  /** 把勾选的项导入为 vortex 的 provider 连接。 */
  async import(items: PreviewItem[]): Promise<CcSwitchImportSummary> {
    const usable = items.filter((i) => i.status === 'ok')
    let imported = 0
    for (const it of usable) {
      try {
        await createProvider({
          provider: it.transformer,
          name: it.name,
          apiKey: it.apiKey,
          authType: 'api-key',
        })
        imported += 1
      } catch {
        /* 单个失败不影响其余 */
      }
    }
    return { imported, skipped: items.length - imported }
  },
}
