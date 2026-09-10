<template>
  <div>
    <PageHeader title="请求日志" sub="网关代理的最近请求记录">
      <template #actions>
        <button type="button" class="btn" @click="load">
          <el-icon :size="14"><Refresh /></el-icon>刷新
        </button>
      </template>
    </PageHeader>

    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <div v-else-if="records.length === 0" class="card">
      <EmptyState title="暂无请求日志" desc="发起一次网关请求后，这里会显示记录" />
    </div>

    <div v-else class="card">
      <table class="table">
        <thead>
          <tr>
            <th style="width: 130px">时间</th>
            <th style="width: 90px">状态</th>
            <th>提供商</th>
            <th>模型</th>
            <th class="num" style="width: 90px">Tokens 输入</th>
            <th class="num" style="width: 90px">Tokens 输出</th>
            <th class="num" style="width: 80px">耗时</th>
            <th class="num" style="width: 90px">成本</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(r, i) in records" :key="i">
            <td class="num">{{ fmtTime(r.time) }}</td>
            <td><StatusBadge :tone="statusTone(r.status)" :label="statusLabel(r.status)" /></td>
            <td class="mono">{{ r.provider || '—' }}</td>
            <td class="mono">{{ r.model || '—' }}</td>
            <td class="num">{{ fmtNum(r.tokensInput) }}</td>
            <td class="num">{{ fmtNum(r.tokensOutput) }}</td>
            <td class="num">{{ r.latencyMs != null ? `${r.latencyMs}ms` : '—' }}</td>
            <td class="num">{{ r.cost != null ? `$${Number(r.cost).toFixed(4)}` : '—' }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Loading, Refresh } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import { getRecentUsage } from '@/api/usage'

interface Row {
  time?: string
  status?: string
  provider?: string
  model?: string
  tokensInput?: number
  tokensOutput?: number
  latencyMs?: number
  cost?: number
}

const records = ref<Row[]>([])
const loading = ref(true)

function statusLabel(s?: string): string {
  if (s === 'success') return '成功'
  if (s === 'timeout') return '超时'
  if (s === 'error' || s === 'failed') return '失败'
  return '成功'
}
function statusTone(s?: string): 'ok' | 'warn' | 'err' | 'neutral' {
  if (s === 'timeout') return 'warn'
  if (s === 'error' || s === 'failed') return 'err'
  return 'ok'
}
function fmtTime(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${String(d.getMonth() + 1).padStart(2, '0')}/${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}
function fmtNum(n?: number): string {
  return n == null ? '—' : String(n)
}

async function load() {
  loading.value = true
  try {
    const data: unknown = await getRecentUsage(100)
    const list: unknown[] = Array.isArray(data)
      ? (data as unknown[])
      : ((data as { records?: unknown[] }).records ?? [])
    records.value = list.map((item: Record<string, unknown>) => ({
      time: (item.created_at ?? item.timestamp ?? item.time) as string | undefined,
      status: item.status as string | undefined,
      provider: item.provider as string | undefined,
      model: item.model as string | undefined,
      tokensInput: (item.tokens_input ?? item.input_tokens) as number | undefined,
      tokensOutput: (item.tokens_output ?? item.output_tokens) as number | undefined,
      latencyMs: item.latency_ms as number | undefined,
      cost: item.cost as number | undefined,
    }))
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
</style>