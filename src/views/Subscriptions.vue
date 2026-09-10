<template>
  <div>
    <PageHeader title="订阅管理" sub="管理各 AI 提供商的连接与 API 密钥">
      <template #actions>
        <router-link to="/subscriptions/new" class="btn">
          <el-icon :size="14"><Plus /></el-icon>添加提供方
        </router-link>
        <router-link to="/subscriptions/custom" class="btn primary">
          <el-icon :size="14"><Connection /></el-icon>添加自定义提供方
        </router-link>
      </template>
    </PageHeader>

    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <div v-else-if="connections.length === 0" class="card">
      <EmptyState title="还没有订阅" desc="从内置提供商列表中添加一个连接，或填写自定义端点接入 OpenAI 兼容服务">
        <div class="empty-actions">
          <router-link to="/subscriptions/new" class="btn">添加提供方</router-link>
          <router-link to="/subscriptions/custom" class="btn primary">添加自定义提供方</router-link>
        </div>
      </EmptyState>
    </div>

    <div v-else class="card">
      <table class="table">
        <thead>
          <tr>
            <th style="width: 120px">状态</th>
            <th>提供商</th>
            <th style="width: 160px">默认模型</th>
            <th style="width: 90px">优先级</th>
            <th style="width: 120px">更新于</th>
            <th style="width: 80px"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="conn in connections" :key="conn.id" @click="$router.push(`/subscriptions/${conn.id}`)">
            <td>
              <StatusBadge :tone="statusTone(conn)" :label="statusLabel(conn)" />
            </td>
            <td>
              <div class="prov-cell">
                <ProviderLogo :name="conn.provider" :color="provColor(conn.provider)" :size="24" />
                <div>
                  <div class="prov-name">{{ conn.name }}</div>
                  <div class="prov-id mono">{{ conn.provider }}</div>
                </div>
              </div>
            </td>
            <td><span class="mono num">{{ conn.defaultModel || '—' }}</span></td>
            <td><span class="num">{{ conn.priority }}</span></td>
            <td><span class="num">{{ fmtTime(conn.createdAt) }}</span></td>
            <td>
              <button type="button" class="btn sm" @click.stop="$router.push(`/subscriptions/${conn.id}`)">查看</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loading, Plus, Connection } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listProviders, type ProviderDef, type ProviderConnection } from '@/api/providers'

const connections = ref<ProviderConnection[]>([])
const providers = ref<ProviderDef[]>([])
const loading = ref(true)

function provColor(id: string): string {
  return providers.value.find((p) => p.id === id)?.color ?? ''
}

function statusLabel(conn: ProviderConnection): string {
  if (!conn.isActive) return '已禁用'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) return '健康'
  if (['failed', 'error', 'invalid'].includes(s)) return '失败'
  return '未测试'
}

function statusTone(conn: ProviderConnection): 'ok' | 'warn' | 'err' | 'neutral' {
  if (!conn.isActive) return 'neutral'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) return 'ok'
  if (['failed', 'error', 'invalid'].includes(s)) return 'err'
  return 'neutral'
}

function fmtTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

onMounted(async () => {
  try {
    const data = await listProviders()
    providers.value = data.providers ?? []
    connections.value = data.connections ?? []
  } finally {
    loading.value = false
  }
})
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.prov-cell { display: flex; align-items: center; gap: 10px; }
.prov-name { font-size: 13px; font-weight: 500; color: var(--ink); }
.prov-id { font-size: 11px; color: var(--ink-4); }
.empty-actions { display: flex; gap: 8px; }
</style>