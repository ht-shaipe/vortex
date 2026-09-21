<template>
  <div class="trend-wrap flex flex-col gap-[var(--gap-md)]">
    <div class="trend-head flex items-center justify-between">
      <h2 class="sec-title m-0 text-13px font-semibold text-ink-2">每日 Token 趋势图</h2>
      <div class="trend-range-tabs">
        <button
          v-for="t in RANGE_TABS"
          :key="t.key"
          type="button"
          class="trend-range-tab"
          :class="{ active: range === t.key }"
          @click="range = t.key"
        >
          {{ t.label }}
        </button>
      </div>
    </div>
    <VChart class="trend-chart h-240px w-full" :option="option" autoresize />
  </div>
</template>

<script setup lang="ts">
/**
 * DailyTokenTrend.vue — 每日 Token 趋势图
 * 职责：以 ECharts 多折线图展示各模型每日 Token 用量趋势，支持近 7 日 / 近 30 日切换。
 */
import { computed, ref } from 'vue'
import VChart from 'vue-echarts'
import { use } from 'echarts/core'
import { LineChart } from 'echarts/charts'
import { GridComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { fmtCompact } from '@/lib/format'
import { CHART_PALETTE, modelColorIndex } from '@/lib/chartPalette'
import { useThemeColors } from '@/composables/useThemeColors'

use([LineChart, GridComponent, TooltipComponent, CanvasRenderer])

/** 单个模型在某一天的 Token 用量 */
export interface ModelDayPoint {
  date: string
  model: string
  tokens: number
}

/** 时间范围选项 */
type TrendRange = '7d' | '30d'

const RANGE_TABS: { key: TrendRange; label: string }[] = [
  { key: '7d', label: '近 7 日' },
  { key: '30d', label: '近 30 日' },
]

const props = defineProps<{
  /** 按日×模型聚合的 Token 用量数据 */
  data: ModelDayPoint[]
  /** 当天 0 点毫秒时间戳（与父组件保持一致的日期锚点） */
  todayStart: number
}>()

const range = ref<TrendRange>('7d')
const colors = useThemeColors()

/** 将 YYYY-MM-DD 转为 "M月D日" 格式 */
function fmtDateLabel(date: string): string {
  const [, m, d] = date.split('-').map(Number)
  return `${m}月${d}日`
}

const option = computed(() => {
  const c = colors.value
  const points = props.data

  // 收集所有日期（升序）和所有模型
  const dateSet = new Set<string>()
  const modelSet = new Set<string>()
  for (const p of points) {
    dateSet.add(p.date)
    modelSet.add(p.model)
  }
  const dates = [...dateSet].sort()
  const models = [...modelSet].sort()

  // 按时间范围过滤日期（使用父组件传入的稳定日期锚点）
  const today = new Date(props.todayStart)
  const todayStr = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`
  const rangeDays = range.value === '7d' ? 7 : 30
  const startDate = new Date(today)
  startDate.setDate(today.getDate() - rangeDays + 1)
  const startStr = `${startDate.getFullYear()}-${String(startDate.getMonth() + 1).padStart(2, '0')}-${String(startDate.getDate()).padStart(2, '0')}`
  const filteredDates = dates.filter((d) => d >= startStr && d <= todayStr)
  // 防御：窗口内无任何日期（如数据都早于窗口）时回退显示全部日期，避免图表空白
  const xAxisDates = filteredDates.length > 0 ? filteredDates : dates

  // 为每个模型构建数据序列
  const series = models.map((model) => {
    const modelPoints = points.filter((p) => p.model === model)
    const pointMap = new Map(modelPoints.map((p) => [p.date, p.tokens]))
    const color = CHART_PALETTE[modelColorIndex(model)]
    return {
      name: model,
      type: 'line' as const,
      smooth: true,
      symbol: 'none',
      data: xAxisDates.map((d) => pointMap.get(d) ?? 0),
      lineStyle: { width: 2, color },
      itemStyle: { color },
      emphasis: { focus: 'series' as const },
    }
  })

  return {
    animationDuration: 300,
    grid: { left: 8, right: 14, top: 12, bottom: 4, containLabel: true },
    tooltip: {
      trigger: 'axis',
      backgroundColor: c.surface,
      borderColor: c.line,
      borderWidth: 1,
      textStyle: { color: c.ink, fontSize: 12 },
      axisPointer: {
        type: 'line',
        lineStyle: { color: c.accent, type: 'dashed', width: 1, opacity: 0.4 },
      },
      // 悬停时才显示模型名：过滤 0 值、按用量降序，最多列 Top 10，其余合并汇总
      formatter: (params: unknown) => {
        const arr = params as { seriesName: string; dataIndex: number; value: number; color: string }[]
        if (!arr?.length) return ''
        const dateLabel = fmtDateLabel(xAxisDates[arr[0].dataIndex])
        const active = arr.filter((p) => p.value > 0).sort((a, b) => b.value - a.value)
        if (!active.length) return ''
        const TOP_N = 10
        const shown = active.slice(0, TOP_N)
        const rest = active.length - shown.length
        let html = `<div style="font-weight:600;margin-bottom:4px">${dateLabel}</div>`
        for (const p of shown) {
          html += `<div style="display:flex;align-items:center;gap:6px;margin:2px 0">
            <span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:${p.color}"></span>
            <span>${p.seriesName}：${fmtCompact(p.value)}</span>
          </div>`
        }
        if (rest > 0) {
          html += `<div style="margin:4px 0 0;color:${c.ink3};font-size:11px">其他 ${rest} 个模型</div>`
        }
        return html
      },
    },
    xAxis: {
      type: 'category',
      data: xAxisDates.map(fmtDateLabel),
      boundaryGap: false,
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: c.ink4, fontSize: 11, hideOverlap: true },
    },
    yAxis: {
      type: 'value',
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: c.ink4, fontSize: 11, formatter: (v: number) => fmtCompact(v) },
      splitLine: { lineStyle: { color: c.line, type: 'dashed' } },
    },
    series,
  }
})
</script>

<style scoped>
.trend-range-tabs {
  display: flex;
  gap: 2px;
  background: var(--surface-2, rgba(255,255,255,0.04));
  border-radius: 6px;
  padding: 2px;
}
.trend-range-tab {
  padding: 4px 12px;
  font-size: 12px;
  color: var(--ink-3, #6b6b76);
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}
.trend-range-tab:hover {
  color: var(--ink-2, #9a9aa6);
}
.trend-range-tab.active {
  background: var(--accent, #6d5bd0);
  color: #fff;
  font-weight: 500;
}
</style>
