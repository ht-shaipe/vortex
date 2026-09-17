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
            <th style="width: 90px">来源</th>
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
            <td>{{ agentLabel(r.agent) }}</td>
            <td class="num" :title="tokenTitle(r, r.tokensInput)">{{ fmtToken(r.tokensInput, r.usageEstimated, isZeroUsage(r)) }}</td>
            <td class="num" :title="tokenTitle(r, r.tokensOutput)">{{ fmtToken(r.tokensOutput, r.usageEstimated, isZeroUsage(r)) }}</td>
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
import { formatTokenK } from '@/lib/format'

/** 单条日志记录的结构 */
interface Row {
  time?: string
  status?: string
  provider?: string
  model?: string
  agent?: string
  tokensInput?: number
  tokensOutput?: number
  latencyMs?: number
  /** token 数是否为估算值（上游未返回用量时按内容估算） */
  usageEstimated?: boolean
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
 * 判断该记录是否为"成功但完全没有用量"（上游未返回且无法估算）。
 * @param r 日志记录
 */
function isZeroUsage(r: Row): boolean {
  return r.status === 'success' && !r.tokensInput && !r.tokensOutput && !r.usageEstimated
}
/**
 * 格式化 Token 数量：紧凑展示（1k / 102k）。
 * 估算值加 ≈ 前缀；成功但无用量显示"未返回"；精确值见 title。
 * @param n Token 数量
 * @param estimated 是否为估算值
 * @param zeroUsage 成功但上游未返回用量
 */
function fmtToken(n?: number, estimated?: boolean, zeroUsage?: boolean): string {
  if (zeroUsage) return '未返回'
  if (n == null) return '—'
  return `${estimated ? '≈' : ''}${formatTokenK(n)}`
}
/**
 * 生成 Token 单元格的悬停提示文本：精确值 + 估算/未返回说明。
 * @param r 日志记录
 * @param n Token 数量
 */
function tokenTitle(r: Row, n?: number): string {
  if (n == null) return ''
  const parts = [n.toLocaleString()]
  if (r.usageEstimated) parts.push('上游未返回用量，为按内容估算的近似值')
  else if (isZeroUsage(r)) parts.push('上游未返回用量')
  return parts.join('\n')
}
/**
 * 格式化耗时：小于 1000ms 用毫秒，否则折算为秒并保留一位小数。
 * @param ms 毫秒
 */
function fmtLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '—'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}

/** Agent 标识 → 友好名称 */
const agentNames: Record<string, string> = {
  claude_code: 'Claude Code',
  codex: 'Codex',
  opencode: 'OpenCode',
  qwen_code: 'Qwen Code',
  dsh: 'DeepSeek',
  zcode: 'ZCode',
  gemini_cli: 'Gemini CLI',
  cursor_agent: 'Cursor',
  vortex_chat: '内置聊天',
}
function agentLabel(agent?: string): string {
  if (!agent) return '—'
  return agentNames[agent] ?? agent
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
    records.value = list.map((item) => {
      const o = (item ?? {}) as Record<string, unknown>
      return {
      time: (o.created_at ?? o.timestamp ?? o.time) as string | undefined,
      status: o.status as string | undefined,
      provider: o.provider as string | undefined,
      model: o.model as string | undefined,
      agent: o.agent as string | undefined,
      tokensInput: (o.tokens_input ?? o.input_tokens) as number | undefined,
      tokensOutput: (o.tokens_output ?? o.output_tokens) as number | undefined,
      latencyMs: o.latency_ms as number | undefined,
      usageEstimated: o.usage_estimated === true || o.usage_estimated === 1,
      }
    })
  } catch (e) {
    ElMessage.error(`加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

watch(page, () => void load())
onMounted(load)
</script>
