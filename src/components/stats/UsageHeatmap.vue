<template>
  <div class="heat" :style="{ gap: GAP }">
    <!-- 月份刻度行（首列占位与星期标签对齐） -->
    <div class="heat-months" :style="{ gridTemplateColumns: gridColumns, gap: GAP }">
      <span
        v-for="m in monthLabels"
        :key="m.col"
        :style="{ gridColumnStart: m.col + 2 }"
      >{{ m.text }}</span>
    </div>

    <!-- 星期标签列 + 格子：同一 grid（列流式填充，前 7 项为标签列） -->
    <div
      class="heat-grid"
      :style="{ gridTemplateColumns: gridColumns, gap: GAP }"
      @mouseleave="tooltip = null"
    >
      <span v-for="i in 7" :key="`wd-${i}`" class="heat-wd">
        {{ WEEKDAY_LABELS[i - 1] ?? '' }}
      </span>
      <div
        v-for="cell in cells"
        :key="cell.date"
        class="heat-cell"
        :style="{ backgroundColor: cellColor(cell.date) }"
        @mouseenter="showTip(cell, $event)"
      />
    </div>
  </div>

  <Teleport to="body">
    <div
      v-if="tooltip"
      class="heat-tip"
      :style="{ left: `${tooltip.x}px`, top: `${tooltip.y - 8}px` }"
    >
      <p class="heat-tip-date">{{ tooltip.cell.date }}</p>
      <div v-if="activeTotals" class="heat-tip-grid">
        <span>请求数</span><span class="tnum">{{ fmtInt(activeTotals.requests) }}</span>
        <span>输入 Token</span><span class="tnum">{{ fmtInt(activeTotals.inputTokens) }}</span>
        <span>输出 Token</span><span class="tnum">{{ fmtInt(activeTotals.outputTokens) }}</span>
        <span>缓存 Token</span><span class="tnum">{{ fmtInt(activeTotals.cacheTokens) }}</span>
      </div>
      <p v-else class="heat-tip-empty">无调用记录</p>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * UsageHeatmap.vue — 用量热力图
 * 职责：以 GitHub 风格的热力图展示近一年每日调用量，支持悬停 tooltip 显示明细。
 */
import { computed, ref } from 'vue'
import { startOfTodayMs } from '@/lib/range'
import { fmtInt } from '@/lib/format'
import { buildHeatmapCells, heatLevel, type DayTotals, type HeatmapCell } from '@/lib/usageChart'
import { useThemeColors } from '@/composables/useThemeColors'

const GAP = '3px' // 格子间距
/** 星期标签列宽（第一列），其余列 1fr 均分容器宽度、格子 aspect-square 保持正方形。 */
const LABEL_COL = '1.25rem'
const WEEKDAY_LABELS: Record<number, string> = { 0: '一', 2: '三', 4: '五', 6: '日' } // 星期标签（仅显示部分）

// Props 定义：totals 为按日期映射的每日总量数据
const props = defineProps<{ totals: Map<string, DayTotals> }>()

const colors = useThemeColors() // 主题色
const todayStart = startOfTodayMs() // 今日零点时间戳
const cells = computed(() => buildHeatmapCells(todayStart)) // 热力图格子列表

// 所有格子中的最大请求数（用于色阶映射）
const max = computed(() => {
  let m = 0
  for (const c of cells.value) m = Math.max(m, props.totals.get(c.date)?.requests ?? 0)
  return m
})

const weekCount = computed(() => Math.ceil(cells.value.length / 7)) // 总周数
const gridColumns = computed(() => `${LABEL_COL} repeat(${weekCount.value}, minmax(0, 1fr))`) // grid 列模板

/** 每列（周）首格所在月份变化时打月份刻度。 */
const monthLabels = computed(() => {
  const labels: { col: number; text: string }[] = []
  let prevMonth = -1
  for (let w = 0; w < weekCount.value; w++) {
    const first = cells.value[w * 7]
    if (!first) continue
    const month = new Date(first.ms).getMonth()
    if (month !== prevMonth) {
      labels.push({ col: w, text: `${month + 1}月` })
      prevMonth = month
    }
  }
  return labels
})

/** 0 档用面板底色，1–4 档走铁锈橙顺序色阶。 */
function cellColor(date: string): string {
  const level = heatLevel(props.totals.get(date)?.requests ?? 0, max.value)
  if (level === 0) return colors.value.surface3
  return colors.value.seq[level - 1]
}

/** Tooltip 状态结构。 */
interface TooltipState {
  cell: HeatmapCell
  x: number
  y: number
}
const tooltip = ref<TooltipState | null>(null) // 当前悬停的 tooltip 状态
const activeTotals = computed(() =>
  tooltip.value ? props.totals.get(tooltip.value.cell.date) : undefined,
) // 当前悬停格子的每日总量

/** 显示 tooltip：记录格子信息与定位坐标。 */
function showTip(cell: HeatmapCell, ev: MouseEvent): void {
  const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect()
  tooltip.value = { cell, x: rect.left + rect.width / 2, y: rect.top }
}
</script>

<style scoped>
.heat { display: flex; flex-direction: column; }
.heat-months {
  display: grid;
  height: 14px;
  font-size: 10px;
  line-height: 1;
  color: var(--ink-4);
}
.heat-months span { white-space: nowrap; }
.heat-grid {
  display: grid;
  grid-template-rows: repeat(7, auto);
  grid-auto-flow: column;
}
.heat-wd {
  display: flex;
  align-items: center;
  font-size: 10px;
  line-height: 1;
  color: var(--ink-4);
}
.heat-cell {
  aspect-ratio: 1;
  width: 100%;
  border-radius: 2px;
  transition: transform 0.1s;
}
.heat-cell:hover { transform: scale(1.35); }
</style>

<style>
/* Teleport 到 body，不能用 scoped */
.heat-tip {
  position: fixed;
  z-index: 3000;
  transform: translate(-50%, -100%);
  pointer-events: none;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  box-shadow: var(--shadow-md);
  padding: 8px 11px;
  font-size: var(--fs-sm);
  color: var(--ink);
}
.heat-tip-date { margin: 0; font-weight: 600; }
.heat-tip-grid {
  margin-top: 4px;
  display: grid;
  grid-template-columns: auto auto;
  column-gap: 14px;
  row-gap: 2px;
  color: var(--ink-3);
}
.heat-tip-grid span:nth-child(even) { text-align: right; color: var(--ink); }
.heat-tip-empty { margin: 4px 0 0; color: var(--ink-4); }
</style>
