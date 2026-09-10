/**
 * 提供商状态管理 Store。
 * 管理所有 AI 服务提供商定义（ProviderDef）及其连接（ProviderConnection），
 * 提供拉取列表、新增、删除、测试连接等操作。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import * as api from '@/api/providers'
import type { ProviderDef, ProviderConnection } from '@/types'

export const useProviderStore = defineStore('providers', () => {
  // 提供商定义列表
  const providers = ref<ProviderDef[]>([])
  // 已建立的提供商连接列表
  const connections = ref<ProviderConnection[]>([])
  // 加载状态标志
  const loading = ref(false)

  /**
   * 拉取所有提供商定义及连接列表。
   * 请求期间设置 loading 为 true，结束后恢复。
   */
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

  /**
   * 新增一个提供商连接。
   * @param params - 包含 provider（提供商 ID）、name（连接名称）、apiKey（可选密钥）
   * @returns 新创建的连接对象
   */
  async function addProvider(params: { provider: string; name: string; apiKey?: string }) {
    const conn = await api.createProvider(params)
    connections.value.push(conn)
    // 更新对应提供商定义的连接计数
    const def = providers.value.find(p => p.id === params.provider)
    if (def) {
      def.connections++
    }
    return conn
  }

  /**
   * 删除指定 ID 的提供商连接。
   * @param id - 连接 ID
   */
  async function removeProvider(id: string) {
    await api.deleteProvider(id)
    // 从本地列表中移除
    connections.value = connections.value.filter(c => c.id !== id)
  }

  /**
   * 测试指定连接的连通性。
   * @param id - 连接 ID
   * @returns 测试结果
   */
  async function testConnection(id: string) {
    return await api.testProvider(id)
  }

  return { providers, connections, loading, fetchProviders, addProvider, removeProvider, testConnection }
})
