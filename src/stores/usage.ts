/**
 * 用量统计状态管理 Store。
 * 管理用量统计数据，提供拉取统计信息的能力。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/usage'
import type { UsageStats } from '@/api/usage'

export const useUsageStore = defineStore('usage', () => {
  // 用量统计数据，初始为 null
  const stats = ref<UsageStats | null>(null)
  // 加载状态标志
  const loading = ref(false)

  /**
   * 拉取用量统计数据。
   * 请求期间设置 loading 为 true，结束后恢复。
   */
  async function fetchStats() {
    loading.value = true
    try {
      stats.value = await api.getUsageStats()
    } finally {
      loading.value = false
    }
  }

  return { stats, loading, fetchStats }
})
