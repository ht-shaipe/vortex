<template>
  <!-- 顶部拖拽区：纯 JS 闭包方式实现，每次拖动独立状态，避免拖一次后失效 -->
  <div class="window-chrome flex text-center justify-between items-center w-full h-38px shrink-0 select-none bg-transparent" data-tauri-drag-region>
    <div class="flex items-center pl-8px">
      <button class="flex items-center justify-center w-26px h-26px border-none rounded-6px bg-transparent text-ink-3 cursor-pointer transition-colors hover:bg-surface-3 hover:text-ink" :title="collapsed ? '展开侧边栏' : '折叠侧边栏'" @click="toggleCollapsed">
        <el-icon :size="16">
          <Fold v-if="!collapsed" />
          <Expand v-else />
        </el-icon>
      </button>
    </div>
    <div class="text-center">Vortex · AI Gateway</div>
    <div class="flex items-center gap-12px pr-12px relative">
      <!-- 网关状态指示灯 -->
      <div class="flex items-center gap-6px text-12px text-ink-3" :title="`网关 ${healthOk ? '运行中' : '未启动'} · :10168`">
        <span class="w-7px h-7px rounded-full [&.on]:bg-ok [&.on]:shadow-[0_0_0_3px_color-mix(in_oklab,var(--ok)_20%,transparent)] [&.off]:bg-err [&.off]:shadow-[0_0_0_3px_color-mix(in_oklab,var(--err)_20%,transparent)]" :class="healthOk ? 'on' : 'off'" />
        <span class="font-medium [&.on]:text-ok [&.off]:text-err" :class="healthOk ? 'on' : 'off'">网关{{ healthOk ? '运行中' : '未启动' }}</span>
      </div>
      <!-- 通知图标 -->
      <button class="bell-btn relative flex items-center justify-center w-26px h-26px border-none rounded-6px bg-transparent text-ink-3 cursor-pointer transition-colors hover:bg-surface-3 hover:text-ink" title="通知" @click="showNotifPanel = !showNotifPanel">
        <el-icon :size="16">
          <Bell />
        </el-icon>
        <span v-if="unreadCount > 0" class="absolute -top-2px -right-2px min-w-15px h-15px px-4px rounded-8px bg-err text-white text-10px font-semibold leading-15px text-center">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
      </button>
      <!-- 通知下拉面板 -->
      <div v-if="showNotifPanel" class="notif-panel absolute top-34px right-0 w-340px max-h-420px bg-surface border border-solid border-line rounded-md shadow-[0_12px_40px_-12px_rgba(0,0,0,0.5)] z-100 flex flex-col overflow-hidden">
        <div class="flex items-center justify-between px-14px py-12px border-b border-line border-solid border-0 text-13px font-semibold">
          <span>通知</span>
          <button v-if="latestNotifications.length" class="text-12px text-accent-ink bg-none border-none cursor-pointer"
            @click="markAllRead(); showNotifPanel = false">全部已读</button>
        </div>
        <div class="flex-1 overflow-y-auto">
          <div v-if="latestNotifications.length === 0" class="py-32px text-center text-13px text-ink-4">暂无通知</div>
          <div v-for="n in latestNotifications" :key="n.id" class="px-14px py-10px border-b border-line border-solid border-0 cursor-pointer transition-colors hover:bg-surface-3 last:border-b-0" @click="openNotif(n)">
            <div class="text-13px font-medium text-ink mb-2px">{{ n.title }}</div>
            <div class="text-12px text-ink-3 leading-[1.5]">{{ n.body }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * WindowChrome.vue — 窗口拖拽区域
 * 职责：侧边栏折叠按钮、网关健康状态指示、通知中心入口。
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { ElIcon } from 'element-plus'
import { Fold, Expand, Bell } from '@element-plus/icons-vue'
import { useSidebarCollapsed } from '@/composables/useSidebarCollapsed'
import { useNotifications } from '@/composables/useNotifications'
import { useGatewayStatus } from '@/composables/useGatewayStatus'
import type { NotificationItem } from '@/api/notifications'

const { collapsed, toggleCollapsed } = useSidebarCollapsed()
const { unreadCount, latestNotifications, markAllRead, startPolling } = useNotifications()
const { healthOk } = useGatewayStatus()

const showNotifPanel = ref(false)

/** 点击通知项：打开外链或跳转免费 Token 页，并关闭面板。 */
function openNotif(n: NotificationItem) {
  if (n.url) {
    window.open(n.url, '_blank', 'noopener')
  } else {
    import('@/router').then((m) => m.default.push('/free-tokens'))
  }
  showNotifPanel.value = false
}

/** 点击面板外部时关闭。 */
function onDocClick(e: MouseEvent) {
  const el = e.target as HTMLElement
  if (showNotifPanel.value && !el.closest('.notif-panel') && !el.closest('.bell-btn')) {
    showNotifPanel.value = false
  }
}

onMounted(async () => {
  startPolling()
  document.addEventListener('click', onDocClick)
})

onUnmounted(() => {
  document.removeEventListener('click', onDocClick)
})
</script>

