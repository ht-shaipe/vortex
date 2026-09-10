/**
 * 设置状态管理 Store。
 * 管理应用全局设置项，提供拉取和保存设置的能力。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/settings'

export const useSettingsStore = defineStore('settings', () => {
  // 设置项数据（键值对形式）
  const settings = ref<Record<string, unknown>>({})
  // 加载状态标志
  const loading = ref(false)

  /**
   * 拉取当前应用设置。
   * 请求期间设置 loading 为 true，结束后恢复。
   */
  async function fetchSettings() {
    loading.value = true
    try {
      settings.value = await api.getSettings()
    } finally {
      loading.value = false
    }
  }

  /**
   * 保存设置更新。
   * @param updates - 需要更新的设置键值对
   * @returns 保存后的完整设置对象
   */
  async function saveSettings(updates: Record<string, unknown>) {
    settings.value = await api.updateSettings(updates)
  }

  return { settings, loading, fetchSettings, saveSettings }
})
