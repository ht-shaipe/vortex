<template>
  <div class="panel flex flex-col">
    <div class="panel-bar flex items-center justify-between flex-wrap">
      <div class="tabs slim mb-0 border-b-none">
        <button
          v-for="t in appTabs"
          :key="t.key"
          type="button"
          class="tab"
          :class="{ active: app === t.key }"
          @click="app = t.key"
        >
          {{ t.label }}
        </button>
      </div>
      <div class="panel-actions flex items-center">
        <DateRangePicker v-model="range" />
        <button type="button" class="btn sm" :disabled="syncing" @click="sync">
          <el-icon :size="13" :class="{ spin: syncing }"><Refresh /></el-icon>刷新
        </button>
      </div>
    </div>

    <div class="kpi-row grid">
      <StatCard label="请求数" :value="summary?.totalRequests ?? 0" />
      <StatCard label="输入 Token" :value="summary?.totalInputTokens ?? 0" hint-below>
        <template #hint>
          <TokenHint :value="summary?.totalInputTokens ?? 0" />
        </template>
      </StatCard>
      <StatCard label="输出 Token" :value="summary?.totalOutputTokens ?? 0" hint-below>
        <template #hint>
          <TokenHint :value="summary?.totalOutputTokens ?? 0" />
        </template>
      </StatCard>
      <StatCard label="缓存 Token" :value="cacheTotal" hint-below>
        <template #hint>
          <TokenHint :value="cacheTotal" />
        </template>
      </StatCard>
    </div>

    <section class="sec flex flex-col">
      <h2 class="sec-title m-0 text-13px font-semibold text-ink-2">调用热力图</h2>
      <div class="card sec-body">
        <UsageHeatmap :totals="dayTotals" />
      </div>
    </section>

    <section class="card sec-body">
      <UsageTrendChart :data="trendData" />
    </section>

    <section v-if="dayModelRows.length > 0" class="sec flex flex-col">
      <h2 class="sec-title m-0 text-13px font-semibold text-ink-2">按日期 · 模型</h2>
      <div class="card table-wrap overflow-hidden">
        <table class="table">
          <thead>
            <tr>
              <th>日期</th>
              <th>来源</th>
              <th>模型</th>
              <th class="right text-right">请求</th>
              <th class="right text-right">输入</th>
              <th class="right text-right">输出</th>
              <th class="right text-right">缓存</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="g in groups" :key="g.date">
              <tr v-for="(r, i) in g.rows" :key="`${g.date}-${r.appType}-${r.model}-${i}`">
                <td v-if="i === 0" :rowspan="g.rows.length" class="num date-cell border-r border-line">{{ g.date }}</td>
                <td class="small text-sm text-ink-3">{{ r.appType || '—' }}</td>
                <td class="mono small text-sm text-ink-3">{{ r.model || '—' }}</td>
                <td class="right num text-right">{{ fmtInt(r.requests) }}</td>
                <td class="right num text-right">{{ fmtInt(r.inputTokens) }}</td>
                <td class="right num text-right">{{ fmtInt(r.outputTokens) }}</td>
                <td class="right num text-right">{{ fmtInt(r.cacheCreationTokens + r.cacheReadTokens) }}</td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
/**
 * UsagePanel.vue — 用量面板
 * 职责：展示用量统计的来源 Tab、KPI 卡片、调用热力图、趋势图与按日期·模型明细表格。
 */
import { computed, ref, watch } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import DateRangePicker from './DateRangePicker.vue'
import StatCard from './StatCard.vue'
import TokenHint from './TokenHint.vue'
import UsageHeatmap from './UsageHeatmap.vue'
import UsageTrendChart from './UsageTrendChart.vue'
import { fmtInt } from '@/lib/format'
import {
  isHourlyTrend,
  rangeValueUsageFilter,
  resolveTrendWindow,
  startOfTodayMs,
  type RangeValue,
} from '@/lib/range'
import { mergeByDate, sliceHourlyTrend, sliceTrend, type DayTotals } from '@/lib/usageChart'
import {
  invalidateStatsCache,
  usageApi,
  type DailyUsage,
  type DayModelUsage,
  type UsageSummary,
} from '@/api/stats'

/**
 * 用量统计：「来源」语义下对应 provider，
 * 故来源 Tab 由数据里出现过的 provider 动态生成。
 */
const app = ref<string>('all') // 当前选中的来源 Tab
const range = ref<RangeValue>({ kind: 'preset', key: 'today' }) // 日期范围筛选值
const syncing = ref(false) // 是否正在刷新
const appTypes = ref<string[]>([]) // 数据中出现的来源列表
const summary = ref<UsageSummary | null>(null) // 汇总统计
const dayModelRows = ref<DayModelUsage[]>([]) // 按日期·模型明细行
const byDayRows = ref<DailyUsage[]>([]) // 按天数据（热力图用）
const byHourRows = ref<DailyUsage[]>([]) // 按小时数据（趋势图用）

const todayStart = startOfTodayMs() // 今日零点时间戳

// 来源 Tab 列表（全部 + 动态来源）
const appTabs = computed(() => [
  { key: 'all', label: '全部' },
  ...appTypes.value.map((t) => ({ key: t, label: appLabel(t) })),
])

/** provider → 展示名。 */
function appLabel(t: string): string {
  if (t === 'claude' || t === 'anthropic') return 'Claude'
  if (t === 'codex') return 'Codex'
  if (t === 'openai') return 'OpenAI'
  return t
}

const appType = computed(() => (app.value === 'all' ? undefined : app.value)) // 实际查询用的来源过滤值
const filter = computed(() => rangeValueUsageFilter(range.value, todayStart)) // 日期范围过滤参数

// 缓存 Token 总量（创建 + 读取）
const cacheTotal = computed(
  () => (summary.value?.totalCacheCreationTokens ?? 0) + (summary.value?.totalCacheReadTokens ?? 0),
)

const trendWin = computed(() => resolveTrendWindow(range.value, todayStart)) // 趋势图时间窗口
const hourly = computed(() => isHourlyTrend(trendWin.value)) // 是否使用小时粒度趋势

const dayTotals = computed<Map<string, DayTotals>>(() => mergeByDate(byDayRows.value)) // 按日期合并的总量（热力图用）

// 趋势图数据（小时粒度或天粒度切片）
const trendData = computed(() => {
  const w = trendWin.value
  if (hourly.value && w) {
    return sliceHourlyTrend(mergeByDate(byHourRows.value), w, Date.now())
  }
  return sliceTrend(dayTotals.value, range.value, todayStart)
})

/** 日期分组结构（用于表格按日期合并行）。 */
interface DateGroup {
  date: string
  rows: DayModelUsage[]
}

/** 按日期分组（后端已按 date 倒序 + 组内 token 降序，仅需聚合连续同日期行）。 */
const groups = computed<DateGroup[]>(() => {
  const out: DateGroup[] = []
  for (const r of dayModelRows.value) {
    const last = out[out.length - 1]
    if (last && last.date === r.date) last.rows.push(r)
    else out.push({ date: r.date, rows: [r] })
  }
  return out
})

/** 加载筛选数据：汇总统计与按日期·模型明细。 */
async function loadFiltered(): Promise<void> {
  const f = { appType: appType.value, ...filter.value }
  const [s, dm] = await Promise.all([usageApi.getSummary(f), usageApi.getByDayModel(f)])
  summary.value = s
  dayModelRows.value = dm
}

/** 全量按天数据：热力图取近一年，趋势图按 range 前端切片（一次查询喂两张图）。 */
async function loadByDay(): Promise<void> {
  byDayRows.value = await usageApi.getByDay({ appType: appType.value })
}

/** 加载按小时数据（仅小时粒度趋势时拉取）。 */
async function loadByHour(): Promise<void> {
  const w = trendWin.value
  if (!hourly.value || !w) {
    byHourRows.value = []
    return
  }
  byHourRows.value = await usageApi.getByHour({
    appType: appType.value,
    startTs: w.startMs,
    endTs: w.endExclusiveMs - 1,
  })
}

/** 加载来源类型列表。 */
async function loadAppTypes(): Promise<void> {
  appTypes.value = await usageApi.listAppTypes()
}

/** 手动刷新：失效缓存后重新拉取所有数据。 */
async function sync(): Promise<void> {
  syncing.value = true
  try {
    invalidateStatsCache()
    await usageApi.sync()
    await Promise.all([loadAppTypes(), loadFiltered(), loadByDay(), loadByHour()])
  } catch (e) {
    ElMessage.error(`刷新失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    syncing.value = false
  }
}

// 来源或筛选变化时重新加载对应数据
watch([appType, filter], () => void loadFiltered(), { immediate: true })
watch(appType, () => void loadByDay(), { immediate: true })
watch([appType, trendWin], () => void loadByHour(), { immediate: true })
void loadAppTypes() // 初始加载来源列表
</script>

<style scoped>
.panel { gap: var(--gap-xl); }
.panel-bar { gap: var(--gap-lg); }
.panel-actions { gap: var(--gap-sm); }
.tabs.slim .tab { padding: 5px 12px; }
.kpi-row { grid-template-columns: repeat(4, 1fr); gap: var(--gap-md); }
@media (max-width: 900px) { .kpi-row { grid-template-columns: repeat(2, 1fr); } }
.sec { gap: var(--gap-sm); }
.sec-body { padding: var(--pad-card); }
.table tbody tr { cursor: default; }
.date-cell { vertical-align: top; }
</style>
