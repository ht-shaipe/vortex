<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">V</div>
      <div class="brand-text">
        <div class="brand-name">Vortex</div>
        <div class="brand-tag">AI Gateway</div>
      </div>
    </div>

    <template v-for="item in items" :key="item.to">
      <router-link :to="item.to" class="nav-item" :class="{ active: isActive(item.to) }">
        <span class="nav-icon"><component :is="item.icon" /></span>
        <span class="nav-label">{{ item.label }}</span>
        <span v-if="item.badge != null" class="badge">{{ item.badge }}</span>
        <span v-else-if="item.dot" class="nav-dot" :class="{ ok: item.dotTone === 'ok' }" />
      </router-link>
    </template>
  </aside>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import {
  Reading,
  DataLine,
  Grid,
  Key,

  Histogram,
  Refresh,
  Setting,
  InfoFilled,
  Document,
} from '@element-plus/icons-vue'
import { listCombos } from '@/api/combos'
import { listProviders } from '@/api/providers'

const route = useRoute()

interface NavItem {
  to: string
  label: string
  icon: unknown
  badge?: string | null
  dot?: boolean
  dotTone?: 'ok' | 'err'
}

const comboCount = ref(0)
const providerCount = ref(0)
const running = ref(false)

const items = computed<NavItem[]>(() => [
  { to: '/guide', label: '接入指南', icon: Reading },
  { to: '/live-routing', label: '实时路由', icon: DataLine, dot: true, dotTone: 'ok' },
  { to: '/virtual-models', label: '虚拟模型', icon: Grid, badge: String(comboCount.value) },
  { to: '/subscriptions', label: '订阅', icon: Key, badge: providerCount.value > 0 ? String(providerCount.value) : null },
  { to: '/request-logs', label: '请求日志', icon: Document },
  { to: '/statistics', label: '统计', icon: Histogram },

  { to: '/updates', label: '检查更新', icon: Refresh },
  { to: '/settings', label: '设置', icon: Setting },
  { to: '/about', label: '关于', icon: InfoFilled },
])

function isActive(to: string): boolean {
  if (to === '/virtual-models') return route.path === '/virtual-models' || route.path === '/'
  return route.path.startsWith(to)
}

async function loadCounts() {
  try {
    const [combos, providers] = await Promise.all([listCombos(), listProviders()])
    comboCount.value = combos.combos?.length ?? 0
    providerCount.value = providers.connections?.length ?? 0
  } catch {
    /* 后端未启动时静默 */
  }
}

async function checkHealth() {
  try {
    const res = await fetch('http://localhost:20128/api/health')
    const data = await res.json()
    running.value = data.status === 'ok'
  } catch {
    running.value = false
  }
}

onMounted(() => {
  loadCounts()
  checkHealth()
  setInterval(loadCounts, 10000)
  setInterval(checkHealth, 5000)
})
</script>