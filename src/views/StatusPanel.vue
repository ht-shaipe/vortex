<template>
  <div class="status-panel">
    <!-- 顶部标题栏 -->
    <div class="sp-header">
      <div class="sp-brand">
        <span class="sp-logo">V</span>
        <div class="sp-brand-text">
          <span class="sp-name">Vortex</span>
          <span class="sp-version">v{{ status?.version ?? '—' }}</span>
        </div>
      </div>
      <div class="sp-proxy-badge" :class="status?.proxy_running ? 'on' : 'off'">
        <span class="sp-dot" />
        {{ status?.proxy_running ? '运行中' : '已停止' }}
      </div>
    </div>

    <!-- 代理状态卡片 -->
    <div class="sp-card">
      <div class="sp-card-row">
        <span class="sp-label">代理端口</span>
        <span class="sp-value tnum">{{ status?.proxy_port ?? '—' }}</span>
      </div>
      <div class="sp-card-row">
        <span class="sp-label">数据库</span>
        <span class="sp-value" :class="status?.database_ok ? 'ok' : 'err'">
          {{ status?.database_ok ? '正常' : '异常' }}
        </span>
      </div>
      <div class="sp-card-actions">
        <button
          type="button"
          class="sp-btn"
          :class="{ primary: !status?.proxy_running, danger: status?.proxy_running }"
          :disabled="busy"
          @click="toggleProxy"
        >
          {{ status?.proxy_running ? '停止代理' : '启动代理' }}
        </button>
      </div>
    </div>

    <!-- 连接统计 -->
    <div class="sp-card">
      <p class="sp-card-title">连接统计</p>
      <div class="sp-stats-grid">
        <div class="sp-stat">
          <span class="sp-stat-num tnum">{{ status?.provider_count ?? 0 }}</span>
          <span class="sp-stat-label">提供商</span>
        </div>
        <div class="sp-stat">
          <span class="sp-stat-num tnum ok">{{ status?.active_provider_count ?? 0 }}</span>
          <span class="sp-stat-label">活跃连接</span>
        </div>
        <div class="sp-stat">
          <span class="sp-stat-num tnum">{{ status?.api_key_count ?? 0 }}</span>
          <span class="sp-stat-label">API Key</span>
        </div>
        <div class="sp-stat">
          <span class="sp-stat-num tnum ok">{{ status?.active_api_key_count ?? 0 }}</span>
          <span class="sp-stat-label">有效 Key</span>
        </div>
      </div>
    </div>

    <!-- 今日用量 -->
    <div class="sp-card">
      <p class="sp-card-title">今日用量</p>
      <div class="sp-card-row">
        <span class="sp-label">请求数</span>
        <span class="sp-value tnum">{{ fmt(status?.today_requests) }}</span>
      </div>
      <div class="sp-card-row">
        <span class="sp-label">输入 Token</span>
        <span class="sp-value tnum">{{ fmt(status?.today_tokens_input) }}</span>
      </div>
      <div class="sp-card-row">
        <span class="sp-label">输出 Token</span>
        <span class="sp-value tnum">{{ fmt(status?.today_tokens_output) }}</span>
      </div>
    </div>

    <!-- 底部操作 -->
    <div class="sp-footer">
      <button type="button" class="sp-btn ghost" @click="showMainWindow">打开主窗口</button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * StatusPanel.vue — 托盘状态面板
 * 职责：展示系统运行状态（代理、端口、数据库、连接统计、今日用量），
 * 提供启动/停止代理与打开主窗口的快捷操作。每 5 秒自动刷新。
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { ElMessage } from 'element-plus'

/** 系统运行状态（与 Rust 端 SystemStatus 结构体对应） */
interface SystemStatus {
  proxy_running: boolean
  proxy_port: number
  version: string
  database_ok: boolean
  provider_count: number
  active_provider_count: number
  api_key_count: number
  active_api_key_count: number
  today_requests: number
  today_tokens_input: number
  today_tokens_output: number
}

const status = ref<SystemStatus | null>(null)
const busy = ref(false)
let timer: ReturnType<typeof setInterval> | null = null

/** 调用 Tauri 命令获取系统状态 */
async function fetchStatus(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    status.value = await invoke<SystemStatus>('get_system_status')
  } catch {
    /* 静默失败，保持上次数据 */
  }
}

/** 切换代理启停 */
async function toggleProxy(): Promise<void> {
  if (busy.value) return
  busy.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    if (status.value?.proxy_running) {
      await invoke('stop_proxy')
      ElMessage.success('代理已停止')
    } else {
      await invoke('start_proxy')
      ElMessage.success('代理已启动')
    }
    await fetchStatus()
  } catch (err) {
    ElMessage.error(err instanceof Error ? err.message : String(err))
  } finally {
    busy.value = false
  }
}

/** 显示并聚焦主窗口 */
async function showMainWindow(): Promise<void> {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    // 先隐藏自身
    await getCurrentWindow().hide()
    // 显示主窗口
    const main = WebviewWindow.getByLabel('main')
    if (main) {
      await main.show()
      await main.unminimize()
      await main.setFocus()
    }
  } catch (err) {
    ElMessage.error(err instanceof Error ? err.message : String(err))
  }
}

/** 数字格式化：千分位 */
function fmt(n?: number): string {
  if (n == null) return '0'
  return n.toLocaleString('zh-CN')
}

onMounted(async () => {
  await fetchStatus()
  // 每 5 秒自动刷新状态
  timer = setInterval(() => void fetchStatus(), 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
/* 透明窗口下的面板背景 */
.status-panel {
  width: 100%;
  height: 100vh;
  box-sizing: border-box;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--surface);
  border-radius: 14px;
  border: 1px solid var(--line);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

/* 顶部标题栏 */
.sp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
}
.sp-brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.sp-logo {
  width: 28px;
  height: 28px;
  border-radius: 7px;
  background: var(--ink);
  color: var(--accent);
  display: grid;
  place-items: center;
  font-weight: 800;
  font-size: 14px;
}
.sp-brand-text {
  display: flex;
  flex-direction: column;
  line-height: 1.15;
}
.sp-name {
  font-size: 14px;
  font-weight: 700;
  color: var(--ink);
}
.sp-version {
  font-size: 10.5px;
  color: var(--ink-4);
}
.sp-proxy-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
}
.sp-proxy-badge.on {
  background: var(--ok-bg);
  color: var(--ok);
}
.sp-proxy-badge.off {
  background: var(--err-bg);
  color: var(--err);
}
.sp-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}
.sp-proxy-badge.on .sp-dot {
  animation: pulse 1.8s ease-in-out infinite;
}
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}

/* 卡片 */
.sp-card {
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 10px 12px;
  flex-shrink: 0;
}
.sp-card-title {
  margin: 0 0 8px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--ink-4);
}
.sp-card-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 3px 0;
}
.sp-card-row + .sp-card-row {
  border-top: 1px solid var(--line);
}
.sp-label {
  font-size: 12.5px;
  color: var(--ink-3);
}
.sp-value {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--ink);
}
.sp-value.ok { color: var(--ok); }
.sp-value.err { color: var(--err); }
.tnum { font-variant-numeric: tabular-nums; }

.sp-card-actions {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--line);
}

/* 统计网格 */
.sp-stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.sp-stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 6px 4px;
  background: var(--surface);
  border-radius: 7px;
}
.sp-stat-num {
  font-size: 18px;
  font-weight: 700;
  color: var(--ink);
  line-height: 1.1;
}
.sp-stat-num.ok { color: var(--ok); }
.sp-stat-label {
  font-size: 10.5px;
  color: var(--ink-4);
}

/* 按钮 */
.sp-btn {
  width: 100%;
  padding: 7px 12px;
  border: none;
  border-radius: 7px;
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.15s, background 0.15s;
}
.sp-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.sp-btn.primary {
  background: var(--accent);
  color: #fff;
}
.sp-btn.primary:hover { opacity: 0.9; }
.sp-btn.danger {
  background: var(--err-bg);
  color: var(--err);
}
.sp-btn.danger:hover { opacity: 0.85; }
.sp-btn.ghost {
  background: var(--surface-3);
  color: var(--ink-2);
}
.sp-btn.ghost:hover { background: var(--line); color: var(--ink); }

/* 底部 */
.sp-footer {
  margin-top: auto;
  flex-shrink: 0;
}
</style>
