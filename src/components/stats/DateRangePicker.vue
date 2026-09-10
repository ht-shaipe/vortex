<template>
  <el-popover
    v-model:visible="open"
    :width="600"
    :placement="placement"
    trigger="click"
    popper-class="range-pop"
    @before-enter="initDraft"
  >
    <template #reference>
      <button type="button" class="btn sm range-trigger">
        <el-icon :size="13"><Calendar /></el-icon>
        {{ triggerLabel }}
      </button>
    </template>

    <!-- 快捷项：点击即生效 -->
    <div class="presets">
      <button
        v-for="p in presetList"
        :key="p.key"
        type="button"
        class="btn sm"
        :class="{ primary: activePresetKey === p.key }"
        @click="applyPreset(p)"
      >
        {{ p.label }}
      </button>
    </div>

    <div class="body">
      <!-- 左：起止时间输入 -->
      <div class="fields">
        <div
          v-for="f in fields"
          :key="f.field"
          class="field"
          :class="{ active: activeField === f.field }"
          @click="activeField = f.field"
        >
          <span class="field-label">{{ f.label }}</span>
          <div class="field-inputs">
            <input
              type="date"
              class="ipt"
              :value="ymd(f.ms)"
              @focus="activeField = f.field"
              @input="onDateInput(f.field, $event)"
            />
            <input
              type="time"
              class="ipt time"
              step="60"
              :value="fmtTimeInput(f.ms)"
              @focus="activeField = f.field"
              @input="onTimeInput(f.field, $event)"
            />
          </div>
        </div>

        <p v-if="error" class="err-text">{{ error }}</p>

        <div class="field-actions">
          <button type="button" class="btn sm" @click="open = false">取消</button>
          <button type="button" class="btn sm primary" @click="apply">确定</button>
        </div>
      </div>

      <!-- 右：月历 -->
      <div class="cal">
        <div class="cal-head">
          <button type="button" class="btn bare icon" aria-label="上个月" @click="shiftMonth(-1)">
            <el-icon :size="14"><ArrowLeft /></el-icon>
          </button>
          <span class="cal-title">{{ viewYear }}年{{ viewMonth + 1 }}月</span>
          <button type="button" class="btn bare icon" aria-label="下个月" @click="shiftMonth(1)">
            <el-icon :size="14"><ArrowRight /></el-icon>
          </button>
        </div>
        <div class="cal-week">
          <span v-for="w in WEEKDAYS" :key="w">{{ w }}</span>
        </div>
        <div class="cal-grid">
          <button
            v-for="dayMs in days"
            :key="dayMs"
            type="button"
            class="cal-day"
            :class="dayClass(dayMs)"
            @click="pickDay(dayMs)"
          >
            {{ new Date(dayMs).getDate() }}
          </button>
        </div>
      </div>
    </div>
  </el-popover>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { Calendar, ArrowLeft, ArrowRight } from '@element-plus/icons-vue'
import {
  calendarDays,
  fmtTimeInput,
  startOfDayMs,
  withDatePart,
  withDayFrom,
  withTimePart,
} from '@/lib/dateRange'
import {
  RANGE_OPTIONS,
  rangeMs,
  rangeValueEquals,
  rangeValueLabel,
  startOfTodayMs,
  ymd,
  type RangePreset,
  type RangeValue,
} from '@/lib/range'

const WEEKDAYS = ['日', '一', '二', '三', '四', '五', '六']
const DAY_MS = 86_400_000

const props = withDefaults(
  defineProps<{
    modelValue: RangeValue
    /** 自定义快捷项（默认今日/近7天/近30天/全部）。 */
    presets?: RangePreset[]
    placement?: 'bottom-end' | 'bottom-start' | 'bottom'
  }>(),
  { placement: 'bottom-end' },
)
const emit = defineEmits<{ 'update:modelValue': [v: RangeValue] }>()

/** 默认快捷项：沿用全局 RANGE_OPTIONS。 */
const DEFAULT_PRESETS: RangePreset[] = RANGE_OPTIONS.map((o) => ({
  key: o.key,
  label: o.label,
  value: () => ({ kind: 'preset', key: o.key }),
}))

const presetList = computed(() => props.presets ?? DEFAULT_PRESETS)

const open = ref(false)
const draftStart = ref(0)
const draftEnd = ref(0)
const activeField = ref<'start' | 'end'>('start')
const viewYear = ref(new Date().getFullYear())
const viewMonth = ref(new Date().getMonth())
const error = ref('')

/** 当前值命中的快捷项，触发按钮优先显示其文案。 */
const activePreset = computed(() => {
  const today = startOfTodayMs()
  return presetList.value.find((p) => rangeValueEquals(p.value(today), props.modelValue))
})
const activePresetKey = computed(() => activePreset.value?.key)
const triggerLabel = computed(() => activePreset.value?.label ?? rangeValueLabel(props.modelValue))

/** 打开时初始化草稿：自定义沿用当前值；预设换算成毫秒区间（上界取现在）。 */
function initDraft(): void {
  const now = Date.now()
  const todayStart = startOfTodayMs(now)
  let s: number
  let e: number
  if (props.modelValue.kind === 'custom') {
    s = props.modelValue.startMs
    e = props.modelValue.endMs
  } else {
    const r = rangeMs(props.modelValue.key, todayStart)
    s = r.startMs ?? todayStart - 29 * DAY_MS
    e = now
  }
  draftStart.value = s
  draftEnd.value = e
  activeField.value = 'start'
  const d = new Date(e)
  viewYear.value = d.getFullYear()
  viewMonth.value = d.getMonth()
  error.value = ''
}

const fields = computed(() => [
  { field: 'start' as const, label: '开始时间', ms: draftStart.value },
  { field: 'end' as const, label: '结束时间', ms: draftEnd.value },
])

function setField(field: 'start' | 'end', ms: number): void {
  if (field === 'start') draftStart.value = ms
  else draftEnd.value = ms
}

function onDateInput(field: 'start' | 'end', ev: Event): void {
  error.value = ''
  const cur = field === 'start' ? draftStart.value : draftEnd.value
  setField(field, withDatePart(cur, (ev.target as HTMLInputElement).value))
}

function onTimeInput(field: 'start' | 'end', ev: Event): void {
  error.value = ''
  const cur = field === 'start' ? draftStart.value : draftEnd.value
  setField(field, withTimePart(cur, (ev.target as HTMLInputElement).value))
}

const days = computed(() => calendarDays(viewYear.value, viewMonth.value))

function shiftMonth(delta: number): void {
  const d = new Date(viewYear.value, viewMonth.value + delta, 1)
  viewYear.value = d.getFullYear()
  viewMonth.value = d.getMonth()
}

function pickDay(dayMs: number): void {
  error.value = ''
  if (activeField.value === 'start') {
    draftStart.value = withDayFrom(draftStart.value, dayMs)
    // 起点晚于终点时把终点拖到同一天，保持区间有效
    if (dayMs > startOfDayMs(draftEnd.value)) draftEnd.value = withDayFrom(draftEnd.value, dayMs)
    activeField.value = 'end'
  } else if (dayMs < startOfDayMs(draftStart.value)) {
    // 终点早于起点 → 视为重选起点
    draftStart.value = withDayFrom(draftStart.value, dayMs)
  } else {
    draftEnd.value = withDayFrom(draftEnd.value, dayMs)
  }
}

function dayClass(dayMs: number): Record<string, boolean> {
  const startDay = startOfDayMs(draftStart.value)
  const endDay = startOfDayMs(draftEnd.value)
  const isStart = dayMs === startDay
  const isEnd = dayMs === endDay
  const isEndpoint = isStart || isEnd
  return {
    out: new Date(dayMs).getMonth() !== viewMonth.value,
    'in-range': dayMs >= startDay && dayMs <= endDay && !isEndpoint,
    endpoint: isEndpoint,
    today: dayMs === startOfTodayMs() && !isEndpoint,
  }
}

function applyPreset(p: RangePreset): void {
  emit('update:modelValue', p.value(startOfTodayMs()))
  open.value = false
}

function apply(): void {
  if (draftStart.value > draftEnd.value) {
    error.value = '开始时间不能晚于结束时间'
    return
  }
  emit('update:modelValue', {
    kind: 'custom',
    startMs: draftStart.value,
    endMs: draftEnd.value,
  })
  open.value = false
}
</script>

<style scoped>
.range-trigger { height: 30px; font-weight: 400; }

.presets {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line);
}

.body { display: flex; gap: var(--gap-lg); padding-top: 12px; }

.fields {
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
  width: 232px;
  flex-shrink: 0;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 9px 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: border-color 0.12s, box-shadow 0.12s;
}
.field:hover { border-color: var(--line-2); }
.field.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-bg);
}
.field-inputs { display: flex; gap: 6px; }
.ipt {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 6px;
  font-size: var(--fs-sm);
  font-family: var(--font-mono);
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 5px;
  outline: none;
}
.ipt:focus { border-color: var(--accent); }
.ipt.time { flex: 0 0 82px; }
html.dark .ipt { color-scheme: dark; }

.err-text { margin: 0; font-size: var(--fs-xs); color: var(--err); }
.field-actions {
  margin-top: auto;
  display: flex;
  justify-content: flex-end;
  gap: 6px;
}

.cal { flex: 1; display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.cal-head { display: flex; align-items: center; justify-content: space-between; }
.cal-title { font-size: 13px; font-weight: 600; }
.cal-week {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  text-align: center;
  font-size: var(--fs-xs);
  color: var(--ink-4);
}
.cal-week span { padding: 3px 0; }
.cal-grid { display: grid; grid-template-columns: repeat(7, 1fr); gap: 1px; }
.cal-day {
  height: 28px;
  border: none;
  background: transparent;
  border-radius: 5px;
  font-size: var(--fs-sm);
  color: var(--ink-2);
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.cal-day:hover { background: var(--surface-3); }
.cal-day.out { color: var(--ink-5); }
.cal-day.in-range { background: var(--accent-bg); color: var(--accent-ink); }
.cal-day.endpoint {
  background: var(--accent);
  color: #fff;
  font-weight: 600;
}
.cal-day.today { box-shadow: inset 0 0 0 1px var(--accent-line); }
</style>
