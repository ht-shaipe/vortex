import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/usage'
import type { UsageStats } from '@/api/usage'

export const useUsageStore = defineStore('usage', () => {
  const stats = ref<UsageStats | null>(null)
  const loading = ref(false)

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
