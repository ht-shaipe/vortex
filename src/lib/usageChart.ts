/**
 * 用量图表的纯数据逻辑：按日合并、热力图网格、强度分级、趋势切片补零（按日 / 按小时）。
 * 与组件分离便于复用。
 */
import { startOfDayMs } from '@/lib/dateRange'
import { ymd, type RangeValue, type TrendWindow } from '@/lib/range'

/** 一天的毫秒数。 */
const DAY_MS = 86_400_000

/** 按天汇总的用量统计。 */
export interface DayTotals {
  /** 请求数 */
  requests: number
  /** 输入 Token 数 */
  inputTokens: number
  /** 输出 Token 数 */
  outputTokens: number
  /** 缓存 Token 数 */
  cacheTokens: number
  /** 总 Token 数 */
  totalTokens: number
}

/** 按天行的最小结构（用量 DailyUsage / 端点 DailyStat 均满足）。 */
export interface DayRow {
  /** 日期 YYYY-MM-DD */
  date: string
  /** 请求数 */
  requests: number
  /** 输入 Token 数 */
  inputTokens: number
  /** 输出 Token 数 */
  outputTokens: number
  /** 缓存创建 Token 数 */
  cacheCreationTokens: number
  /** 缓存读取 Token 数 */
  cacheReadTokens: number
}

/**
 * 后端按 (date, 来源/端点) 返回多行，这里按 date 合并求和。
 * @param rows - 原始按天分行的数据
 * @returns 以日期为键、汇总统计为值的 Map
 */
export function mergeByDate(rows: DayRow[]): Map<string, DayTotals> {
  const map = new Map<string, DayTotals>()
  for (const r of rows) {
    // 取已有或初始化汇总对象
    const t = map.get(r.date) ?? {
      requests: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheTokens: 0,
      totalTokens: 0,
    }
    t.requests += r.requests
    t.inputTokens += r.inputTokens
    t.outputTokens += r.outputTokens
    t.cacheTokens += r.cacheCreationTokens + r.cacheReadTokens
    t.totalTokens += r.inputTokens + r.outputTokens + r.cacheCreationTokens + r.cacheReadTokens
    map.set(r.date, t)
  }
  return map
}

/** 热力图单元格。 */
export interface HeatmapCell {
  /** 日期 YYYY-MM-DD */
  date: string
  /** 当天 0 点毫秒时间戳 */
  ms: number
  /** 周一=0 … 周日=6（网格行号）。 */
  dayIndex: number
}

/**
 * 周一对齐的行号：周一=0 … 周日=6。
 * @param ms - 毫秒时间戳
 * @returns 周一为起始的行号 0-6
 */
function mondayIndex(ms: number): number {
  return (new Date(ms).getDay() + 6) % 7
}

/**
 * 生成 GitHub 贡献图风格的格子序列：从 `weeks` 周前那一周的周一起，到今天止。
 * 顺序为按天递增，配合 `grid-auto-flow: column`（7 行）即自动按周成列。
 * @param todayStartMs - 当天 0 点毫秒时间戳
 * @param weeks - 显示周数，默认 53
 * @returns 热力图单元格数组
 */
export function buildHeatmapCells(todayStartMs: number, weeks = 53): HeatmapCell[] {
  // 本周周一
  const currentMonday = todayStartMs - mondayIndex(todayStartMs) * DAY_MS
  // 起始周一（weeks-1 周前）
  const start = new Date(currentMonday - (weeks - 1) * 7 * DAY_MS)
  // 用 Date 构造器逐日递增而非 +DAY_MS，规避 DST 跳变导致的重复/缺日。
  const cells: HeatmapCell[] = []
  const maxCells = weeks * 7 + 7 // 上限：周数 × 7 + 余量
  for (let i = 0; i < maxCells; i++) {
    const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i)
    const ms = d.getTime()
    if (ms > todayStartMs) break
    cells.push({ date: ymd(ms), ms, dayIndex: mondayIndex(ms) })
  }
  return cells
}

/**
 * 相对分级：0 无数据；1–4 按与最大值的比例分档（自适应任意数据规模）。
 * @param value - 当前值
 * @param max - 最大值
 * @returns 强度等级 0-4
 */
export function heatLevel(value: number, max: number): 0 | 1 | 2 | 3 | 4 {
  if (value <= 0 || max <= 0) return 0
  const level = Math.ceil((value / max) * 4)
  return Math.min(4, Math.max(1, level)) as 1 | 2 | 3 | 4
}

/** 趋势图数据点。 */
export interface TrendPoint {
  /** 日期或小时键 */
  date: string
  /** X 轴短标签，如 `8/12`。 */
  label: string
  /** 请求数 */
  requests: number
  /** Token 总数 */
  tokens: number
}

/**
 * 解析 YYYY-MM-DD 日期字符串为 Date 对象（本地时区）。
 * @param date - YYYY-MM-DD 格式日期
 * @returns Date 对象
 */
function parseYmd(date: string): Date {
  const [y, m, d] = date.split('-').map(Number)
  return new Date(y, m - 1, d)
}

/**
 * 按时间周期切出趋势序列（升序），缺失日期补 0。
 * 预设 `all`：从数据最早日期到今天；数据为空时仅今天一个点。
 * 自定义：起止日闭区间（终点不超过今天）。
 * @param merged - 按日期合并后的数据 Map
 * @param range - 时间段选择值
 * @param todayStartMs - 当天 0 点毫秒时间戳
 * @returns 趋势数据点数组
 */
export function sliceTrend(
  merged: Map<string, DayTotals>,
  range: RangeValue,
  todayStartMs: number,
): TrendPoint[] {
  let startMs: number
  let endMs = todayStartMs
  if (range.kind === 'custom') {
    // 自定义区间：起点取当天 0 点，终点不超过今天
    startMs = startOfDayMs(range.startMs)
    endMs = Math.max(startMs, Math.min(startOfDayMs(range.endMs), todayStartMs))
  } else if (range.key === 'today') startMs = todayStartMs
  else if (range.key === '7d') startMs = todayStartMs - 6 * DAY_MS
  else if (range.key === '30d') startMs = todayStartMs - 29 * DAY_MS
  else {
    // 'all'：从数据最早日期开始
    const dates = [...merged.keys()].sort()
    startMs = dates.length ? parseYmd(dates[0]).getTime() : todayStartMs
  }

  // 逐日生成数据点，缺失日期补 0
  const start = new Date(startMs)
  const points: TrendPoint[] = []
  const maxDays = 366 * 5 // 上限 5 年，防止异常输入导致死循环
  for (let i = 0; i < maxDays; i++) {
    const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i)
    const ms = d.getTime()
    if (ms > endMs) break
    const date = ymd(ms)
    const t = merged.get(date)
    points.push({
      date,
      label: `${d.getMonth() + 1}/${d.getDate()}`,
      requests: t?.requests ?? 0,
      tokens: t?.totalTokens ?? 0,
    })
  }
  return points
}

/**
 * 取给定时间戳所在小时的 0 分 0 秒毫秒时间戳。
 * @param ms - 毫秒时间戳
 * @returns 整点毫秒时间戳
 */
function startOfHourMs(ms: number): number {
  const d = new Date(ms)
  d.setMinutes(0, 0, 0)
  return d.getTime()
}

/**
 * 本地小时键，与 `YYYY-MM-DD HH:00` 对齐。
 * @param ms - 毫秒时间戳
 * @returns 小时键字符串
 */
export function hourKey(ms: number): string {
  const d = new Date(ms)
  return `${ymd(ms)} ${String(d.getHours()).padStart(2, '0')}:00`
}

/**
 * 按小时切出趋势序列（升序），缺失小时补 0。
 * 上界截到 `min(now, endExclusive-1)` 所在小时，不垫未来空小时。
 * @param merged - 按日期/小时合并后的数据 Map
 * @param param0 - 趋势窗口 { startMs, endExclusiveMs }
 * @param nowMs - 当前时间戳
 * @returns 趋势数据点数组
 */
export function sliceHourlyTrend(
  merged: Map<string, DayTotals>,
  { startMs, endExclusiveMs }: TrendWindow,
  nowMs: number,
): TrendPoint[] {
  // 起始整点
  const first = startOfHourMs(startMs)
  // 结束整点：不超过当前时间和窗口结束前一刻
  const last = startOfHourMs(Math.min(nowMs, endExclusiveMs - 1))
  if (last < first) return []
  // 逐小时生成数据点，缺失补 0
  const points: TrendPoint[] = []
  const maxHours = 24 * 366 // 上限 1 年，防止异常输入导致死循环
  for (let i = 0; i < maxHours; i++) {
    const d = new Date(first)
    d.setHours(d.getHours() + i)
    const ms = d.getTime()
    if (ms > last) break
    const date = hourKey(ms)
    const t = merged.get(date)
    points.push({
      date,
      label: `${String(d.getHours()).padStart(2, '0')}:00`,
      requests: t?.requests ?? 0,
      tokens: t?.totalTokens ?? 0,
    })
  }
  return points
}
