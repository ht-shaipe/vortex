<template>
  <div class="panel">
    <div class="panel-bar">
      <DateRangePicker v-model="range" :presets="PERIOD_PRESETS" />
      <HistoryDialog @changed="reload" />
      <button type="button" class="btn sm" :disabled="loading" @click="reload">
        <el-icon :size="13" :class="{ spin: loading }"><Refresh /></el-icon>刷新
      </button>
    </div>

    <p v-if="loading && !overview" class="hint">加载中…</p>

    <template v-else>
      <div class="kpi-row">
        <StatCard label="请求" :value="stats?.requests ?? 0">
          <template v-if="showTrend" #hint>
            <TrendBadge :pct="overview!.trend.requestsPct" />
          </template>
        </StatCard>
        <StatCard label="错误" :value="stats?.errors ?? 0" />
        <StatCard label="输入 Token" :value="stats?.inputTokens ?? 0">
          <template v-if="showTrend" #hint>
            <TrendBadge :pct="overview!.trend.inputTokensPct" />
          </template>
        </StatCard>
        <StatCard label="输出 Token" :value="stats?.outputTokens ?? 0">
          <template v-if="showTrend" #hint>
            <TrendBadge :pct="overview!.trend.outputTokensPct" />
          </template>
        </StatCard>
      </div>

      <section class="sec">
        <h2 class="sec-title">调用热力图</h2>
        <div class="card sec-body">
          <UsageHeatmap :totals="dayTotals" />
        </div>
      </section>

      <section class="card sec-body">
        <UsageTrendChart :data="trendData" />
      </section>

      <EndpointStatsTable v-if="(stats?.endpoints.length ?? 0) > 0" :rows="stats!.endpoints" />
    </template>

    <RequestMonitor mode="ranged" :range="range" hide-when-empty :refresh-key="refreshKey" />
  </div>
</template>

<script setup lang="ts">
/**
 * EndpointStatsPanel.vue — 端点统计面板
 * 职责：展示端点维度的统计概览（KPI、趋势徽章）、热力图、趋势图、明细表格与请求监控。
 */
import { computed, ref, watch } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import DateRangePicker from './DateRangePicker.vue'
import HistoryDialog from './HistoryDialog.vue'
import StatCard from './StatCard.vue'
import TrendBadge from './TrendBadge.vue'
import EndpointStatsTable from './EndpointStatsTable.vue'
import UsageHeatmap from './UsageHeatmap.vue'
import UsageTrendChart from './UsageTrendChart.vue'
import RequestMonitor from './RequestMonitor.vue'
import {
  isHourlyTrend,
  rangeValueEquals,
  resolveTrendWindow,
  startOfTodayMs,
  ymd,
  type RangePreset,
  type RangeValue,
} from '@/lib/range'
import { mergeByDate, sliceHourlyTrend, sliceTrend, type DayTotals } from '@/lib/usageChart'
import {
  invalidateStatsCache,
  statsApi,
  type DailyStat,
  type EndpointStat,
  type HourlyStat,
  type PeriodStats,
  type StatsOverview,
} from '@/api/stats'

// 周期定义（今日/昨日/本周/本月）
const PERIODS = [
  { key: 'today', label: '今日' },
  { key: 'yesterday', label: '昨日' },
  { key: 'thisWeek', label: '本周' },
  { key: 'thisMonth', label: '本月' },
] as const

type PeriodKey = (typeof PERIODS)[number]['key'] // 周期键类型

const DAY_MS = 86_400_000 // 一天的毫秒数

/** 周期 Tab → 趋势图日期区间（周一为一周起点，与后端周聚合一致）。 */
function periodRange(period: PeriodKey, todayStartMs: number): RangeValue {
  switch (period) {
    case 'today':
      return { kind: 'preset', key: 'today' }
    case 'yesterday': {
      const y = todayStartMs - DAY_MS
      return { kind: 'custom', startMs: y, endMs: y }
    }
    case 'thisWeek': {
      const dow = (new Date(todayStartMs).getDay() + 6) % 7
      return { kind: 'custom', startMs: todayStartMs - dow * DAY_MS, endMs: todayStartMs }
    }
    case 'thisMonth': {
      const d = new Date(todayStartMs)
      return {
        kind: 'custom',
        startMs: new Date(d.getFullYear(), d.getMonth(), 1).getTime(),
        endMs: todayStartMs,
      }
    }
  }
}

/** 日期选择器快捷项：四个周期（命中时统计卡走实时聚合）。 */
const PERIOD_PRESETS: RangePreset[] = PERIODS.map((p) => ({
  key: p.key,
  label: p.label,
  value: (todayStartMs: number) => periodRange(p.key, todayStartMs),
}))

/** 按日期区间（YYYY-MM-DD 闭区间）从历史行聚合出周期统计（自定义范围用，天粒度）。 */
function aggregateRange(rows: DailyStat[], startDate: string, endDate: string): PeriodStats {
  const totals: PeriodStats = {
    requests: 0,
    errors: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheCreationTokens: 0,
    cacheReadTokens: 0,
    endpoints: [],
  }
  const byEndpoint = new Map<string, EndpointStat>()
  for (const r of rows) {
    if (r.date < startDate || r.date > endDate) continue
    totals.requests += r.requests
    totals.errors += r.errors
    totals.inputTokens += r.inputTokens
    totals.outputTokens += r.outputTokens
    totals.cacheCreationTokens += r.cacheCreationTokens
    totals.cacheReadTokens += r.cacheReadTokens
    let ep = byEndpoint.get(r.endpointName)
    if (!ep) {
      ep = {
        endpointName: r.endpointName,
        requests: 0,
        errors: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheCreationTokens: 0,
        cacheReadTokens: 0,
      }
      byEndpoint.set(r.endpointName, ep)
    }
    ep.requests += r.requests
    ep.errors += r.errors
    ep.inputTokens += r.inputTokens
    ep.outputTokens += r.outputTokens
    ep.cacheCreationTokens += r.cacheCreationTokens
    ep.cacheReadTokens += r.cacheReadTokens
  }
  totals.endpoints = [...byEndpoint.values()].sort((a, b) => b.requests - a.requests)
  return totals
}

const range = ref<RangeValue>({ kind: 'preset', key: 'today' }) // 日期范围筛选值
const loading = ref(true) // 是否正在加载
const refreshKey = ref(0) // 外部刷新触发键
const overview = ref<StatsOverview | null>(null) // 统计概览
const historyRows = ref<DailyStat[]>([]) // 历史按天统计行
const hourlyRows = ref<HourlyStat[]>([]) // 按小时统计行

const todayStart = startOfTodayMs() // 今日零点时间戳

/** 复用分页接口一次拉全（端点×日行数量级为千，无需新接口）。 */
async function loadBase(): Promise<void> {
  loading.value = true
  try {
    const [ov, hist] = await Promise.all([
      statsApi.getStats(),
      statsApi.getStatsHistory(1, 100_000),
    ])
    overview.value = ov
    historyRows.value = hist.items
  } finally {
    loading.value = false
  }
}

const trendWin = computed(() => resolveTrendWindow(range.value, todayStart)) // 趋势图时间窗口
const hourly = computed(() => isHourlyTrend(trendWin.value)) // 是否使用小时粒度趋势

/** 加载按小时数据（仅小时粒度趋势时拉取）。 */
async function loadHourly(): Promise<void> {
  const w = trendWin.value
  if (!hourly.value || !w) {
    hourlyRows.value = []
    return
  }
  hourlyRows.value = await statsApi.getRequestLogsHourly({
    startMs: w.startMs,
    endMs: w.endExclusiveMs - 1,
  })
}

/** 刷新：失效缓存并重新加载基础与小时数据。 */
function reload(): void {
  invalidateStatsCache()
  refreshKey.value += 1
  void loadBase()
  void loadHourly()
}

watch(trendWin, () => void loadHourly(), { immediate: true })
void loadBase() // 初始加载

const dayTotals = computed<Map<string, DayTotals>>(() => mergeByDate(historyRows.value)) // 按日期合并的总量（热力图用）

// 趋势图数据（小时粒度或天粒度切片）
const trendData = computed(() => {
  const w = trendWin.value
  if (hourly.value && w) {
    return sliceHourlyTrend(mergeByDate(hourlyRows.value), w, Date.now())
  }
  return sliceTrend(dayTotals.value, range.value, todayStart)
})

/** 命中四周期快捷项 → 用实时聚合；自定义区间 → 从历史行按天聚合。 */
const activePeriod = computed(() =>
  PERIODS.map((p) => p.key).find((k) => rangeValueEquals(range.value, periodRange(k, todayStart))),
)

// 当前周期统计（命中预设用实时聚合，自定义区间从历史行聚合）
const stats = computed<PeriodStats | undefined>(() => {
  const p = activePeriod.value
  if (p) return overview.value?.[p]
  if (range.value.kind !== 'custom') return undefined
  return aggregateRange(historyRows.value, ymd(range.value.startMs), ymd(range.value.endMs))
})

// 是否显示趋势徽章（仅今日周期且有趋势数据时）
const showTrend = computed(() => activePeriod.value === 'today' && !!overview.value?.trend)
</script>

<style scoped>
.panel { display: flex; flex-direction: column; gap: var(--gap-xl); }
.panel-bar { display: flex; align-items: center; justify-content: flex-end; gap: var(--gap-sm); }
.hint { margin: 0; font-size: var(--fs-body); color: var(--ink-4); }
.kpi-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: var(--gap-md); }
@media (max-width: 900px) { .kpi-row { grid-template-columns: repeat(2, 1fr); } }
.sec { display: flex; flex-direction: column; gap: var(--gap-sm); }
.sec-title { margin: 0; font-size: 13px; font-weight: 600; color: var(--ink-2); }
.sec-body { padding: var(--pad-card); }
</style>
