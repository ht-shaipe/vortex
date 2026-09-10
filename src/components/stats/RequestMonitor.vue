<template>
  <section v-if="!hidden" class="mon">
    <div class="mon-head">
      <h2 class="sec-title">{{ title ?? (mode === 'live' ? '实时请求监控' : '端点请求记录') }}</h2>
      <div class="mon-actions">
        <DateRangePicker
          v-if="mode === 'ranged' && !range"
          v-model="ownRange"
        />
      </div>
    </div>

    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="items.length === 0" class="hint">暂无请求记录</p>
    <div v-else class="card table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th>时间</th>
            <th>端点</th>
            <th>入站</th>
            <th style="width: 88px">状态</th>
            <th style="width: 132px">模型</th>
            <th class="right">用时</th>
            <th class="right">首字</th>
            <th class="right">Token</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in items" :key="r.id || r.ts">
            <td class="num nowrap" :title="new Date(r.ts).toLocaleString()">
              {{ fmtDateTime(r.ts) }}
            </td>
            <td class="ellipsis">{{ r.endpointName }}</td>
            <td class="mono small">{{ inferPath(r.inboundFormat) }}</td>
            <td>
              <span class="pill" :class="statusTone(r.statusCode)">
                <span class="dot" />
                <span class="tnum">{{ r.statusCode ?? 'ERR' }}</span>
              </span>
            </td>
            <td class="small ellipsis" :title="r.model ?? ''">{{ r.model || '—' }}</td>
            <td class="right num small">
              {{ !r.isError && r.durationMs != null ? formatDuration(r.durationMs) : '—' }}
            </td>
            <td class="right num small">
              {{ !r.isError && r.firstByteMs != null ? formatDuration(r.firstByteMs) : '—' }}
            </td>
            <td class="right">
              <el-tooltip placement="left" :show-after="80">
                <template #content>
                  <div class="tok">
                    <div v-if="r.model" class="tok-model">模型：{{ r.model }}</div>
                    <div class="tok-row"><span>输入</span><span>{{ formatTokenK(r.inputTokens) }}</span></div>
                    <div class="tok-row"><span>输出</span><span>{{ formatTokenK(r.outputTokens) }}</span></div>
                    <div class="tok-row"><span>缓存创建</span><span>{{ formatTokenK(r.cacheCreationTokens) }}</span></div>
                    <div class="tok-row"><span>缓存读取</span><span>{{ formatTokenK(r.cacheReadTokens) }}</span></div>
                    <div class="tok-row total"><span>合计</span><span>{{ formatTokenK(totalTokens(r)) }}</span></div>
                    <div v-if="r.errorBody" class="tok-err">错误：{{ r.errorBody }}</div>
                  </div>
                </template>
                <span class="tok-trigger tnum">
                  {{ fmtInt(totalTokens(r)) }}
                  <el-icon :size="12"><InfoFilled /></el-icon>
                </span>
              </el-tooltip>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <Pagination v-if="total > pageSize" v-model:page="page" :page-size="pageSize" :total="total" />
  </section>
</template>

<script setup lang="ts">
/**
 * RequestMonitor.vue — 请求监控
 * 职责：以表格展示请求日志列表，支持实时/时间段两种模式、分页、Token 明细 tooltip。
 */
import { computed, ref, watch } from 'vue'
import { InfoFilled } from '@element-plus/icons-vue'
import DateRangePicker from './DateRangePicker.vue'
import Pagination from './Pagination.vue'
import { fmtInt, formatDuration, formatTokenK } from '@/lib/format'
import { rangeValueMs, startOfTodayMs, type RangeValue } from '@/lib/range'
import { statsApi, type RequestLog } from '@/api/stats'

// Props 定义：mode 控制实时/时间段模式，range 为受控时间范围，hideWhenEmpty 空数据时隐藏，endpointFilter 端点过滤，pageSize 分页大小，title 自定义标题，refreshKey 外部刷新键
const props = withDefaults(
  defineProps<{
    /** live：进入即拉最新；ranged：时间段 + 分页查询。 */
    mode: 'live' | 'ranged'
    /** 受控时间范围：传入后隐藏内置日期选择器，跟随外部筛选。 */
    range?: RangeValue
    /** 当前范围无记录时整块隐藏（含标题）。 */
    hideWhenEmpty?: boolean
    endpointFilter?: string
    pageSize?: number
    title?: string
    /** 外部数据版本号，变化时重新拉取。 */
    refreshKey?: number
  }>(),
  { pageSize: 20, hideWhenEmpty: false, refreshKey: 0 },
)

const page = ref(1) // 当前页码
const ownRange = ref<RangeValue>({ kind: 'preset', key: 'today' }) // 内置日期范围（无外部受控时使用）
const loading = ref(true) // 是否正在加载
const items = ref<RequestLog[]>([]) // 请求日志列表
const total = ref(0) // 总记录数

const rangeValue = computed<RangeValue>(() => props.range ?? ownRange.value) // 实际生效的时间范围
const hidden = computed(() => props.hideWhenEmpty && !loading.value && total.value === 0) // 是否整块隐藏

/** 加载请求日志数据。 */
async function load(): Promise<void> {
  loading.value = true
  try {
    // 按天对齐的稳定锚点：同一天内多次渲染得到相同区间
    const todayStart = startOfTodayMs()
    const win = props.mode === 'ranged' ? rangeValueMs(rangeValue.value, todayStart) : {}
    const res = await statsApi.getRequestLogs({
      startMs: win.startMs,
      endMs: win.endMs,
      endpoint: props.endpointFilter,
      page: page.value,
      pageSize: props.pageSize,
    })
    items.value = res.items
    total.value = res.total
  } finally {
    loading.value = false
  }
}

// 范围变化（含外部受控变化）时回到第 1 页
watch(rangeValue, () => {
  page.value = 1
  void load()
})
watch([page, () => props.refreshKey, () => props.endpointFilter], () => void load())
void load() // 初始加载

/** 计算请求的总 Token 数。 */
function totalTokens(r: RequestLog): number {
  return r.inputTokens + r.outputTokens + r.cacheCreationTokens + r.cacheReadTokens
}

/** 根据状态码推断色调。 */
function statusTone(code: number | null): 'ok' | 'warn' | 'err' {
  if (code == null) return 'err'
  if (code < 300) return 'ok'
  if (code < 400) return 'warn'
  return 'err'
}

/** 按入站协议推断路由（明细未落库真实路径时的兜底）。 */
function inferPath(format: string): string {
  if (format === 'openai') return '/v1/chat/completions'
  if (format === 'responses') return '/v1/responses'
  if (format === 'claude') return '/v1/messages'
  if (format === 'images') return '/v1/images/generations'
  return '—'
}

/** 格式化时间戳为可读日期时间字符串。 */
function fmtDateTime(ts: number): string {
  const d = new Date(ts)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}
</script>

<style scoped>
.mon { display: flex; flex-direction: column; gap: var(--gap-md); }
.mon-head { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--gap-md); }
.mon-actions { display: flex; align-items: center; gap: var(--gap-sm); flex-shrink: 0; }
.sec-title { margin: 0; font-size: 13px; font-weight: 600; color: var(--ink-2); }
.hint { margin: 0; font-size: var(--fs-body); color: var(--ink-4); }
.table-wrap { overflow: hidden; }
.table tbody tr { cursor: default; }
.right { text-align: right; }
.nowrap { white-space: nowrap; }
.small { font-size: var(--fs-sm); color: var(--ink-3); }
.ellipsis { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tok-trigger {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--ink-3);
  cursor: default;
}
.tok-trigger:hover { color: var(--ink); }
.tok { display: flex; flex-direction: column; gap: 4px; font-size: var(--fs-sm); min-width: 150px; }
.tok-model { color: var(--ink-3); }
.tok-row { display: flex; justify-content: space-between; gap: 16px; }
.tok-row.total { border-top: 1px solid var(--line-2); padding-top: 4px; font-weight: 600; }
.tok-err { color: var(--err); max-width: 260px; word-break: break-all; }
</style>
