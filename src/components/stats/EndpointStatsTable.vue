<template>
  <p v-if="rows.length === 0" class="hint m-0 text-body text-ink-4">该周期暂无数据</p>
  <div v-else class="card table-wrap overflow-hidden">
    <table class="table">
      <thead>
        <tr>
          <th>端点</th>
          <th class="text-right">请求</th>
          <th class="text-right">错误</th>
          <th class="text-right">输入 Token</th>
          <th class="text-right">输出 Token</th>
          <th class="text-right">缓存创建</th>
          <th class="text-right">缓存读取</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in rows" :key="r.endpointName">
          <td>{{ r.endpointName }}</td>
          <td class="text-right num">{{ fmtInt(r.requests) }}</td>
          <td class="text-right num" :class="{ 'text-err': r.errors > 0 }">{{ fmtInt(r.errors) }}</td>
          <td class="text-right num">{{ fmtInt(r.inputTokens) }}</td>
          <td class="text-right num">{{ fmtInt(r.outputTokens) }}</td>
          <td class="text-right num">{{ fmtInt(r.cacheCreationTokens) }}</td>
          <td class="text-right num">{{ fmtInt(r.cacheReadTokens) }}</td>
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
import { fmtInt } from '@/lib/format'
import type { EndpointStat } from '@/api/stats'

// Props 定义：rows 为端点统计行列表
defineProps<{ rows: EndpointStat[] }>()
</script>

<style scoped>
.table tbody tr { cursor: default; }
</style>
