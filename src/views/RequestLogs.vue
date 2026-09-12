<template>
  <div>
    <!-- 页面头部：标题与刷新按钮 -->
    <PageHeader title="请求日志" sub="网关代理的最近请求记录">
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load">
          <el-icon :size="14" :class="{ spin: loading }"><Refresh /></el-icon>刷新
        </button>
      </template>
    </PageHeader>

    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap p-40px text-center text-ink-4">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <!-- 空态 -->
    <div v-else-if="records.length === 0" class="card">
      <EmptyState title="暂无请求日志" desc="发起一次网关请求后，这里会显示记录" />
    </div>

    <!-- 日志表格 -->
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
            <td class="num">{{ r.latencyMs != null ? fmtLatency(r.latencyMs) : '—' }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <Pagination
      v-if="!loading && records.length > 0 && total > pageSize"
      v-model:page="page"
      :page-size="pageSize"
      :total="total"
      class="mt-16px"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 请求日志页面。
 * 职责：展示网关代理的最近请求记录，包括时间、状态、提供商、模型、
 * Token 用量与耗时，支持分页与手动刷新。
 */
import { onMounted, ref, watch } from 'vue'
import { Loading, Refresh } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import Pagination from '@/components/stats/Pagination.vue'
import { getUsagePaged } from '@/api/usage'

/** 单条日志记录的结构 */
interface Row {
  time?: string
  status?: string
  provider?: string
  model?: string
  tokensInput?: number
  tokensOutput?: number
  latencyMs?: number
}

const records = ref<Row[]>([])
const loading = ref(true)
const page = ref(1)
const pageSize = 20
const total = ref(0)

/**
 * 将状态字段映射为中文标签。
 * @param s 原始状态字符串
 */
function statusLabel(s?: string): string {
  if (s === 'success') return '成功'
  if (s === 'timeout') return '超时'
  if (s === 'error' || s === 'failed') return '失败'
  return '成功'
}
/**
 * 将状态字段映射为徽标色调。
 * @param s 原始状态字符串
 */
function statusTone(s?: string): 'ok' | 'warn' | 'err' | 'neutral' {
  if (s === 'timeout') return 'warn'
  if (s === 'error' || s === 'failed') return 'err'
  return 'ok'
}
/**
 * 格式化 ISO 时间为 MM/DD HH:mm。
 * @param iso ISO 时间字符串
 */
function fmtTime(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${String(d.getMonth() + 1).padStart(2, '0')}/${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}
/**
 * 格式化数值，空值返回占位符。
 * @param n 待格式化的数值
 */
function fmtNum(n?: number): string {
  return n == null ? '—' : n.toLocaleString()
}

/**
 * 格式化耗时：小于 1000ms 用毫秒，否则折算为秒并保留一位小数。
 * @param ms 毫秒
 */
function fmtLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '—'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}

/**
 * 加载指定页的请求日志。
 */
async function load() {
  loading.value = true
  try {
    const data = await getUsagePaged(page.value, pageSize)
    const list: unknown[] = data.usage ?? []
    total.value = data.total ?? 0
    records.value = list.map((item: Record<string, unknown>) => ({
      time: (item.created_at ?? item.timestamp ?? item.time) as string | undefined,
      status: item.status as string | undefined,
      provider: item.provider as string | undefined,
      model: item.model as string | undefined,
      tokensInput: (item.tokens_input ?? item.input_tokens) as number | undefined,
      tokensOutput: (item.tokens_output ?? item.output_tokens) as number | undefined,
      latencyMs: item.latency_ms as number | undefined,
    }))
  } catch (e) {
    ElMessage.error(`加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

watch(page, () => void load())
onMounted(load)
</script>
