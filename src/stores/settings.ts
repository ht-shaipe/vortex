import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/settings'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Record<string, unknown>>({})
  const loading = ref(false)

  async function fetchSettings() {
    loading.value = true
    try {
      settings.value = await api.getSettings()
    } finally {
      loading.value = false
    }
  }

  async function saveSettings(updates: Record<string, unknown>) {
    settings.value = await api.updateSettings(updates)
  }

  return { settings, loading, fetchSettings, saveSettings }
})
