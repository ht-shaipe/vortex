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
    <div v-else>
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
  </div>
</template>

<script setup lang="ts">
/**
 * 统计页面。
 * 职责：作为统计模块的容器，在「端点统计」「用量统计」「延迟分析」三个子面板间切换。
 */
import { ref, watch, computed, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Loading, Refresh } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import EndpointStatsPanel from '@/components/stats/EndpointStatsPanel.vue'
import UsagePanel from '@/components/stats/UsagePanel.vue'
import { latencyStatsApi, type LatencyStatsResponse } from '@/api/latencyStats'

const TOP_TABS = [
  { key: 'endpoint', label: '端点统计' },
  { key: 'usage', label: '用量统计' },
  { key: 'latency', label: '延迟分析' },
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
}, { immediate: true })

onBeforeUnmount(() => { latencyRequestId++ })
</script>

<style scoped>
/* 统计行悬浮底色 */
.stat-line:hover {
  background: var(--surface-2);
  border-radius: 4px;
}
</style>
