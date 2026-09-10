<template>
  <div>
    <PageHeader title="虚拟模型" sub="将多个提供商/模型组合为虚拟模型，按路由策略智能调度" />

    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <template v-else>
      <div v-if="combos.length === 0" class="card">
        <EmptyState title="还没有虚拟模型" desc="创建一个组合模型，将请求按策略调度到多个上游提供商">
          <router-link to="/subscriptions/new" class="btn primary">添加订阅</router-link>
        </EmptyState>
      </div>

      <div v-else class="slot-grid">
        <div v-for="combo in combos" :key="combo.id" class="card slot-card">
          <div class="slot-head">
            <div class="slot-name mono">{{ combo.name }}</div>
            <div class="slot-purpose">
              <StatusBadge :tone="strategyTone(combo.data?.strategy)" :label="strategyLabel(combo.data?.strategy)" />
            </div>
          </div>
          <div class="slot-body">
            <div v-if="combo.data?.models?.length" class="step-list">
              <div v-for="(step, i) in combo.data.models" :key="i" class="step-row">
                <span class="step-index mono">{{ i + 1 }}</span>
                <ProviderLogo :name="step.provider" :size="20" />
                <span class="step-model mono">{{ step.modelStr }}</span>
                <span v-if="step.weight != null" class="step-weight mono">×{{ step.weight }}</span>
              </div>
            </div>
            <div v-else class="step-empty">暂无模型步骤</div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loading } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listCombos, type Combo } from '@/api/combos'

const combos = ref<Combo[]>([])
const loading = ref(true)

const STRATEGY_LABELS: Record<string, string> = {
  priority: '优先级',
  fill_first: '填满优先',
  weighted: '加权随机',
  round_robin: '轮询',
  p2c: 'P2C 负载均衡',
  least_used: '最少使用',
  random: '随机',
  strict_random: '严格随机',
  cost_optimized: '成本优化',
  headroom: '剩余配额',
  reset_window: '重置窗口',
  reset_aware: '重置感知',
  context_relay: '上下文接力',
  context_optimized: '上下文优化',
  lkgp: 'LKGP 粘性',
  auto: '自动评分',
  fusion: '融合',
}

function strategyLabel(s?: string): string {
  return s ? STRATEGY_LABELS[s] ?? s : '优先级'
}
function strategyTone(s?: string): 'ok' | 'warn' | 'err' | 'neutral' {
  if (s === 'auto' || s === 'cost_optimized') return 'ok'
  if (s === 'fusion') return 'warn'
  return 'neutral'
}

onMounted(async () => {
  try {
    const data = await listCombos()
    combos.value = data.combos ?? []
  } finally {
    loading.value = false
  }
})
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.slot-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--gap-md);
}
@media (max-width: 900px) {
  .slot-grid { grid-template-columns: 1fr; }
}
.slot-card { padding: var(--pad-card); }
.slot-head { margin-bottom: var(--gap-sm); }
.slot-name { font-size: 15px; font-weight: 600; color: var(--ink); }
.slot-purpose { margin-top: 6px; }
.step-list { display: flex; flex-direction: column; gap: 4px; }
.step-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--r-sm);
  background: var(--surface-2);
}
.step-index { color: var(--ink-4); font-size: 11px; width: 14px; text-align: center; }
.step-model { flex: 1; font-size: 12.5px; color: var(--ink-2); }
.step-weight { color: var(--ink-3); font-size: 11px; }
.step-empty { font-size: 12.5px; color: var(--ink-4); padding: 8px; }
</style>