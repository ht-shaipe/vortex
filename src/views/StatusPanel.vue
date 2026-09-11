<template>
  <div
    class="status-panel w-full h-screen box-border p-14px flex flex-col gap-10px bg-surface rounded-14px border border-line overflow-hidden"
  >
    <!-- 顶部标题栏 -->
    <div class="sp-header flex items-center justify-between shrink-0">
      <div class="sp-brand flex items-center gap-8px">
        <span
          class="sp-logo w-28px h-28px rounded-7px bg-ink text-accent grid place-items-center font-extrabold text-14px"
          >V</span
        >
        <div class="sp-brand-text flex flex-col leading-1.15">
          <span class="sp-name text-14px font-bold text-ink">Vortex</span>
          <span class="sp-version text-10.5px text-ink-4">v{{ status?.version ?? '—' }}</span>
        </div>
      </div>
      <div
        class="sp-proxy-badge inline-flex items-center gap-5px py-3px px-8px rounded-full text-11px font-semibold"
        :class="status?.proxy_running ? 'on bg-ok-bg text-ok' : 'off bg-err-bg text-err'"
      >
        <span class="sp-dot w-6px h-6px rounded-full bg-current" />
        {{ status?.proxy_running ? '运行中' : '已停止' }}
      </div>
    </div>

    <!-- 代理状态卡片 -->
    <div class="sp-card bg-surface-2 border border-line rounded-10px py-10px px-12px shrink-0">
      <div class="sp-card-row flex items-center justify-between py-3px">
        <span class="sp-label text-12.5px text-ink-3">代理端口</span>
        <span class="sp-value text-12.5px font-semibold text-ink tabular-nums">{{
          status?.proxy_port ?? '—'
        }}</span>
      </div>
      <div class="sp-card-row flex items-center justify-between py-3px">
        <span class="sp-label text-12.5px text-ink-3">数据库</span>
        <span
          class="sp-value text-12.5px font-semibold text-ink"
          :class="status?.database_ok ? 'text-ok' : 'text-err'"
        >
          {{ status?.database_ok ? '正常' : '异常' }}
        </span>
      </div>
      <div class="sp-card-actions mt-8px pt-8px border-t border-line">
        <button
          type="button"
          class="sp-btn w-full py-7px px-12px border-none rounded-7px text-12.5px font-semibold cursor-pointer"
          :class="{
            'primary bg-accent text-white': !status?.proxy_running,
            'danger bg-err-bg text-err': status?.proxy_running,
          }"
          :disabled="busy"
          @click="toggleProxy"
        >
          {{ status?.proxy_running ? '停止代理' : '启动代理' }}
        </button>
      </div>
    </div>

    <!-- 连接统计 -->
    <div class="sp-card bg-surface-2 border border-line rounded-10px py-10px px-12px shrink-0">
      <p
        class="sp-card-title m-0 mb-8px text-11px font-semibold tracking-0.05em uppercase text-ink-4"
      >
        连接统计
      </p>
      <div class="sp-stats-grid grid grid-cols-2 gap-8px">
        <div class="sp-stat flex flex-col items-center gap-2px py-6px px-4px bg-surface rounded-7px">
          <span class="sp-stat-num text-18px font-bold text-ink leading-1.1 tabular-nums">{{
            status?.provider_count ?? 0
          }}</span>
          <span class="sp-stat-label text-10.5px text-ink-4">提供商</span>
        </div>
        <div class="sp-stat flex flex-col items-center gap-2px py-6px px-4px bg-surface rounded-7px">
          <span class="sp-stat-num text-18px font-bold text-ink leading-1.1 text-ok tabular-nums">{{
            status?.active_provider_count ?? 0
          }}</span>
          <span class="sp-stat-label text-10.5px text-ink-4">活跃连接</span>
        </div>
        <div class="sp-stat flex flex-col items-center gap-2px py-6px px-4px bg-surface rounded-7px">
          <span class="sp-stat-num text-18px font-bold text-ink leading-1.1 tabular-nums">{{
            status?.api_key_count ?? 0
          }}</span>
          <span class="sp-stat-label text-10.5px text-ink-4">API Key</span>
        </div>
        <div class="sp-stat flex flex-col items-center gap-2px py-6px px-4px bg-surface rounded-7px">
          <span class="sp-stat-num text-18px font-bold text-ink leading-1.1 text-ok tabular-nums">{{
            status?.active_api_key_count ?? 0
          }}</span>
          <span class="sp-stat-label text-10.5px text-ink-4">有效 Key</span>
        </div>
      </div>
    </div>

    <!-- 今日用量 -->
    <div class="sp-card bg-surface-2 border border-line rounded-10px py-10px px-12px shrink-0">
      <p
        class="sp-card-title m-0 mb-8px text-11px font-semibold tracking-0.05em uppercase text-ink-4"
      >
        今日用量
      </p>
      <div class="sp-card-row flex items-center justify-between py-3px">
        <span class="sp-label text-12.5px text-ink-3">请求数</span>
        <span class="sp-value text-12.5px font-semibold text-ink tabular-nums">{{
          fmt(status?.today_requests)
        }}</span>
      </div>
      <div class="sp-card-row flex items-center justify-between py-3px">
        <span class="sp-label text-12.5px text-ink-3">输入 Token</span>
        <span class="sp-value text-12.5px font-semibold text-ink tabular-nums">{{
          fmt(status?.today_tokens_input)
        }}</span>
      </div>
      <div class="sp-card-row flex items-center justify-between py-3px">
        <span class="sp-label text-12.5px text-ink-3">输出 Token</span>
        <span class="sp-value text-12.5px font-semibold text-ink tabular-nums">{{
          fmt(status?.today_tokens_output)
        }}</span>
      </div>
    </div>

    <!-- 底部操作 -->
    <div class="sp-footer mt-auto shrink-0">
      <button
        type="button"
        class="sp-btn ghost w-full py-7px px-12px border-none rounded-7px text-12.5px font-semibold cursor-pointer bg-surface-3 text-ink-2"
        @click="showMainWindow"
      >
        打开主窗口
      </button>
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
/* 透明窗口下的面板背景：box-shadow 与 font-family 难以用原子类表达，保留 */
.status-panel {
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

/* 代理状态徽标的脉冲动画（复杂选择器 + @keyframes，保留） */
.sp-proxy-badge.on .sp-dot {
  animation: pulse 1.8s ease-in-out infinite;
}
@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

/* 卡片行分隔线（兄弟选择器，保留） */
.sp-card-row + .sp-card-row {
  border-top: 1px solid var(--line);
}

/* 按钮：transition 与伪类样式保留 */
.sp-btn {
  transition: opacity 0.15s, background 0.15s;
}
.sp-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.sp-btn.primary:hover {
  opacity: 0.9;
}
.sp-btn.danger:hover {
  opacity: 0.85;
}
.sp-btn.ghost:hover {
  background: var(--line);
  color: var(--ink);
}
</style>
