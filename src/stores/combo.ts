import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/combos'
import type { Combo, ComboStep } from '@/types'

export const useComboStore = defineStore('combos', () => {
  const combos = ref<Combo[]>([])
  const loading = ref(false)

  async function fetchCombos() {
    loading.value = true
    try {
      const data = await api.listCombos()
      combos.value = data.combos
    } finally {
      loading.value = false
    }
  }

  async function addCombo(params: { name: string; strategy?: string; models?: ComboStep[] }) {
    const combo = await api.createCombo(params)
    combos.value.push(combo)
    return combo
  }

  async function removeCombo(id: string) {
    await api.deleteCombo(id)
    combos.value = combos.value.filter(c => c.id !== id)
  }

  return { combos, loading, fetchCombos, addCombo, removeCombo }
})
