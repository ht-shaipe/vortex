/**
 * 同步适配层（对齐 ccMesh 的 webdav / backup / ccSwitch 三个 service 的数据形状）。
 *
 * 背景差异：ccMesh 的备份与 cc-switch 迁移全部走 Tauri command（读本地 SQLite），
 * vortex 后端目前没有对应接口。按既定移植范围：
 * - **本机可直接完成**的能力（配置导出/导入、cc-switch 文件预览）在前端真实实现；
 * - **必须后端参与**的能力（WebDAV 连通性测试、云端备份列表/上传/恢复）
 *   保留 ccMesh 的数据形状与调用签名，但抛出明确 TODO 错误，UI 降级提示。
 *
 * 后端补齐后，只需替换本文件里 `throwTodo(...)` 的实现，组件层无需改动。
 */
import { listKeys } from './keys'
import { listProviders, createProvider, updateProvider, type ProviderConnection } from './providers'
import { getSettings } from './settings'

/* ============ 通用 ============ */

function throwTodo(what: string): never {
  throw new Error(`后端未实现：${what}（vortex 暂无对应接口）`)
}

export const errMsg = (e: unknown): string => (e instanceof Error ? e.message : String(e))

/* ============ WebDAV 配置 ============ */

export interface WebDavConfig {
  url: string
  username: string
  password: string
  configPath: string
  statsPath: string
}

const WEBDAV_KEY = 'vortex-webdav-v1'

const EMPTY_WEBDAV: WebDavConfig = {
  url: '',
  username: '',
  password: '',
  configPath: '/vortex',
  statsPath: '/vortex/stats',
}

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

export interface BackupFile {
  filename: string
  size: number
  modTime: string
}

/* ============ 本地配置导出 / 导入 ============ */

export type ImportStrategy = 'skip' | 'overwrite'

export interface ImportSummary {
  endpointsAdded: number
  endpointsUpdated: number
  endpointsSkipped: number
  settingsKeys: number
}

export interface VortexBackup {
  version: 1
  app: 'vortex'
  exportedAt: string
  endpoints: ProviderConnection[]
  keys: { id: string; name: string; isActive: boolean }[]
  settings: { general: Record<string, unknown> } | null
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

export const backupApi = {
  /** 导出端点（连接）+ 密钥名 + 设置到 JSON，浏览器下载。 */
  async exportConfig(): Promise<string> {
    const [providersRes, keysRes] = await Promise.all([listProviders(), listKeys()])
    const settings = await getSettings().catch(() => null)

    const bundle: VortexBackup = {
      version: 1,
      app: 'vortex',
      exportedAt: new Date().toISOString(),
      endpoints: providersRes.connections ?? [],
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
      const hit = existing.find((c) => c.provider === ep.provider && c.name === ep.name)
      if (hit) {
        if (strategy === 'overwrite') {
          await updateProvider(hit.id, {
            name: ep.name,
            apiKey: ep.apiKey,
            authType: ep.authType,
            priority: ep.priority,
            defaultModel: ep.defaultModel,
          })
          summary.endpointsUpdated += 1
        } else {
          summary.endpointsSkipped += 1
        }
        continue
      }
      await createProvider({
        provider: ep.provider,
        name: ep.name,
        apiKey: ep.apiKey,
        authType: ep.authType,
        priority: ep.priority,
        defaultModel: ep.defaultModel,
      })
      summary.endpointsAdded += 1
    }

    summary.settingsKeys = Object.keys(bundle.settings?.general ?? {}).length
    return summary
  },
}

/* ============ cc-switch 配置迁移 ============ */

export type PreviewStatus = 'ok' | 'skipped'

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

const APP_TO_PROVIDER: Record<string, string> = {
  claude: 'anthropic',
  anthropic: 'anthropic',
  codex: 'openai',
  openai: 'openai',
  gemini: 'gemini',
  google: 'gemini',
}

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

export interface CcSwitchImportSummary {
  imported: number
  skipped: number
}

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
