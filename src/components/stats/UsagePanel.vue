<template>
  <div class="panel">
    <div class="panel-bar">
      <div class="tabs slim">
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
      <div class="panel-actions">
        <DateRangePicker v-model="range" />
        <button type="button" class="btn sm" :disabled="syncing" @click="sync">
          <el-icon :size="13" :class="{ spin: syncing }"><Refresh /></el-icon>刷新
        </button>
      </div>
    </div>

    <div class="kpi-row">
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

    <section class="sec">
      <h2 class="sec-title">调用热力图</h2>
      <div class="card sec-body">
        <UsageHeatmap :totals="dayTotals" />
      </div>
    </section>

    <section class="card sec-body">
      <UsageTrendChart :data="trendData" />
    </section>

    <section v-if="dayModelRows.length > 0" class="sec">
      <h2 class="sec-title">按日期 · 模型</h2>
      <div class="card table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>日期</th>
              <th>来源</th>
              <th>模型</th>
              <th class="right">请求</th>
              <th class="right">输入</th>
              <th class="right">输出</th>
              <th class="right">缓存</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="g in groups" :key="g.date">
              <tr v-for="(r, i) in g.rows" :key="`${g.date}-${r.appType}-${r.model}-${i}`">
                <td v-if="i === 0" :rowspan="g.rows.length" class="num date-cell">{{ g.date }}</td>
                <td class="small">{{ r.appType || '—' }}</td>
                <td class="mono small">{{ r.model || '—' }}</td>
                <td class="right num">{{ fmtInt(r.requests) }}</td>
                <td class="right num">{{ fmtInt(r.inputTokens) }}</td>
                <td class="right num">{{ fmtInt(r.outputTokens) }}</td>
                <td class="right num">{{ fmtInt(r.cacheCreationTokens + r.cacheReadTokens) }}</td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
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
 * 用量统计：ccMesh 里「来源」是本机 Claude Code / Codex 会话日志；
 * vortex 语义下对应 provider，故来源 Tab 由数据里出现过的 provider 动态生成。
 */
const app = ref<string>('all')
const range = ref<RangeValue>({ kind: 'preset', key: 'today' })
const syncing = ref(false)
const appTypes = ref<string[]>([])
const summary = ref<UsageSummary | null>(null)
const dayModelRows = ref<DayModelUsage[]>([])
const byDayRows = ref<DailyUsage[]>([])
const byHourRows = ref<DailyUsage[]>([])

const todayStart = startOfTodayMs()

const appTabs = computed(() => [
  { key: 'all', label: '全部' },
  ...appTypes.value.map((t) => ({ key: t, label: appLabel(t) })),
])

/** provider → 展示名（保留 ccMesh 对已知客户端的中文命名习惯）。 */
function appLabel(t: string): string {
  if (t === 'claude' || t === 'anthropic') return 'Claude'
  if (t === 'codex') return 'Codex'
  if (t === 'openai') return 'OpenAI'
  return t
}

const appType = computed(() => (app.value === 'all' ? undefined : app.value))
const filter = computed(() => rangeValueUsageFilter(range.value, todayStart))

const cacheTotal = computed(
  () => (summary.value?.totalCacheCreationTokens ?? 0) + (summary.value?.totalCacheReadTokens ?? 0),
)

const trendWin = computed(() => resolveTrendWindow(range.value, todayStart))
const hourly = computed(() => isHourlyTrend(trendWin.value))

const dayTotals = computed<Map<string, DayTotals>>(() => mergeByDate(byDayRows.value))

const trendData = computed(() => {
  const w = trendWin.value
  if (hourly.value && w) {
    return sliceHourlyTrend(mergeByDate(byHourRows.value), w, Date.now())
  }
  return sliceTrend(dayTotals.value, range.value, todayStart)
})

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

async function loadAppTypes(): Promise<void> {
  appTypes.value = await usageApi.listAppTypes()
}

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

watch([appType, filter], () => void loadFiltered(), { immediate: true })
watch(appType, () => void loadByDay(), { immediate: true })
watch([appType, trendWin], () => void loadByHour(), { immediate: true })
void loadAppTypes()
</script>

<style scoped>
.panel { display: flex; flex-direction: column; gap: var(--gap-xl); }
.panel-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-lg);
  flex-wrap: wrap;
}
.panel-actions { display: flex; align-items: center; gap: var(--gap-sm); }
.tabs.slim { margin-bottom: 0; border-bottom: none; }
.tabs.slim .tab { padding: 5px 12px; }
.kpi-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: var(--gap-md); }
@media (max-width: 900px) { .kpi-row { grid-template-columns: repeat(2, 1fr); } }
.sec { display: flex; flex-direction: column; gap: var(--gap-sm); }
.sec-title { margin: 0; font-size: 13px; font-weight: 600; color: var(--ink-2); }
.sec-body { padding: var(--pad-card); }
.table-wrap { overflow: hidden; }
.table tbody tr { cursor: default; }
.right { text-align: right; }
.small { font-size: var(--fs-sm); color: var(--ink-3); }
.date-cell { vertical-align: top; border-right: 1px solid var(--line); }
</style>
