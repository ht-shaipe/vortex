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
import { ref, computed } from 'vue'
import { useRoute } from 'vue-router'

const route = useRoute()
const healthStatus = ref('ok')

const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/providers': 'Providers',
  '/combos': 'Combos',
  '/keys': 'API Keys',
  '/usage': 'Usage',
  '/settings': 'Settings',
}

const pageTitle = computed(() => pageTitles[route.path] || 'Vortex')

async function checkHealth() {
  try {
    const res = await fetch('http://localhost:20128/api/health')
    const data = await res.json()
    healthStatus.value = data.status
  } catch {
    healthStatus.value = 'error'
  }
}

checkHealth()
setInterval(checkHealth, 30000)
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
