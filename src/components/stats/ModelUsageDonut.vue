<template>
  <div class="donut-wrap flex items-center gap-[var(--gap-xl)]">
    <VChart class="donut-chart h-220px w-220px shrink-0" :option="option" autoresize />
    <div class="donut-legend flex flex-col gap-10px min-w-0 flex-1">
      <div v-for="m in models" :key="m.model" class="legend-row flex items-center justify-between gap-12px">
        <div class="flex items-center gap-8px min-w-0">
          <span class="legend-dot shrink-0" :style="{ background: CHART_PALETTE[modelColorIndex(m.model)] }" />
          <span class="legend-name mono text-13px truncate">{{ m.model }}</span>
        </div>
        <div class="flex items-center gap-12px shrink-0">
          <span class="legend-pct text-14px font-semibold tabular-nums text-ink-2">{{ m.pct }}%</span>
        </div>
        <div class="legend-tokens text-12px text-ink-4 tabular-nums shrink-0">{{ formatTokenCompact(m.tokens) }} tokens</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ModelUsageDonut.vue — 模型用量环形图
 * 职责：以 ECharts 环形图展示各模型 Token 用量占比，中心显示总量，右侧图例列明各模型用量与百分比。
 */
import { computed } from 'vue'
import VChart from 'vue-echarts'
import { use } from 'echarts/core'
import { PieChart } from 'echarts/charts'
import { TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { formatTokenCompact } from '@/lib/format'
import { CHART_PALETTE, modelColorIndex } from '@/lib/chartPalette'
import { useThemeColors } from '@/composables/useThemeColors'

use([PieChart, TooltipComponent, CanvasRenderer])

/** 单个模型的用量数据 */
export interface ModelUsageItem {
  model: string
  tokens: number
  pct: number
}

const props = defineProps<{
  /** 模型用量列表（已按 tokens 降序） */
  models: ModelUsageItem[]
  /** 总 Token 数 */
  total: number
}>()

const colors = useThemeColors()

const option = computed(() => {
  const c = colors.value
  const total = props.total
  return {
    animationDuration: 300,
    tooltip: {
      trigger: 'item',
      backgroundColor: c.surface,
      borderColor: c.line,
      borderWidth: 1,
      textStyle: { color: c.ink, fontSize: 12 },
      formatter: (p: { name: string; value: number; percent: number }) =>
        `${p.name}<br/>${formatTokenCompact(p.value)} tokens（${p.percent}%）`,
    },
    legend: { show: false },
    series: [
      {
        type: 'pie',
        radius: ['52%', '76%'],
        center: ['50%', '50%'],
        avoidLabelOverlap: false,
        itemStyle: { borderColor: c.surface, borderWidth: 2 },
        label: { show: false },
        emphasis: {
          label: { show: false },
          itemStyle: { shadowBlur: 10, shadowOffsetX: 0, shadowColor: 'rgba(0,0,0,0.3)' },
        },
        data: props.models.map((m) => ({
          name: m.model,
          value: m.tokens,
          itemStyle: { color: CHART_PALETTE[modelColorIndex(m.model)] },
        })),
      },
    ],
    graphic: total > 0
      ? [
          {
            type: 'text',
            left: 'center',
            top: '44%',
            style: {
              text: formatTokenCompact(total),
              fill: c.ink,
              fontSize: 18,
              fontWeight: 700,
              textAlign: 'center',
            },
          },
          {
            type: 'text',
            left: 'center',
            top: '58%',
            style: {
              text: 'tokens',
              fill: c.ink4,
              fontSize: 12,
              textAlign: 'center',
            },
          },
        ]
      : [],
  }
})
</script>

<style scoped>
.legend-dot {
  width: 10px;
  height: 10px;
  border-radius: 3px;
  display: inline-block;
}
.legend-row {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--line, rgba(255,255,255,0.06));
}
.legend-row:last-child {
  border-bottom: none;
  padding-bottom: 0;
}
</style>
