<template>
  <div>
    <!-- 页面头部：标题与刷新按钮 -->
    <PageHeader title="统计" sub="网关用量与成本概览">
      <template #actions>
        <button type="button" class="btn" @click="load">
          <el-icon :size="14"><Refresh /></el-icon>刷新
        </button>
      </template>
    </PageHeader>

    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <template v-else>
      <!-- KPI 概览行：总请求数、Token 用量、成本、成功率、平均延迟 -->
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

      <!-- 分布统计：按提供商 / 按模型 -->
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
/**
 * 旧版统计页面（已弃用，保留作参考）。
 * 职责：展示网关用量与成本概览，包括 KPI 指标行与按提供商/模型的请求分布。
 * 新版统计请见 Statistics.vue。
 */
import { computed, onMounted, ref } from 'vue'
import { Loading, Refresh } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import ShareList from '@/components/stats/ShareList.vue'
import { getUsageStats, type UsageStats } from '@/api/usage'

// 是否正在加载
const loading = ref(true)
// 用量统计数据，初始为零值
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

// 按提供商分布的派生列表
const byProvider = computed(() => toEntries(stats.value.byProvider))
// 按模型分布的派生列表
const byModel = computed(() => toEntries(stats.value.byModel))

/**
 * 将记录对象转为 { name, value } 数组，供 ShareList 组件渲染。
 * @param map 原始的名称到数值映射
 * @returns 转换后的数组
 */
function toEntries(map: Record<string, number> | undefined): { name: string; value: number }[] {
  return Object.entries(map ?? {}).map(([name, value]) => ({ name, value }))
}

/**
 * 格式化整数为千分位字符串。
 * @param n 待格式化的数值
 */
function fmtInt(n?: number): string {
  return (n ?? 0).toLocaleString()
}
/**
 * 格式化成本，保留 4 位小数。
 * @param n 待格式化的成本
 */
function fmtCost(n?: number): string {
  return (n ?? 0).toFixed(4)
}
/**
 * 格式化百分数，0-1 的小数转为带一位小数的百分比字符串。
 * @param n 0-1 范围的比率
 */
function fmtPct(n?: number): string {
  return `${((n ?? 0) * 100).toFixed(1)}%`
}

/**
 * 加载用量统计数据。
 */
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
