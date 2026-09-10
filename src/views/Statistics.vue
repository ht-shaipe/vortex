<template>
  <div>
    <PageHeader title="统计" sub="网关用量与成本概览">
      <template #actions>
        <button type="button" class="btn" @click="load">
          <el-icon :size="14"><Refresh /></el-icon>刷新
        </button>
      </template>
    </PageHeader>

    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <template v-else>
      <div class="kpi-row">
        <div class="card stat">
          <div class="stat-label">总请求数</div>
          <div class="stat-val">{{ fmtInt(stats.totalRequests) }}</div>
        </div>
        <div class="card stat">
          <div class="stat-label">输入 Tokens</div>
          <div class="stat-val">{{ fmtInt(stats.totalTokensInput) }}</div>
        </div>
        <div class="card stat">
          <div class="stat-label">输出 Tokens</div>
          <div class="stat-val">{{ fmtInt(stats.totalTokensOutput) }}</div>
        </div>
        <div class="card stat">
          <div class="stat-label">总成本</div>
          <div class="stat-val">${{ fmtCost(stats.totalCost) }}</div>
        </div>
        <div class="card stat">
          <div class="stat-label">成功率</div>
          <div class="stat-val">{{ fmtPct(stats.successRate) }}</div>
        </div>
        <div class="card stat">
          <div class="stat-label">平均延迟</div>
          <div class="stat-val">{{ stats.avgLatencyMs != null ? `${Math.round(stats.avgLatencyMs)}ms` : '—' }}</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">按提供商</div>
              <div class="card-sub">请求分布</div>
            </div>
          </div>
          <div class="card-body">
            <ShareList :items="byProvider" />
          </div>
        </div>
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">按模型</div>
              <div class="card-sub">请求分布</div>
            </div>
          </div>
          <div class="card-body">
            <ShareList :items="byModel" />
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Loading, Refresh } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import ShareList from '@/components/stats/ShareList.vue'
import { getUsageStats, type UsageStats } from '@/api/usage'

const loading = ref(true)
const stats = ref<UsageStats>({
  totalRequests: 0,
  totalTokensInput: 0,
  totalTokensOutput: 0,
  totalCost: 0,
  avgLatencyMs: 0,
  successRate: 0,
  byProvider: {},
  byModel: {},
})

const byProvider = computed(() => toEntries(stats.value.byProvider))
const byModel = computed(() => toEntries(stats.value.byModel))

function toEntries(map: Record<string, number> | undefined): { name: string; value: number }[] {
  return Object.entries(map ?? {}).map(([name, value]) => ({ name, value }))
}

function fmtInt(n?: number): string {
  return (n ?? 0).toLocaleString()
}
function fmtCost(n?: number): string {
  return (n ?? 0).toFixed(4)
}
function fmtPct(n?: number): string {
  return `${((n ?? 0) * 100).toFixed(1)}%`
}

async function load() {
  loading.value = true
  try {
    stats.value = await getUsageStats()
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.kpi-row {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: var(--gap-md);
  margin-bottom: var(--gap-lg);
}
@media (max-width: 1100px) { .kpi-row { grid-template-columns: repeat(3, 1fr); } }
@media (max-width: 720px) { .kpi-row { grid-template-columns: repeat(2, 1fr); } }
.stat { padding: 18px 20px; }
.grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: var(--gap-md); }
@media (max-width: 900px) { .grid-2 { grid-template-columns: 1fr; } }
.section { height: fit-content; }
</style>