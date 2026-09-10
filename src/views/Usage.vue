<template>
  <div class="usage-page">
    <el-row :gutter="16">
      <el-col :span="6" v-for="stat in statCards" :key="stat.label">
        <el-card shadow="never" class="stat-card">
          <div class="stat-value" :style="{ color: stat.color }">{{ stat.value }}</div>
          <div class="stat-label">{{ stat.label }}</div>
        </el-card>
      </el-col>
    </el-row>

    <el-card shadow="never" style="margin-top: 20px">
      <template #header>
        <span>Usage by Provider</span>
      </template>
      <div v-if="usageStore.stats?.byProvider" class="provider-usage">
        <div v-for="(count, provider) in usageStore.stats.byProvider" :key="provider" class="provider-usage-row">
          <span class="provider-usage-name">{{ provider }}</span>
          <el-progress :percentage="getPercentage(count as number)" :stroke-width="8" :show-text="false" />
          <span class="provider-usage-count">{{ count }} requests</span>
        </div>
        <el-empty v-if="Object.keys(usageStore.stats?.byProvider ?? {}).length === 0" description="No usage data yet" :image-size="60" />
      </div>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useUsageStore } from '@/stores/usage'

const usageStore = useUsageStore()

const statCards = computed(() => [
  { label: 'Total Requests', value: usageStore.stats?.totalRequests ?? 0, color: '#6366f1' },
  { label: 'Tokens In', value: formatTokens(usageStore.stats?.totalTokensInput ?? 0), color: '#22c55e' },
  { label: 'Tokens Out', value: formatTokens(usageStore.stats?.totalTokensOutput ?? 0), color: '#e54d5e' },
  { label: 'Total Cost', value: `$${(usageStore.stats?.totalCost ?? 0).toFixed(4)}`, color: '#f59e0b' },
])

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toString()
}

function getPercentage(count: number): number {
  const total = usageStore.stats?.totalRequests ?? 1
  return Math.round((count / total) * 100)
}

onMounted(() => {
  usageStore.fetchStats()
})
</script>

<style scoped>
.provider-usage {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.provider-usage-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.provider-usage-name {
  min-width: 100px;
  font-size: 14px;
}

.provider-usage-count {
  font-size: 13px;
  color: var(--color-text-muted);
  min-width: 100px;
  text-align: right;
}
</style>
