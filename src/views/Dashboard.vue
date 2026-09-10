<template>
  <div class="dashboard">
    <el-row :gutter="16">
      <el-col :span="6" v-for="stat in statsCards" :key="stat.label">
        <el-card shadow="never" class="stat-card">
          <div class="stat-value" :style="{ color: stat.color }">{{ stat.value }}</div>
          <div class="stat-label">{{ stat.label }}</div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="16" style="margin-top: 20px">
      <el-col :span="12">
        <el-card shadow="never">
          <template #header>
            <span>Provider Status</span>
          </template>
          <div class="provider-grid">
            <div v-for="p in providerStore.providers" :key="p.id" class="provider-item">
              <span class="provider-dot" :style="{ backgroundColor: p.color }"></span>
              <span class="provider-name">{{ p.name }}</span>
              <el-tag :type="p.activeConnections > 0 ? 'success' : 'info'" size="small">
                {{ p.activeConnections }}/{{ p.connections }}
              </el-tag>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :span="12">
        <el-card shadow="never">
          <template #header>
            <span>Active Combos</span>
          </template>
          <div class="combo-list">
            <div v-for="c in comboStore.combos" :key="c.id" class="combo-item">
              <span class="combo-name">{{ c.name }}</span>
              <el-tag size="small" effect="plain">{{ c.data.strategy }}</el-tag>
              <span class="combo-models">{{ c.data.models.length }} models</span>
            </div>
            <el-empty v-if="comboStore.combos.length === 0" description="No combos yet" :image-size="60" />
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useProviderStore } from '@/stores/provider'
import { useComboStore } from '@/stores/combo'
import { useUsageStore } from '@/stores/usage'

const providerStore = useProviderStore()
const comboStore = useComboStore()
const usageStore = useUsageStore()

const statsCards = computed(() => [
  {
    label: 'Providers',
    value: providerStore.providers.filter(p => p.activeConnections > 0).length,
    color: '#22c55e',
  },
  {
    label: 'Total Requests',
    value: usageStore.stats?.totalRequests ?? 0,
    color: '#6366f1',
  },
  {
    label: 'Tokens Used',
    value: formatNumber((usageStore.stats?.totalTokensInput ?? 0) + (usageStore.stats?.totalTokensOutput ?? 0)),
    color: '#e54d5e',
  },
  {
    label: 'Active Combos',
    value: comboStore.combos.length,
    color: '#f59e0b',
  },
])

function formatNumber(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toString()
}

onMounted(() => {
  providerStore.fetchProviders()
  comboStore.fetchCombos()
  usageStore.fetchStats()
})
</script>

<style scoped>
.provider-grid {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.provider-item {
  display: flex;
  align-items: center;
  gap: 8px;
}

.provider-name {
  flex: 1;
  font-size: 14px;
}

.combo-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.combo-item {
  display: flex;
  align-items: center;
  gap: 8px;
}

.combo-name {
  font-weight: 600;
  min-width: 120px;
}

.combo-models {
  font-size: 12px;
  color: var(--color-text-muted);
  margin-left: auto;
}
</style>
