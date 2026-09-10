<template>
  <el-header class="vortex-header" height="56px">
    <div class="header-left">
      <h2 class="page-title">{{ pageTitle }}</h2>
    </div>
    <div class="header-right">
      <el-tag :type="healthStatus === 'ok' ? 'success' : 'danger'" size="small" effect="dark">
        {{ healthStatus === 'ok' ? 'Online' : 'Offline' }}
      </el-tag>
    </div>
  </el-header>
</template>

<script setup lang="ts">
/**
 * Header.vue — 顶部头部
 * 职责：显示当前页面标题与后端健康状态标签，定时轮询健康检查接口。
 */
import { ref, computed } from 'vue'
import { useRoute } from 'vue-router'

const route = useRoute() // 当前路由对象
const healthStatus = ref('ok') // 后端健康状态（ok / error）

// 路由路径 → 页面标题映射表
const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/providers': 'Providers',
  '/keys': 'API Keys',
  '/usage': 'Usage',
  '/settings': 'Settings',
}

// 当前页面标题（未匹配时回退为 'Vortex'）
const pageTitle = computed(() => pageTitles[route.path] || 'Vortex')

/** 轮询后端健康检查接口，更新健康状态。 */
async function checkHealth() {
  try {
    const res = await fetch('http://localhost:10168/api/health')
    const data = await res.json()
    healthStatus.value = data.status
  } catch {
    healthStatus.value = 'error' // 请求失败标记为异常
  }
}

checkHealth() // 首次检查
setInterval(checkHealth, 30000) // 每 30 秒轮询一次
</script>

<style scoped>
.vortex-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
}

.page-title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}
</style>
