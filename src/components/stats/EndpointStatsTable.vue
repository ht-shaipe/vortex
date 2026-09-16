<template>
  <p v-if="rows.length === 0" class="hint m-0 text-body text-ink-4">该周期暂无数据</p>
  <div v-else class="card table-wrap overflow-hidden">
    <table class="table [&_tbody_tr]:!cursor-default">
      <thead>
        <tr>
          <th>端点</th>
          <th class="text-right">请求</th>
          <th class="text-right">错误</th>
          <th class="text-right">成功率</th>
          <th class="text-right">输入 Token</th>
          <th class="text-right">输出 Token</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in rows" :key="r.endpointName">
          <td>{{ r.endpointName }}</td>
          <td class="text-right num">{{ fmtInt(r.requests) }}</td>
          <td class="text-right num" :class="{ 'text-err': r.errors > 0 }">{{ fmtInt(r.errors) }}</td>
          <td class="text-right num" :class="{ 'text-err': r.requests > 0 && r.errors >= r.requests }">{{ successRate(r) }}</td>
          <td class="text-right num">
            <span :title="r.estimatedCount > 0 ? `${r.estimatedCount}/${r.requests} 条估算` : undefined">{{ formatTokenCompact(r.inputTokens) }}</span>
          </td>
          <td class="text-right num">
            <span :title="r.estimatedCount > 0 ? `${r.estimatedCount}/${r.requests} 条估算` : undefined" :class="{ 'text-ink-3': r.estimatedCount > 0 }">{{ formatTokenCompact(r.outputTokens) }}</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
/**
 * EndpointStatsTable.vue — 端点统计表格
 * 职责：以表格展示各端点的请求、错误、Token 用量等统计明细。
 */
import { fmtInt, formatTokenCompact } from '@/lib/format'
import type { EndpointStat } from '@/api/stats'

// Props 定义：rows 为端点统计行列表
defineProps<{ rows: EndpointStat[] }>()

// 成功率 = (请求数 - 错误数) / 请求数；无请求时显示占位符
function successRate(r: EndpointStat): string {
  if (!r.requests) return '—'
  return `${(((r.requests - r.errors) / r.requests) * 100).toFixed(1)}%`
}
</script>

