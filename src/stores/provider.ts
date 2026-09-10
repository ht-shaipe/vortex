import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/providers'
import type { ProviderDef, ProviderConnection } from '@/types'

export const useProviderStore = defineStore('providers', () => {
  const providers = ref<ProviderDef[]>([])
  const connections = ref<ProviderConnection[]>([])
  const loading = ref(false)

  async function fetchProviders() {
    loading.value = true
    try {
      const data = await api.listProviders()
      providers.value = data.providers
      connections.value = data.connections
    } finally {
      loading.value = false
    }
  }

  async function addProvider(params: { provider: string; name: string; apiKey?: string }) {
    const conn = await api.createProvider(params)
    connections.value.push(conn)
    const def = providers.value.find(p => p.id === params.provider)
    if (def) {
      def.connections++
    }
    return conn
  }

  async function removeProvider(id: string) {
    await api.deleteProvider(id)
    connections.value = connections.value.filter(c => c.id !== id)
  }

  async function testConnection(id: string) {
    return await api.testProvider(id)
  }

  return { providers, connections, loading, fetchProviders, addProvider, removeProvider, testConnection }
})
