<template>
  <!-- 顶部拖拽区：纯 JS 闭包方式实现，每次拖动独立状态，避免拖一次后失效 -->
  <div class="window-chrome flex text-center justify-between items-center" data-tauri-drag-region>
    <div class="chrome-left">
      <button class="collapse-btn" :title="collapsed ? '展开侧边栏' : '折叠侧边栏'" @click="toggleCollapsed">
        <el-icon :size="16">
          <Fold v-if="!collapsed" />
          <Expand v-else />
        </el-icon>
      </button>
    </div>
    <div class="text-center">Vortex · AI Gateway</div>
    <div class="chrome-right">
      <!-- 网关状态指示灯 -->
      <div class="gw-status" :title="`网关 ${healthOk ? '运行中' : '未启动'} · :10168`">
        <span class="gw-dot" :class="healthOk ? 'on' : 'off'" />
        <span class="gw-text" :class="healthOk ? 'on' : 'off'">网关{{ healthOk ? '运行中' : '未启动' }}</span>
      </div>
      <!-- 通知图标 -->
      <button class="bell-btn" title="通知" @click="showNotifPanel = !showNotifPanel">
        <el-icon :size="16">
          <Bell />
        </el-icon>
        <span v-if="unreadCount > 0" class="bell-badge">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
      </button>
      <!-- 通知下拉面板 -->
      <div v-if="showNotifPanel" class="notif-panel">
        <div class="notif-head">
          <span>通知</span>
          <button v-if="latestNotifications.length" class="notif-mark"
            @click="markAllRead(); showNotifPanel = false">全部已读</button>
        </div>
        <div class="notif-list">
          <div v-if="latestNotifications.length === 0" class="notif-empty">暂无通知</div>
          <div v-for="n in latestNotifications" :key="n.id" class="notif-item" @click="openNotif(n)">
            <div class="notif-title">{{ n.title }}</div>
            <div class="notif-body">{{ n.body }}</div>
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
import type { NotificationItem } from '@/api/notifications'

const { collapsed, toggleCollapsed } = useSidebarCollapsed()
const { unreadCount, latestNotifications, markAllRead, startPolling } = useNotifications()

const healthOk = ref(false)
const showNotifPanel = ref(false)
let _healthTimer: ReturnType<typeof setInterval> | null = null

/** 轮询后端健康检查接口。 */
async function checkHealth() {
  try {
    const res = await fetch('http://localhost:10168/api/health')
    const data = await res.json()
    healthOk.value = data.status === 'ok'
  } catch {
    healthOk.value = false
  }
}

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

onMounted(() => {
  checkHealth()
  _healthTimer = setInterval(checkHealth, 30000)
  startPolling()
  document.addEventListener('click', onDocClick)
})

onUnmounted(() => {
  if (_healthTimer) clearInterval(_healthTimer)
  document.removeEventListener('click', onDocClick)
})
</script>

<style scoped>
.window-chrome {
  width: 100%;
  height: 38px;
  flex-shrink: 0;
  user-select: none;
  -webkit-user-select: none;
  background: transparent;
}

.chrome-left {
  display: flex;
  align-items: center;
  padding-left: 8px;
}

.collapse-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-3);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.collapse-btn:hover {
  background: var(--surface-3);
  color: var(--ink);
}

.chrome-right {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-right: 12px;
  position: relative;
}

/* 网关状态 */
.gw-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--ink-3);
}

.gw-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}

.gw-dot.on {
  background: var(--ok);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--ok) 20%, transparent);
}

.gw-dot.off {
  background: var(--err);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--err) 20%, transparent);
}

.gw-text {
  font-weight: 500;

  &.on {
    color: var(--ok);
  }

  &.off {
    color: var(--err);
  }
}


/* 通知按钮 */
.bell-btn {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-3);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.bell-btn:hover {
  background: var(--surface-3);
  color: var(--ink);
}

.bell-badge {
  position: absolute;
  top: -2px;
  right: -2px;
  min-width: 15px;
  height: 15px;
  padding: 0 4px;
  border-radius: 8px;
  background: var(--err);
  color: #fff;
  font-size: 10px;
  font-weight: 600;
  line-height: 15px;
  text-align: center;
}

/* 通知面板 */
.notif-panel {
  position: absolute;
  top: 34px;
  right: 0;
  width: 340px;
  max-height: 420px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md, 12px);
  box-shadow: 0 12px 40px -12px rgba(0, 0, 0, 0.5);
  z-index: 100;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.notif-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px;
  border-bottom: 1px solid var(--line);
  font-size: 13px;
  font-weight: 600;
}

.notif-mark {
  font-size: 12px;
  color: var(--accent-ink, oklch(0.78 0.10 280));
  background: none;
  border: none;
  cursor: pointer;
}

.notif-list {
  flex: 1;
  overflow-y: auto;
}

.notif-empty {
  padding: 32px 0;
  text-align: center;
  font-size: 13px;
  color: var(--ink-4);
}

.notif-item {
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
  cursor: pointer;
  transition: background 0.15s;
}

.notif-item:hover {
  background: var(--surface-3);
}

.notif-item:last-child {
  border-bottom: none;
}

.notif-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--ink);
  margin-bottom: 2px;
}

.notif-body {
  font-size: 12px;
  color: var(--ink-3);
  line-height: 1.5;
}
</style>
