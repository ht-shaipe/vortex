<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="统计" :sub="tab === 'latency' ? '按时间范围统计请求延迟与首 Token 延迟（TTFT）分位数，定位慢提供方' : '端点调用与 Token 用量的多维分析'" />

    <!-- 按选中标签渲染对应统计面板：顶部切换标签通过插槽并入面板操作栏，与操作按钮同一行 -->
    <EndpointStatsPanel v-if="tab === 'endpoint'">
      <template #top-tabs>
        <div class="tabs mb-0 border-b-none">
          <button
            v-for="t in TOP_TABS"
            :key="t.key"
            type="button"
            class="tab"
            :class="{ active: tab === t.key }"
            @click="tab = t.key"
          >
            {{ t.label }}
          </button>
        </div>
      </template>
    </EndpointStatsPanel>
    <UsagePanel v-else-if="tab === 'usage'">
      <template #top-tabs>
        <div class="tabs mb-0 border-b-none">
          <button
            v-for="t in TOP_TABS"
            :key="t.key"
            type="button"
            class="tab"
            :class="{ active: tab === t.key }"
            @click="tab = t.key"
          >
            {{ t.label }}
          </button>
        </div>
      </template>
    </UsagePanel>
    <div v-if="tab === 'latency'">
      <div class="panel-bar flex items-center justify-between flex-wrap gap-[var(--gap-lg)] mb-[var(--gap-lg)]">
        <div class="tabs mb-0 border-b-none">
          <button
            v-for="t in TOP_TABS"
            :key="t.key"
            type="button"
            class="tab"
            :class="{ active: tab === t.key }"
            @click="tab = t.key"
          >
            {{ t.label }}
          </button>
        </div>
        <div class="flex items-center gap-4px" role="group" aria-label="延迟分析时间范围">
          <button
            v-for="r in latencyRanges"
            :key="r.value"
            type="button"
            class="btn sm"
            :class="{ primary: latencyRange === r.value }"
            :aria-pressed="latencyRange === r.value"
            @click="latencyRange = r.value"
          >{{ r.label }}</button>
          <button type="button" class="btn sm" :disabled="latencyLoading" @click="loadLatency">
            <el-icon :size="13"><Refresh /></el-icon>刷新
          </button>
        </div>
      </div>

      <div v-if="latencyLoading" class="text-center text-ink-4 py-40px">
        <el-icon class="spin" :size="18"><Loading /></el-icon>
      </div>
      <div v-else-if="latencyError" class="card py-24px text-center text-ink-3" role="alert">
        {{ latencyError }}
        <button type="button" class="btn sm ml-12px" @click="loadLatency">重试</button>
      </div>
      <!-- 空态 -->
      <div v-else-if="latencyData && latencyData.latency.count === 0" class="card">
        <EmptyState title="暂无延迟数据" desc="产生成功转发的请求后，这里会展示延迟分位数统计（p50/p95/p99）与首 Token 延迟" />
      </div>

      <template v-else-if="latencyData">
        <!-- 总览统计卡片 -->
        <div class="grid grid-cols-4 gap-12px mb-12px">
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">P50 延迟</div>
            <div class="text-20px font-semibold tabular-nums mt-2px text-ok">{{ fmtMs(latencyData.latency.p50) }}</div>
            <div class="text-11px text-ink-5 mt-2px">一半请求快于此值</div>
          </div>
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">P95 延迟</div>
            <div class="text-20px font-semibold tabular-nums mt-2px text-warn">{{ fmtMs(latencyData.latency.p95) }}</div>
            <div class="text-11px text-ink-5 mt-2px">5% 请求慢于此值</div>
          </div>
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">P99 延迟</div>
            <div class="text-20px font-semibold tabular-nums mt-2px text-err">{{ fmtMs(latencyData.latency.p99) }}</div>
            <div class="text-11px text-ink-5 mt-2px">尾部极端延迟</div>
          </div>
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">样本数</div>
            <div class="text-20px font-semibold tabular-nums mt-2px">{{ latencyData.latency.count }}</div>
            <div class="text-11px text-ink-5 mt-2px">成功请求（当前范围）</div>
          </div>
        </div>

        <!-- 明细：总延迟 + TTFT -->
        <div class="grid grid-cols-2 gap-12px mb-12px">
          <!-- 延迟明细 -->
          <div class="card">
            <div class="card-head py-10px px-16px border-b border-line border-solid border-0">
              <div class="card-title text-13.5px">请求延迟明细</div>
              <div class="card-sub text-11.5px">从发起请求到响应完成的耗时</div>
            </div>
            <div class="card-body py-10px px-16px">
              <div class="stat-line flex items-center justify-between py-6px">
                <span class="text-12px text-ink-3">最快 / 最慢</span>
                <span class="mono text-12.5px tabular-nums">{{ fmtMs(latencyData.latency.min) }} / {{ fmtMs(latencyData.latency.max) }}</span>
              </div>
              <div class="stat-line flex items-center justify-between py-6px">
                <span class="text-12px text-ink-3">平均值</span>
                <span class="mono text-12.5px tabular-nums">{{ fmtMs(latencyData.latency.avg) }}</span>
              </div>
              <div class="stat-line flex items-center justify-between py-6px">
                <span class="text-12px text-ink-3">P50 / P95 / P99</span>
                <span class="mono text-12.5px tabular-nums">{{ fmtMs(latencyData.latency.p50) }} / {{ fmtMs(latencyData.latency.p95) }} / {{ fmtMs(latencyData.latency.p99) }}</span>
              </div>
            </div>
          </div>

          <!-- TTFT 明细 -->
          <div class="card">
            <div class="card-head py-10px px-16px border-b border-line border-solid border-0">
              <div class="card-title text-13.5px">首 Token 延迟（TTFT）</div>
              <div class="card-sub text-11.5px">流式请求从发出到收到第一个 Token 的耗时，体感最敏感</div>
            </div>
            <div class="card-body py-10px px-16px">
              <template v-if="latencyData.ttft.p50 > 0">
                <div class="stat-line flex items-center justify-between py-6px">
                  <span class="text-12px text-ink-3">P50 / P95</span>
                  <span class="mono text-12.5px tabular-nums">{{ fmtMs(latencyData.ttft.p50) }} / {{ fmtMs(latencyData.ttft.p95) }}</span>
                </div>
                <div class="stat-line flex items-center justify-between py-6px">
                  <span class="text-12px text-ink-3">平均值</span>
                  <span class="mono text-12.5px tabular-nums">{{ fmtMs(latencyData.ttft.avg) }}</span>
                </div>
              </template>
              <div v-else class="text-12px text-ink-4 py-10px">暂无 TTFT 数据（非流式请求或历史数据无该字段）</div>
            </div>
          </div>
        </div>

        <!-- 压缩内容回忆 -->
        <div class="card">
          <div class="px-16px py-10px border-b border-line border-solid border-0">
            <div class="card-title text-13.5px">压缩内容回忆</div>
            <div class="card-sub text-11.5px">被 Prompt 压缩的原始内容记录，点击行查看完整原文</div>
          </div>
          <div v-if="compContentLoading" class="py-20px text-center text-ink-4"><el-icon class="spin" :size="14"><Loading /></el-icon></div>
          <div v-else-if="compContentList.length > 0">
            <table class="table">
              <thead><tr><th>Hash</th><th class="num">节省 Tokens</th><th>模型</th><th>时间</th></tr></thead>
              <tbody>
                <tr v-for="c in compContentList" :key="c.hash" class="cursor-pointer" @click="showCompContent(c.hash)">
                  <td class="mono text-11.5px text-ink-3">{{ c.hash.slice(0, 16) }}…</td>
                  <td class="num mono text-12.5px tabular-nums text-ok">{{ c.savedTokens }}</td>
                  <td class="mono text-12px">{{ c.model || '—' }}</td>
                  <td class="text-11.5px text-ink-4">{{ c.createdAt.slice(0, 19) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <EmptyState v-else title="暂无压缩记录" desc="Prompt 压缩发生后，这里会展示被压缩的原始内容与节省的 Token 数" />
        </div>

        <!-- 按提供方 -->
        <div class="card">
          <div class="card-head py-10px px-16px border-b border-line border-solid border-0">
            <div class="card-title text-13.5px">按提供方</div>
            <div class="card-sub text-11.5px">平均延迟从低到高排序，便于对比各提供方的响应速度</div>
          </div>
          <table class="table">
            <thead>
              <tr>
                <th>提供方</th>
                <th class="num">平均延迟</th>
                <th class="num">请求数</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="p in sortedProviders" :key="p.provider">
                <td>
                  <div class="flex items-center gap-8px">
                    <ProviderLogo :name="p.provider" :size="18" />
                    <span class="mono text-12.5px">{{ p.provider }}</span>
                  </div>
                </td>
                <td class="num mono text-12.5px tabular-nums">{{ fmtMs(p.avg) }}</td>
                <td class="num mono text-12.5px tabular-nums">{{ p.count }}</td>
              </tr>
              <tr v-if="sortedProviders.length === 0">
                <td colspan="3" class="text-center text-ink-4 py-20px text-12.5px">当前范围无数据</td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </div>

    <!-- 成本分析 -->
    <div v-if="tab === 'cost'">
      <div class="panel-bar flex items-center justify-between flex-wrap gap-[var(--gap-lg)] mb-[var(--gap-lg)]">
        <div class="tabs mb-0 border-b-none">
          <button v-for="t in TOP_TABS" :key="t.key" type="button" class="tab" :class="{ active: tab === t.key }" @click="tab = t.key">{{ t.label }}</button>
        </div>
        <button type="button" class="btn sm" :disabled="costLoading" @click="loadCost">
          <el-icon :size="13"><Refresh /></el-icon>刷新
        </button>
      </div>
      <div v-if="costLoading" class="text-center text-ink-4 py-40px"><el-icon class="spin" :size="18"><Loading /></el-icon></div>
      <template v-else-if="costData">
        <div class="grid grid-cols-3 gap-12px mb-12px">
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">总支出</div>
            <div class="text-20px font-semibold tabular-nums mt-2px">${{ costData.totalCost.toFixed(2) }}</div>
          </div>
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">压缩节省 Tokens</div>
            <div class="text-20px font-semibold tabular-nums mt-2px text-ok">{{ formatTokenCompact(costData.totalSavedTokens) }}</div>
          </div>
          <div class="card py-14px px-18px">
            <div class="text-11.5px text-ink-4">总请求数</div>
            <div class="text-20px font-semibold tabular-nums mt-2px">{{ costData.totalRequests }}</div>
          </div>
        </div>
        <div class="card mb-12px">
          <div class="card-head py-10px px-16px border-b border-line border-solid border-0">
            <div class="card-title text-13.5px">按模型成本</div>
          </div>
          <table class="table">
            <thead><tr><th>模型</th><th class="num">请求数</th><th class="num">成本</th><th class="num">输入 Tokens</th><th class="num">输出 Tokens</th><th class="num">节省 Tokens</th></tr></thead>
            <tbody>
              <tr v-for="(m, idx) in costData.byModel" :key="m.model || idx">
                <td class="mono text-12.5px">{{ m.model || '—' }}</td>
                <td class="num mono text-12.5px tabular-nums">{{ m.requests }}</td>
                <td class="num mono text-12.5px tabular-nums">${{ m.cost.toFixed(4) }}</td>
                <td class="num mono text-12.5px tabular-nums">{{ formatTokenCompact(m.tokensInput) }}</td>
                <td class="num mono text-12.5px tabular-nums">{{ formatTokenCompact(m.tokensOutput) }}</td>
                <td class="num mono text-12.5px tabular-nums text-ok">{{ m.savedTokens > 0 ? formatTokenCompact(m.savedTokens) : '—' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </div>

    <!-- 优化建议 -->
    <div v-if="tab === 'recs'">
      <div class="panel-bar flex items-center justify-between flex-wrap gap-[var(--gap-lg)] mb-[var(--gap-lg)]">
        <div class="tabs mb-0 border-b-none">
          <button v-for="t in TOP_TABS" :key="t.key" type="button" class="tab" :class="{ active: tab === t.key }" @click="tab = t.key">{{ t.label }}</button>
        </div>
        <button type="button" class="btn sm" :disabled="recsLoading" @click="loadRecs">
          <el-icon :size="13"><Refresh /></el-icon>刷新
        </button>
      </div>
      <div v-if="recsLoading" class="text-center text-ink-4 py-40px"><el-icon class="spin" :size="18"><Loading /></el-icon></div>
      <div v-else-if="recsData && recsData.length > 0" class="flex flex-col gap-8px">
        <div v-for="(r, i) in recsData" :key="i" class="card py-12px px-16px flex items-start gap-12px">
          <div class="text-18px mt--2px" :class="{ 'text-err': r.severity === 'high', 'text-warn': r.severity === 'medium', 'text-ok': r.severity === 'info', 'text-ink-3': r.severity === 'low' }">
            <el-icon><WarningFilled v-if="r.severity === 'high'" /><InfoFilled v-else /></el-icon>
          </div>
          <div class="flex-1">
            <div class="text-13px">{{ r.message }}</div>
            <div class="text-11px text-ink-5 mt-2px">{{ r.type }}</div>
          </div>
        </div>
      </div>
      <div v-else class="card"><EmptyState title="暂无优化建议" desc="产生足够的请求后，系统会自动分析使用模式并给出优化建议" /></div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 统计页面。
 * 职责：作为统计模块的容器，在「端点统计」「用量统计」「延迟分析」三个子面板间切换。
 */
import { ref, watch, computed, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Loading, Refresh, WarningFilled, InfoFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import EndpointStatsPanel from '@/components/stats/EndpointStatsPanel.vue'
import UsagePanel from '@/components/stats/UsagePanel.vue'
import { latencyStatsApi, type LatencyStatsResponse } from '@/api/latencyStats'
import { analyticsApi, type CostAnalysis, type Recommendation } from '@/api/analytics'
import { compressedContentApi, type CompressedContentEntry } from '@/api/compressedContent'
import { formatTokenCompact } from '@/lib/format'
import { ElMessage } from 'element-plus'

const TOP_TABS = [
  { key: 'endpoint', label: '端点统计' },
  { key: 'usage', label: '用量统计' },
  { key: 'latency', label: '延迟分析' },
  { key: 'cost', label: '成本分析' },
  { key: 'recs', label: '优化建议' },
] as const

type TopKey = (typeof TOP_TABS)[number]['key']

const route = useRoute()
const router = useRouter()
// 标签写入地址，刷新页面及旧入口跳转后仍定位到对应面板。
const tab = computed<TopKey>({
  get: () => TOP_TABS.find(t => t.key === route.query.tab)?.key ?? 'endpoint',
  set: (value) => { void router.replace({ query: { ...route.query, tab: value } }) },
})

// 时间范围选项
const latencyRanges = [
  { label: '24小时', value: '24h' },
  { label: '7天', value: '7d' },
  { label: '30天', value: '30d' },
  { label: '全部', value: '' },
] as const
const latencyRange = ref<string>('24h')
// 延迟分析数据
const latencyData = ref<LatencyStatsResponse | null>(null)
const latencyLoading = ref(false)
const latencyError = ref('')
let latencyRequestId = 0

/** 提供方按平均延迟升序 */
const sortedProviders = computed(() =>
  [...(latencyData.value?.byProvider ?? [])].sort((a, b) => a.avg - b.avg),
)

/** 格式化毫秒耗时：1000ms 以下显示 ms，否则折算秒。 */
function fmtMs(v: number): string {
  if (!Number.isFinite(v) || v <= 0) return '—'
  return v < 1000 ? `${Math.round(v)}ms` : `${(v / 1000).toFixed(2)}s`
}

/** 计算时间范围对应的 ISO 起点（无范围为 undefined 表示全部）。 */
function sinceIso(): string | undefined {
  if (!latencyRange.value) return undefined
  const hours = latencyRange.value === '24h' ? 24 : latencyRange.value === '7d' ? 24 * 7 : 24 * 30
  return new Date(Date.now() - hours * 3600_000).toISOString()
}

/** 加载统计数据，快速切换范围时只采纳最后一次请求。 */
async function loadLatency() {
  const requestId = ++latencyRequestId
  latencyLoading.value = true
  latencyError.value = ''
  try {
    const data = await latencyStatsApi.get(sinceIso())
    if (requestId === latencyRequestId) latencyData.value = data
  } catch {
    if (requestId === latencyRequestId) {
      latencyData.value = null
      latencyError.value = '延迟统计加载失败，请重试'
    }
  } finally {
    if (requestId === latencyRequestId) latencyLoading.value = false
  }
}

watch([tab, latencyRange], ([activeTab]) => {
  if (activeTab === 'latency') void loadLatency()
  if (activeTab === 'cost') { void loadCost(); void loadCompContent() }
  if (activeTab === 'recs') void loadRecs()
}, { immediate: true })

// 成本分析
const costData = ref<CostAnalysis | null>(null)
const costLoading = ref(false)
async function loadCost() {
  costLoading.value = true
  try { costData.value = await analyticsApi.costAnalysis() }
  catch { /* ignore */ }
  finally { costLoading.value = false }
}

// 优化建议
const recsData = ref<Recommendation[]>([])
const recsLoading = ref(false)
async function loadRecs() {
  recsLoading.value = true
  try { recsData.value = (await analyticsApi.recommendations()).suggestions }
  catch { /* ignore */ }
  finally { recsLoading.value = false }
}

// 压缩内容回忆
const compContentList = ref<CompressedContentEntry[]>([])
const compContentLoading = ref(false)
async function loadCompContent() {
  compContentLoading.value = true
  try { compContentList.value = await compressedContentApi.list() }
  catch { /* ignore */ }
  finally { compContentLoading.value = false }
}

async function showCompContent(hash: string) {
  try {
    const detail = await compressedContentApi.get(hash)
    if (detail?.originalContent) {
      ElMessage.info(`原始内容 ${detail.originalContent.length} 字符，节省 ${detail.savedTokens} tokens`)
    }
  } catch { /* ignore */ }
}

onBeforeUnmount(() => { latencyRequestId++ })
</script>

<style scoped>
/* 统计行悬浮底色 */
.stat-line:hover {
  background: var(--surface-2);
  border-radius: 4px;
}
</style>
