<template>
  <aside class="sidebar" :class="{ collapsed }">
    <div class="brand">
      <img class="brand-logo" src="@/images/logo.png" alt="Vortex" />
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
/**
 * Sidebar.vue — 左侧导航栏
 * 职责：展示品牌标识、主导航项与底部导航项，支持折叠/展开，定时刷新供应商数量徽章。
 */
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

  Setting,
  InfoFilled,
  Document,
} from '@element-plus/icons-vue'
import { listProviders } from '@/api/providers'

const route = useRoute() // 当前路由对象，用于高亮激活项

// 折叠状态的 localStorage 持久化键
const COLLAPSE_KEY = 'vortex-sidebar-collapsed'
// 侧边栏是否折叠（从本地存储恢复初始值）
const collapsed = ref(localStorage.getItem(COLLAPSE_KEY) === '1')

/** 切换侧边栏折叠/展开状态，并持久化到 localStorage。 */
function toggleCollapsed() {
  collapsed.value = !collapsed.value
  localStorage.setItem(COLLAPSE_KEY, collapsed.value ? '1' : '0')
}

/** 导航项数据结构。 */
interface NavItem {
  to: string
  label: string
  icon: unknown
  badge?: string | null
  dot?: boolean
  dotTone?: 'ok' | 'err'
}

const providerCount = ref(0) // 已配置的供应商连接数量

// 主导航项列表（含徽章/圆点指示）
const mainItems = computed<NavItem[]>(() => [
  { to: '/guide', label: '接入指南', icon: Reading },
  { to: '/live-routing', label: '实时路由', icon: DataLine, dot: true, dotTone: 'ok' },
  { to: '/subscriptions', label: '订阅管理', icon: Key, badge: providerCount.value > 0 ? String(providerCount.value) : null },
  { to: '/statistics', label: '数据统计', icon: Histogram },
  { to: '/sync', label: '配置同步', icon: Connection },
  { to: '/request-logs', label: '请求日志', icon: Document },
  { to: '/free-tokens', label: '薅Token', icon: Present },
  { to: '/chat', label: '对话', icon: ChatDotRound },

])

// 底部导航项列表（设置、关于）
const bottomItems: NavItem[] = [
  { to: '/settings', label: '设置', icon: Setting },
  { to: '/about', label: '关于', icon: InfoFilled },
]

/** 判断指定路由前缀是否处于激活状态。 */
function isActive(to: string): boolean {
  // 实时路由与首页共用激活态
  if (to === '/live-routing') return route.path === '/live-routing' || route.path === '/'
  return route.path.startsWith(to)
}

/** 加载供应商连接数量，用于订阅管理徽章显示。 */
async function loadCounts() {
  try {
    const providers = await listProviders()
    providerCount.value = providers.connections?.length ?? 0
  } catch {
    /* 后端未启动时静默 */
  }
}

onMounted(() => {
  loadCounts() // 首次加载
  setInterval(loadCounts, 10000) // 每 10 秒轮询刷新
})
</script>
