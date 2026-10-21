<template>
  <el-dialog
    :model-value="visible"
    @update:model-value="onVisibleChange"
    :title="`登录${platformLabel}`"
    width="480"
    :close-on-click-modal="false"
    class="cdp-wizard-dialog"
  >
    <!-- 步骤指示器：启动 Chrome → 浏览器登录 → 收集导入 → 完成 -->
    <div class="flex items-center gap-4px mb-18px">
      <template v-for="(s, i) in STEPS" :key="s.key">
        <div
          class="w-22px h-22px rounded-full flex items-center justify-center text-11px font-medium border border-solid transition-all shrink-0"
          :class="stepIndex === i ? 'border-accent bg-accent text-white' : stepIndex > i ? 'border-ok bg-ok text-white' : 'border-line bg-surface text-ink-4'"
        >
          <el-icon v-if="stepIndex > i" :size="11"><Check /></el-icon>
          <span v-else>{{ i + 1 }}</span>
        </div>
        <div v-if="i < STEPS.length - 1" class="w-16px h-1px" :class="stepIndex > i ? 'bg-ok' : 'bg-line'" />
      </template>
    </div>

    <!-- 启动失败 / 授权失败 -->
    <div v-if="phase === 'failed'" class="flex flex-col items-center text-center gap-10px py-12px">
      <div class="w-48px h-48px rounded-full flex items-center justify-center" style="background: var(--err); opacity: 0.12">
        <el-icon :size="24" style="color: var(--err)"><CircleCloseFilled /></el-icon>
      </div>
      <div class="text-14px font-semibold text-ink">授权失败</div>
      <div class="text-12.5px text-ink-3 leading-[1.6] max-w-380px break-words">{{ status?.message || '未知错误' }}</div>
    </div>

    <!-- 已取消 -->
    <div v-else-if="phase === 'cancelled'" class="flex flex-col items-center text-center gap-10px py-12px">
      <div class="w-48px h-48px rounded-full flex items-center justify-center" style="background: var(--ink-4); opacity: 0.12">
        <el-icon :size="24" style="color: var(--ink-4)"><CircleCloseFilled /></el-icon>
      </div>
      <div class="text-14px font-semibold text-ink">已取消授权</div>
    </div>

    <!-- 成功 -->
    <div v-else-if="phase === 'success'" class="flex flex-col items-center text-center gap-10px py-12px">
      <div class="w-48px h-48px rounded-full flex items-center justify-center" style="background: var(--ok); opacity: 0.12">
        <el-icon :size="24" style="color: var(--ok)"><SuccessFilled /></el-icon>
      </div>
      <div class="text-14px font-semibold text-ink">登录成功</div>
      <div class="text-12.5px text-ink-3 leading-[1.6]">
        已导入 {{ status?.imported ?? 0 }} 条登录凭证{{ status?.loggedIn === false ? '（登录态校验未通过，建议重试）' : '' }}，
        重启后自动保持登录。
      </div>
    </div>

    <!-- 进行中：launching / waiting_login / collecting / importing -->
    <div v-else class="flex flex-col gap-12px">
      <div class="flex items-center gap-10px">
        <el-icon class="spin" :size="16" style="color: var(--accent)"><Loading /></el-icon>
        <span class="text-13px font-medium text-ink-2">{{ STEPS[Math.max(stepIndex, 0)].label }}</span>
        <span v-if="phase === 'waiting_login'" class="mono text-11.5px text-ink-4 ml-auto">{{ Math.floor(elapsed / 60) }}:{{ String(elapsed % 60).padStart(2, '0') }}</span>
      </div>
      <div class="progress-note text-12px text-ink-3 leading-[1.6] px-14px py-10px bg-surface-2 rounded-sm">
        <template v-if="phase === 'launching'">
          正在拉起本地 Chrome 并打开{{ platformLabel }}登录页…<br />
          <span class="text-ink-4">首次启动可能需要几秒钟</span>
        </template>
        <template v-else-if="phase === 'waiting_login'">
          请在弹出的 Chrome 浏览器中完成登录（扫码 / 验证码均可）。<br />
          <span class="text-ink-4">登录完成后系统会自动检测；也可点击下方「我已完成登录」立即收集凭证。</span>
        </template>
        <template v-else-if="phase === 'collecting'">
          正在通过 CDP 从 Chrome 收集{{ platformLabel }}的登录 cookie…
        </template>
        <template v-else-if="phase === 'importing'">
          正在把登录凭证注入网关引擎并校验登录态…
        </template>
      </div>
      <div v-if="status?.message" class="text-11.5px text-ink-4 leading-[1.5] break-words">{{ status.message }}</div>
    </div>

    <template #footer>
      <div class="dialog-footer flex items-center gap-10px">
        <!-- 失败可重试 -->
        <button v-if="phase === 'failed'" type="button" class="btn" @click="restart">重试</button>
        <span class="spacer flex-1" />
        <!-- 进行中：可取消 -->
        <button v-if="isRunning" type="button" class="btn" @click="cancel">取消授权</button>
        <!-- 等待登录：手动触发收集 -->
        <button v-if="phase === 'waiting_login'" type="button" class="btn primary" @click="finish">
          我已完成登录
        </button>
        <!-- 终态：关闭 -->
        <button v-if="isTerminal" type="button" class="btn primary" @click="close">完成</button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * CdpLoginWizard.vue — 网页版平台 CDP 登录向导对话框
 *
 * 职责：打开时自动拉起本地 Chrome 开始 CDP 授权，轮询授权进度
 * （启动 → 用户在浏览器登录 → cookie 收集 → 注入网关），支持
 * 手动触发收集（finish）与取消（cancel），成功后向父组件发 success 事件。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Check, Loading, SuccessFilled, CircleCloseFilled } from '@element-plus/icons-vue'
import { startCdpLogin, cdpLoginStatus, finishCdpLogin, cancelCdpLogin, type CdpLoginStatus } from '@/api/webProviders'

const props = defineProps<{
  visible: boolean
  /** 平台标识（doubao / chatgpt / ...） */
  platform: string
  /** 平台展示名（默认取 platform） */
  platformName?: string
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  success: []
}>()

/** 流程步骤（与后端 phase 对应的展示映射） */
const STEPS = [
  { key: 'launch', label: '启动 Chrome' },
  { key: 'login', label: '浏览器登录' },
  { key: 'import', label: '收集与导入' },
  { key: 'done', label: '完成' },
] as const

// 当前状态快照（null = 尚未开始）
const status = ref<CdpLoginStatus | null>(null)
// 轮询定时器句柄
let pollTimer: ReturnType<typeof setInterval> | null = null

const platformLabel = computed(() => props.platformName || props.platform)
const phase = computed(() => status.value?.phase ?? 'launching')
const elapsed = computed(() => status.value?.elapsedSecs ?? 0)
const isTerminal = computed(() => ['success', 'failed', 'cancelled'].includes(phase.value))
const isRunning = computed(() => !!status.value && !isTerminal.value)

/** phase → 步骤序号（步骤指示器高亮位） */
const stepIndex = computed(() => {
  switch (phase.value) {
    case 'launching': return 0
    case 'waiting_login': return 1
    case 'collecting': return 2
    case 'importing': return 2
    case 'success': return 3
    default: return 0
  }
})

/** 停止轮询 */
function stopPolling() {
  if (pollTimer !== null) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

/** 启动轮询：每 2 秒拉取授权进度，终态自动停止 */
function startPolling() {
  stopPolling()
  pollTimer = setInterval(async () => {
    try {
      const st = await cdpLoginStatus(props.platform)
      // 404：会话不存在（被外部结束）；进行中窗口里视为异常关闭
      if (st === null) {
        if (isRunning.value) {
          stopPolling()
          status.value = { phase: 'failed', elapsedSecs: 0, message: '授权会话已不存在（可能被其他操作结束）' }
        }
        return
      }
      status.value = st
      if (['success', 'failed', 'cancelled'].includes(st.phase)) {
        stopPolling()
        if (st.phase === 'success') emit('success')
      }
    } catch {
      /* 网络抖动等瞬态错误：跳过本轮，下轮重试 */
    }
  }, 2000)
}

/** 开始（或重启）授权 */
async function restart() {
  status.value = { phase: 'launching', elapsedSecs: 0, message: '' }
  try {
    await startCdpLogin(props.platform)
    startPolling()
  } catch (e: unknown) {
    const resp = e as { response?: { status?: number; data?: { error?: string } }; message?: string }
    if (resp.response?.status === 409) {
      // 已有进行中的会话：直接接管轮询
      startPolling()
    } else {
      status.value = {
        phase: 'failed',
        elapsedSecs: 0,
        message: resp.response?.data?.error || resp.message || '启动失败（请确认本机已安装 Chrome）',
      }
    }
  }
}

/** 手动触发 cookie 收集 */
async function finish() {
  try {
    await finishCdpLogin(props.platform)
    ElMessage.success('已触发凭证收集，正在导入…')
  } catch (e: unknown) {
    const resp = e as { response?: { data?: { error?: string } }; message?: string }
    ElMessage.error(resp.response?.data?.error || resp.message || '触发收集失败')
  }
}

/** 取消授权 */
async function cancel() {
  try {
    await cancelCdpLogin(props.platform)
  } catch {
    /* 会话可能已结束，忽略 */
  }
  stopPolling()
  status.value = { phase: 'cancelled', elapsedSecs: 0, message: '' }
}

/** 关闭对话框：进行中的会话自动取消（避免 Chrome 悬挂） */
function close() {
  if (isRunning.value) {
    void cancel()
  } else {
    stopPolling()
  }
  emit('update:visible', false)
}

/** 对话框 visible 变化（含点遮罩 / ESC） */
function onVisibleChange(v: boolean) {
  if (!v) {
    close()
  } else {
    emit('update:visible', true)
  }
}

// 打开时自动开始授权；关闭（组件卸载）时清理轮询
watch(
  () => props.visible,
  (v) => {
    if (v) {
      void restart()
    } else {
      stopPolling()
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => stopPolling())
</script>
