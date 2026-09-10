<template>
  <div class="trend">
    <div class="trend-head">
      <h2 class="sec-title">调用趋势</h2>
      <div class="range-tabs">
        <button
          v-for="t in METRIC_TABS"
          :key="t.key"
          type="button"
          class="range-tab"
          :class="{ active: metric === t.key }"
          @click="metric = t.key"
        >
          {{ t.label }}
        </button>
      </div>
    </div>
    <VChart class="trend-chart" :option="option" autoresize />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import VChart from 'vue-echarts'
import { use } from 'echarts/core'
import { LineChart } from 'echarts/charts'
import { GridComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { fmtCompact, fmtInt } from '@/lib/format'
import { useThemeColors } from '@/composables/useThemeColors'
import type { TrendPoint } from '@/lib/usageChart'

use([LineChart, GridComponent, TooltipComponent, CanvasRenderer])

type Metric = 'requests' | 'tokens'

const METRIC_TABS: { key: Metric; label: string }[] = [
  { key: 'requests', label: '次数' },
  { key: 'tokens', label: 'Tokens' },
]

const props = defineProps<{ data: TrendPoint[] }>()

const metric = ref<Metric>('requests')
const colors = useThemeColors()

/** 面积渐变的 rgba 化：echarts 不认 oklch，取计算值后交给 canvas 直接用。 */
const option = computed(() => {
  const c = colors.value
  const points = props.data
  const seriesName = metric.value === 'requests' ? '请求次数' : 'Tokens'
  return {
    animationDuration: 240,
    grid: { left: 8, right: 14, top: 12, bottom: 4, containLabel: true },
    tooltip: {
      trigger: 'axis',
      backgroundColor: c.surface,
      borderColor: c.line,
      borderWidth: 1,
      textStyle: { color: c.ink, fontSize: 12 },
      formatter: (params: unknown) => {
        const arr = params as { dataIndex: number; value: number }[]
        const first = arr?.[0]
        if (!first) return ''
        const p = points[first.dataIndex]
        return `<div style="font-weight:600">${p?.date ?? ''}</div>${seriesName}：${fmtInt(first.value)}`
      },
    },
    xAxis: {
      type: 'category',
      data: points.map((p) => p.label),
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
    series: [
      {
        name: seriesName,
        type: 'line',
        smooth: true,
        symbol: 'none',
        data: points.map((p) => (metric.value === 'requests' ? p.requests : p.tokens)),
        lineStyle: { width: 2, color: c.accent },
        itemStyle: { color: c.accent },
        areaStyle: {
          color: {
            type: 'linear',
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: c.seq[1] },
              { offset: 1, color: 'transparent' },
            ],
          },
          opacity: 0.45,
        },
      },
    ],
  }
})
</script>

<style scoped>
.trend { display: flex; flex-direction: column; gap: var(--gap-md); }
.trend-head { display: flex; align-items: center; justify-content: space-between; }
.sec-title { margin: 0; font-size: 13px; font-weight: 600; color: var(--ink-2); }
.trend-chart { height: 224px; width: 100%; }
</style>
