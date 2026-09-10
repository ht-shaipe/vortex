<template>
  <aside class="sidebar" :class="{ collapsed }">
    <div class="brand">
      <div class="brand-mark">V</div>
      <div v-if="!collapsed" class="brand-text">
        <div class="brand-name">Vortex</div>
        <div class="brand-tag">AI Gateway</div>
      </div>
    </div>

    <el-scrollbar class="nav-main-scroll">
      <div class="nav-main">
        <template v-for="item in mainItems" :key="item.to">
          <router-link
            :to="item.to"
            class="nav-item"
            :class="{ active: isActive(item.to) }"
            :title="collapsed ? item.label : undefined"
          >
            <span class="nav-icon"><component :is="item.icon" /></span>
            <span v-if="!collapsed" class="nav-label">{{ item.label }}</span>
            <span v-if="!collapsed && item.badge != null" class="badge">{{ item.badge }}</span>
            <span v-else-if="!collapsed && item.dot" class="nav-dot" :class="{ ok: item.dotTone === 'ok' }" />
          </router-link>
        </template>
      </div>
    </el-scrollbar>

    <div class="nav-bottom">
      <router-link
        v-for="item in bottomItems"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ active: isActive(item.to) }"
        :title="collapsed ? item.label : undefined"
      >
        <span class="nav-icon"><component :is="item.icon" /></span>
        <span v-if="!collapsed" class="nav-label">{{ item.label }}</span>
      </router-link>
      <button class="collapse-btn" :title="collapsed ? '展开侧边栏' : '折叠侧边栏'" @click="toggleCollapsed">
        <el-icon><Fold v-if="!collapsed" /><Expand v-else /></el-icon>
      </button>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { ElIcon } from 'element-plus'
import { Fold, Expand } from '@element-plus/icons-vue'
import {
  Reading,
  DataLine,
  Key,
  Present,
  ChatDotRound,
  Histogram,
  Connection,
  Refresh,
  Setting,
  InfoFilled,
  Document,
} from '@element-plus/icons-vue'
import { listProviders } from '@/api/providers'

const route = useRoute()

const COLLAPSE_KEY = 'vortex-sidebar-collapsed'
const collapsed = ref(localStorage.getItem(COLLAPSE_KEY) === '1')

function toggleCollapsed() {
  collapsed.value = !collapsed.value
  localStorage.setItem(COLLAPSE_KEY, collapsed.value ? '1' : '0')
}

interface NavItem {
  to: string
  label: string
  icon: unknown
  badge?: string | null
  dot?: boolean
  dotTone?: 'ok' | 'err'
}

const providerCount = ref(0)

const mainItems = computed<NavItem[]>(() => [
  { to: '/guide', label: '接入指南', icon: Reading },
  { to: '/live-routing', label: '实时路由', icon: DataLine, dot: true, dotTone: 'ok' },
  { to: '/subscriptions', label: '订阅', icon: Key, badge: providerCount.value > 0 ? String(providerCount.value) : null },
  { to: '/free-tokens', label: '免费 Token', icon: Present },
  { to: '/request-logs', label: '请求日志', icon: Document },
  { to: '/statistics', label: '统计', icon: Histogram },
  { to: '/sync', label: '同步', icon: Connection },
  { to: '/chat', label: '对话', icon: ChatDotRound },
  { to: '/updates', label: '检查更新', icon: Refresh },
])

const bottomItems: NavItem[] = [
  { to: '/settings', label: '设置', icon: Setting },
  { to: '/about', label: '关于', icon: InfoFilled },
]

function isActive(to: string): boolean {
  if (to === '/live-routing') return route.path === '/live-routing' || route.path === '/'
  return route.path.startsWith(to)
}

async function loadCounts() {
  try {
    const providers = await listProviders()
    providerCount.value = providers.connections?.length ?? 0
  } catch {
    /* 后端未启动时静默 */
  }
}

onMounted(() => {
  loadCounts()
  setInterval(loadCounts, 10000)
})
</script>
