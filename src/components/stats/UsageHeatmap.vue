<template>
  <div class="heat flex flex-col" :style="{ gap: GAP }">
    <!-- 月份刻度行（首列占位与星期标签对齐） -->
    <div class="heat-months grid h-14px text-10px leading-1 text-ink-4" :style="{ gridTemplateColumns: gridColumns, gap: GAP }">
      <span
        v-for="m in monthLabels"
        :key="m.col"
        class="whitespace-nowrap"
        :style="{ gridColumnStart: m.col + 2 }"
      >{{ m.text }}</span>
    </div>

    <!-- 星期标签列 + 格子：同一 grid（列流式填充，前 7 项为标签列） -->
    <div
      class="heat-grid grid"
      :style="{ gridTemplateColumns: gridColumns, gap: GAP }"
      @mouseleave="tooltip = null"
    >
      <span v-for="i in 7" :key="`wd-${i}`" class="heat-wd flex items-center text-10px leading-1 text-ink-4">
        {{ WEEKDAY_LABELS[i - 1] ?? '' }}
      </span>
      <div
        v-for="cell in cells"
        :key="cell.date"
        class="heat-cell aspect-square w-full rounded-2px"
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

/** 显示 tooltip：记录格子信息与定位坐标，做屏幕边界检测避免被裁剪。 */
const TIP_WIDTH = 172 // 与 CSS min-width 保持一致
function showTip(cell: HeatmapCell, ev: MouseEvent): void {
  const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect()
  // 以格子水平中心为基准
  let x = rect.left + rect.width / 2
  const half = TIP_WIDTH / 2
  // 左边界保护
  if (x - half < 4) x = 4 + half
  // 右边界保护
  if (x + half > window.innerWidth - 4) x = window.innerWidth - 4 - half
  tooltip.value = { cell, x, y: rect.top }
}
</script>

<style scoped>
.heat-grid {
  grid-template-rows: repeat(7, auto);
  grid-auto-flow: column;
}
.heat-cell { transition: transform 0.1s; }
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
  padding: 9px 12px;
  font-size: var(--fs-sm);
  color: var(--ink);
  min-width: 168px;
  max-width: 240px;
}
.heat-tip-date {
  margin: 0 0 2px;
  font-weight: 600;
  font-size: 13px;
  white-space: nowrap;
  color: var(--ink);
}
.heat-tip-grid {
  margin-top: 4px;
  display: grid;
  grid-template-columns: 1fr auto;
  column-gap: 16px;
  row-gap: 3px;
  color: var(--ink-3);
  font-size: 12.5px;
}
.heat-tip-grid span { white-space: nowrap; }
.heat-tip-grid span:nth-child(even) {
  text-align: right;
  color: var(--ink);
  font-variant-numeric: tabular-nums;
  font-weight: 500;
}
.heat-tip-empty { margin: 4px 0 0; color: var(--ink-4); white-space: nowrap; }
</style>
