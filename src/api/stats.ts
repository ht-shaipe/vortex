/**
 * 统计适配层（对齐 ccMesh statsApi / usageApi 的数据形状）。
 *
 * 背景：ccMesh 的统计由 Rust 侧 `daily_stats` / `request_logs` 两张聚合表直接供数；
 * vortex 后端目前只有 `usage_history` 明细表（`GET /api/usage`，snake_case JSON）。
 * 因此本模块把明细在前端聚合成 ccMesh 的各类形状，让统计页拿真实数据跑起来，
 * 后端补齐聚合接口后只需替换本文件的实现，组件层无需改动。
 *
 * 注意：后端 JSON 为 snake_case（Rust struct 未加 serde rename_all），
 * 原 `src/api/usage.ts` 里 camelCase 的 `UsageStats` 声明与实际不符，
 * 本模块统一按 snake_case 解析。
 */
import api from './client'
import { listProviders } from './providers'
import { ymd } from '@/lib/range'
import { hourKey } from '@/lib/usageChart'

/** 后端 `usage_history` 单行（snake_case）。 */
export interface RawUsageEntry {
  id: number
  provider: string | null
  model: string | null
  connection_id: string | null
  api_key_id: string | null
  api_key_name: string | null
  tokens_input: number
  tokens_output: number
  tokens_cache_read: number
  tokens_cache_creation: number
  tokens_reasoning: number
  service_tier: string
  status: string
  success: boolean
  error_code: string | null
  latency_ms: number | null
  ttft_ms: number | null
  cost: number
  timestamp: string
}

/* ============ ccMesh 数据形状 ============ */

export interface EndpointStat {
  endpointName: string
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
}

export interface PeriodStats {
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
  endpoints: EndpointStat[]
}

export interface TrendCompare {
  requestsPct: number
  inputTokensPct: number
  outputTokensPct: number
}

export interface StatsOverview {
  today: PeriodStats
  yesterday: PeriodStats
  thisWeek: PeriodStats
  thisMonth: PeriodStats
  trend: TrendCompare
}

/** 端点 × 日聚合行。 */
export interface DailyStat {
  endpointName: string
  date: string
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
}

export interface StatsHistoryPage {
  items: DailyStat[]
  total: number
}

/** 按本地小时聚合（跨端点）。`date` 为 `YYYY-MM-DD HH:00`。 */
export interface HourlyStat {
  date: string
  requests: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
}

/** 逐条请求明细。 */
export interface RequestLog {
  id: number
  /** 请求时间（Unix 毫秒，本地解析）。 */
  ts: number
  endpointName: string
  inboundFormat: string
  transformer: string | null
  upstreamUrl: string
  inboundPath: string
  upstreamPath: string
  statusCode: number | null
  isError: boolean
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
  model: string | null
  durationMs: number | null
  firstByteMs: number | null
  actualModel: string | null
  errorBody: string | null
  cost: number
}

export interface RequestLogPage {
  items: RequestLog[]
  total: number
}

export interface RequestLogQuery {
  startMs?: number
  endMs?: number
  endpoint?: string
  page: number
  pageSize: number
}

/* ============ 明细拉取与缓存 ============ */

/** 一次拉取上限：明细量级为千，足够覆盖近一年统计。 */
const FETCH_LIMIT = 20000
/** 缓存 TTL，避免同一屏多个面板重复请求。 */
const CACHE_TTL_MS = 3000

let entriesCache: { at: number; data: RawUsageEntry[] } | null = null
let inflight: Promise<RawUsageEntry[]> | null = null

/** 强制下次读取重新拉取（刷新按钮 / 写操作后调用）。 */
export function invalidateStatsCache(): void {
  entriesCache = null
  inflight = null
  connCache = null
}

async function fetchEntries(): Promise<RawUsageEntry[]> {
  const now = Date.now()
  if (entriesCache && now - entriesCache.at < CACHE_TTL_MS) return entriesCache.data
  if (inflight) return inflight
  inflight = (async () => {
    try {
      const { data } = await api.get('/usage', { params: { limit: FETCH_LIMIT } })
      const list: unknown = Array.isArray(data) ? data : (data?.usage ?? data?.records ?? [])
      const entries = Array.isArray(list) ? (list as RawUsageEntry[]) : []
      entriesCache = { at: Date.now(), data: entries }
      return entries
    } catch {
      // 后端未启动时静默降级为空集，页面走空态而不是崩溃
      entriesCache = { at: Date.now(), data: [] }
      return []
    } finally {
      inflight = null
    }
  })()
  return inflight
}

/** connection_id → 连接名，用于把明细里的 id 显示成可读端点名。 */
let connCache: Map<string, string> | null = null
async function connNames(): Promise<Map<string, string>> {
  if (connCache) return connCache
  const map = new Map<string, string>()
  try {
    const res = await listProviders()
    for (const c of res.connections ?? []) map.set(c.id, c.name)
  } catch {
    /* 后端未启动时用 provider 兜底 */
  }
  connCache = map
  return map
}

/**
 * 时间戳解析：兼容 ISO（`2026-09-10T11:41:28Z`）与 SQLite 风格（`2026-09-10 11:41:28`）。
 * 后者无时区标记，按本地时间解析，与后端 `DATE(timestamp)` 的本地分组语义一致。
 */
export function parseTs(ts: string | null | undefined): number {
  if (!ts) return 0
  const direct = Date.parse(ts)
  if (Number.isFinite(direct) && /[TZ]|[+-]\d{2}:?\d{2}$/.test(ts)) return direct
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?/.exec(ts)
  if (m) return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +(m[6] ?? 0)).getTime()
  return Number.isFinite(direct) ? direct : 0
}

/** 统一视图：把原始行归一成带毫秒时间与端点名的中间结构。 */
interface NormEntry {
  raw: RawUsageEntry
  ts: number
  date: string
  endpointName: string
  isError: boolean
}

async function normalized(): Promise<NormEntry[]> {
  const [entries, names] = await Promise.all([fetchEntries(), connNames()])
  return entries.map((raw) => {
    const ts = parseTs(raw.timestamp)
    const endpointName =
      (raw.connection_id ? names.get(raw.connection_id) : undefined) ??
      raw.provider ??
      raw.connection_id ??
      '未知端点'
    return { raw, ts, date: ymd(ts), endpointName, isError: !raw.success }
  })
}

/* ============ 聚合工具 ============ */

function emptyPeriod(): PeriodStats {
  return {
    requests: 0,
    errors: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheCreationTokens: 0,
    cacheReadTokens: 0,
    endpoints: [],
  }
}

function accumulate(target: Omit<PeriodStats, 'endpoints'>, e: NormEntry): void {
  target.requests += 1
  if (e.isError) target.errors += 1
  target.inputTokens += e.raw.tokens_input ?? 0
  target.outputTokens += e.raw.tokens_output ?? 0
  target.cacheCreationTokens += e.raw.tokens_cache_creation ?? 0
  target.cacheReadTokens += e.raw.tokens_cache_read ?? 0
}

/** 半开区间 [startMs, endMs) 内的行聚合成 PeriodStats（含按端点拆分）。 */
function aggregatePeriod(rows: NormEntry[], startMs: number, endMs: number): PeriodStats {
  const totals = emptyPeriod()
  const byEndpoint = new Map<string, EndpointStat>()
  for (const e of rows) {
    if (e.ts < startMs || e.ts >= endMs) continue
    accumulate(totals, e)
    let ep = byEndpoint.get(e.endpointName)
    if (!ep) {
      ep = {
        endpointName: e.endpointName,
        requests: 0,
        errors: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheCreationTokens: 0,
        cacheReadTokens: 0,
      }
      byEndpoint.set(e.endpointName, ep)
    }
    accumulate(ep, e)
  }
  totals.endpoints = [...byEndpoint.values()].sort((a, b) => b.requests - a.requests)
  return totals
}

/** 环比百分比：基期为 0 时，本期有值记 100%，否则 0%。 */
function pct(cur: number, prev: number): number {
  if (prev === 0) return cur > 0 ? 100 : 0
  return ((cur - prev) / prev) * 100
}

const DAY_MS = 86_400_000

/* ============ statsApi ============ */

export const statsApi = {
  /** 四周期实时聚合 + 今日环比昨日趋势。 */
  async getStats(): Promise<StatsOverview> {
    const rows = await normalized()
    const d = new Date()
    d.setHours(0, 0, 0, 0)
    const todayStart = d.getTime()
    const tomorrow = todayStart + DAY_MS
    const yesterdayStart = todayStart - DAY_MS
    // 周一为一周起点
    const dow = (new Date(todayStart).getDay() + 6) % 7
    const weekStart = todayStart - dow * DAY_MS
    const monthStart = new Date(d.getFullYear(), d.getMonth(), 1).getTime()

    const today = aggregatePeriod(rows, todayStart, tomorrow)
    const yesterday = aggregatePeriod(rows, yesterdayStart, todayStart)
    return {
      today,
      yesterday,
      thisWeek: aggregatePeriod(rows, weekStart, tomorrow),
      thisMonth: aggregatePeriod(rows, monthStart, tomorrow),
      trend: {
        requestsPct: pct(today.requests, yesterday.requests),
        inputTokensPct: pct(today.inputTokens, yesterday.inputTokens),
        outputTokensPct: pct(today.outputTokens, yesterday.outputTokens),
      },
    }
  },

  /** 端点 × 日聚合行分页（date 倒序，同日按请求数降序）。 */
  async getStatsHistory(page: number, pageSize: number): Promise<StatsHistoryPage> {
    const rows = await normalized()
    const map = new Map<string, DailyStat>()
    for (const e of rows) {
      const key = `${e.date}\u0000${e.endpointName}`
      let cur = map.get(key)
      if (!cur) {
        cur = {
          endpointName: e.endpointName,
          date: e.date,
          requests: 0,
          errors: 0,
          inputTokens: 0,
          outputTokens: 0,
          cacheCreationTokens: 0,
          cacheReadTokens: 0,
        }
        map.set(key, cur)
      }
      accumulate(cur, e)
    }
    const items = [...map.values()].sort(
      (a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : b.requests - a.requests),
    )
    const start = Math.max(0, (page - 1) * pageSize)
    return { items: items.slice(start, start + pageSize), total: items.length }
  },

  /** 请求明细按本地小时聚合（跨端点求和），闭区间 [startMs, endMs]。 */
  async getRequestLogsHourly(q: { startMs?: number; endMs?: number }): Promise<HourlyStat[]> {
    const rows = await normalized()
    const map = new Map<string, HourlyStat>()
    for (const e of rows) {
      if (q.startMs != null && e.ts < q.startMs) continue
      if (q.endMs != null && e.ts > q.endMs) continue
      const key = hourKey(e.ts)
      let cur = map.get(key)
      if (!cur) {
        cur = {
          date: key,
          requests: 0,
          inputTokens: 0,
          outputTokens: 0,
          cacheCreationTokens: 0,
          cacheReadTokens: 0,
        }
        map.set(key, cur)
      }
      cur.requests += 1
      cur.inputTokens += e.raw.tokens_input ?? 0
      cur.outputTokens += e.raw.tokens_output ?? 0
      cur.cacheCreationTokens += e.raw.tokens_cache_creation ?? 0
      cur.cacheReadTokens += e.raw.tokens_cache_read ?? 0
    }
    return [...map.values()].sort((a, b) => (a.date < b.date ? -1 : 1))
  },

  /** 请求明细分页查询（时间段 + 可选端点过滤）。 */
  async getRequestLogs(q: RequestLogQuery): Promise<RequestLogPage> {
    const rows = await normalized()
    const filtered = rows
      .filter((e) => {
        if (q.startMs != null && e.ts < q.startMs) return false
        if (q.endMs != null && e.ts >= q.endMs) return false
        if (q.endpoint && e.endpointName !== q.endpoint) return false
        return true
      })
      .sort((a, b) => b.ts - a.ts)
    const start = Math.max(0, (q.page - 1) * q.pageSize)
    const items = filtered.slice(start, start + q.pageSize).map<RequestLog>((e) => {
      const r = e.raw
      const format = inferFormat(r.provider)
      return {
        id: r.id,
        ts: e.ts,
        endpointName: e.endpointName,
        inboundFormat: format,
        transformer: r.provider,
        upstreamUrl: '',
        inboundPath: '',
        upstreamPath: '',
        statusCode: statusToCode(r.status, r.success),
        isError: e.isError,
        inputTokens: r.tokens_input ?? 0,
        outputTokens: r.tokens_output ?? 0,
        cacheCreationTokens: r.tokens_cache_creation ?? 0,
        cacheReadTokens: r.tokens_cache_read ?? 0,
        model: r.model,
        durationMs: r.latency_ms,
        firstByteMs: r.ttft_ms,
        actualModel: null,
        errorBody: r.error_code,
        cost: r.cost ?? 0,
      }
    })
    return { items, total: filtered.length }
  },

  /** TODO(backend)：后端暂无保留期配置，先返回 null 让 UI 隐藏该项。 */
  async getRetentionDays(): Promise<number | null> {
    return null
  },

  /* --- 以下写操作后端尚未提供，统一抛出可读错误，UI 会 toast 出来 --- */
  async deleteDailyStat(_endpointName: string, _date: string): Promise<number> {
    throw new Error('后端暂未提供按端点删除统计的接口')
  },
  async deleteStatsByDate(_date: string): Promise<number> {
    throw new Error('后端暂未提供按日期删除统计的接口')
  },
  async pruneRequestLogs(): Promise<number> {
    throw new Error('后端暂未提供请求明细清理接口')
  },
  async clearRequestLogs(): Promise<number> {
    throw new Error('后端暂未提供请求明细清空接口')
  },
}

/** provider → 入站协议（用于图标与路径推断）。 */
function inferFormat(provider: string | null): string {
  const p = (provider ?? '').toLowerCase()
  if (p.includes('anthropic') || p.includes('claude')) return 'claude'
  if (p.includes('codex') || p.includes('responses')) return 'responses'
  return 'openai'
}

/** status/success → HTTP 码近似值（明细未落库真实状态码）。 */
function statusToCode(status: string | null, success: boolean): number | null {
  if (success) return 200
  const s = (status ?? '').toLowerCase()
  if (s.includes('timeout')) return 504
  if (s.includes('rate') || s.includes('429')) return 429
  if (s.includes('auth') || s.includes('401')) return 401
  const num = Number(status)
  if (Number.isFinite(num) && num >= 100 && num < 600) return num
  return null
}

/* ============ usageApi（用量统计面板） ============ */

export interface UsageSummary {
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCacheCreationTokens: number
  totalCacheReadTokens: number
  totalCost: number
}

/** 按天 × 来源 × 模型聚合。 */
export interface DayModelUsage {
  date: string
  appType: string
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
}

/** 按天（或小时）× 来源聚合。 */
export interface DailyUsage {
  date: string
  appType: string
  requests: number
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
}

export interface UsageFilter {
  /** date 闭区间（YYYY-MM-DD，预设周期） */
  start?: string
  end?: string
  /** ts 毫秒闭区间（自定义时分范围） */
  startTs?: number
  endTs?: number
  /** 来源过滤：vortex 语义下即 provider。 */
  appType?: string
}

function passFilter(e: NormEntry, f: UsageFilter): boolean {
  if (f.appType && appTypeOf(e) !== f.appType) return false
  if (f.start && e.date < f.start) return false
  if (f.end && e.date > f.end) return false
  if (f.startTs != null && e.ts < f.startTs) return false
  if (f.endTs != null && e.ts > f.endTs) return false
  return true
}

/** ccMesh 的「来源」在 vortex 对应 provider。 */
function appTypeOf(e: NormEntry): string {
  return e.raw.provider ?? 'unknown'
}

export const usageApi = {
  /** ccMesh 是扫本机会话日志；vortex 数据本就在库里，这里等价于刷新缓存。 */
  async sync(): Promise<{ imported: number; filesScanned: number; errors: number }> {
    invalidateStatsCache()
    const rows = await fetchEntries()
    return { imported: 0, filesScanned: rows.length, errors: 0 }
  },

  /** 当前筛选下出现过的来源（供来源 Tab 动态生成）。 */
  async listAppTypes(): Promise<string[]> {
    const rows = await normalized()
    return [...new Set(rows.map(appTypeOf))].sort()
  },

  async getSummary(f: UsageFilter = {}): Promise<UsageSummary> {
    const rows = await normalized()
    const s: UsageSummary = {
      totalRequests: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalCacheCreationTokens: 0,
      totalCacheReadTokens: 0,
      totalCost: 0,
    }
    for (const e of rows) {
      if (!passFilter(e, f)) continue
      s.totalRequests += 1
      s.totalInputTokens += e.raw.tokens_input ?? 0
      s.totalOutputTokens += e.raw.tokens_output ?? 0
      s.totalCacheCreationTokens += e.raw.tokens_cache_creation ?? 0
      s.totalCacheReadTokens += e.raw.tokens_cache_read ?? 0
      s.totalCost += e.raw.cost ?? 0
    }
    return s
  },

  /** date 倒序、组内 token 降序（与 ccMesh 后端排序一致，前端表格按 date 合并行）。 */
  async getByDayModel(f: UsageFilter = {}): Promise<DayModelUsage[]> {
    const rows = await normalized()
    const map = new Map<string, DayModelUsage>()
    for (const e of rows) {
      if (!passFilter(e, f)) continue
      const app = appTypeOf(e)
      const model = e.raw.model ?? ''
      const key = `${e.date}\u0000${app}\u0000${model}`
      let cur = map.get(key)
      if (!cur) {
        cur = {
          date: e.date,
          appType: app,
          model,
          requests: 0,
          inputTokens: 0,
          outputTokens: 0,
          cacheCreationTokens: 0,
          cacheReadTokens: 0,
        }
        map.set(key, cur)
      }
      cur.requests += 1
      cur.inputTokens += e.raw.tokens_input ?? 0
      cur.outputTokens += e.raw.tokens_output ?? 0
      cur.cacheCreationTokens += e.raw.tokens_cache_creation ?? 0
      cur.cacheReadTokens += e.raw.tokens_cache_read ?? 0
    }
    const tokensOf = (r: DayModelUsage) =>
      r.inputTokens + r.outputTokens + r.cacheCreationTokens + r.cacheReadTokens
    return [...map.values()].sort((a, b) => {
      if (a.date !== b.date) return a.date < b.date ? 1 : -1
      return tokensOf(b) - tokensOf(a)
    })
  },

  async getByDay(f: UsageFilter = {}): Promise<DailyUsage[]> {
    return groupByBucket(await normalized(), f, (e) => e.date)
  },

  /** `date` 为 `YYYY-MM-DD HH:00`。 */
  async getByHour(f: UsageFilter = {}): Promise<DailyUsage[]> {
    return groupByBucket(await normalized(), f, (e) => hourKey(e.ts))
  },
}

function groupByBucket(
  rows: NormEntry[],
  f: UsageFilter,
  bucket: (e: NormEntry) => string,
): DailyUsage[] {
  const map = new Map<string, DailyUsage>()
  for (const e of rows) {
    if (!passFilter(e, f)) continue
    const app = appTypeOf(e)
    const date = bucket(e)
    const key = `${date}\u0000${app}`
    let cur = map.get(key)
    if (!cur) {
      cur = {
        date,
        appType: app,
        requests: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheCreationTokens: 0,
        cacheReadTokens: 0,
      }
      map.set(key, cur)
    }
    cur.requests += 1
    cur.inputTokens += e.raw.tokens_input ?? 0
    cur.outputTokens += e.raw.tokens_output ?? 0
    cur.cacheCreationTokens += e.raw.tokens_cache_creation ?? 0
    cur.cacheReadTokens += e.raw.tokens_cache_read ?? 0
  }
  return [...map.values()].sort((a, b) => (a.date < b.date ? -1 : 1))
}
