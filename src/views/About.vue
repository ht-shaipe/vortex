<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="关于" sub="Vortex AI Gateway" />

    <!-- 应用信息卡片：图标、版本、简介与相关链接 -->
    <div class="card section">
      <div class="card-body about-hero flex gap-20px items-start">
        <img class="app-mark w-56px h-56px rounded-14px object-cover shrink-0" src="@/images/logo.png" alt="Vortex" />
        <div class="about-info">
          <div class="about-name text-20px font-bold">Vortex</div>
          <div class="about-meta mono text-11.5px text-ink-3 mt-4px">v{{ currentVersion || '0.1.0' }} · MIT License · Tauri 2 + Rust + Vue 3</div>
          <p class="about-desc text-13px text-ink-2 leading-[1.7] my-12px">统一的 AI 网关桌面应用，将多家 AI 提供商聚合为 OpenAI 兼容 API。支持多种路由策略、组合模型、API 密钥管理与用量统计。</p>
          <div class="about-links">
            <a class="btn" href="https://v2.tauri.app" target="_blank" rel="noopener">Tauri 文档</a>
            <a class="btn" href="https://element-plus.org" target="_blank" rel="noopener">Element Plus</a>
          </div>
        </div>
      </div>
    </div>

    <!-- 检查更新：根据状态展示不同 UI -->
    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">检查更新</div>
          <div class="card-sub">从 GitHub Releases 拉取最新版本</div>
        </div>
        <StatusBadge :tone="statusTone" :label="statusLabel" />
      </div>
      <div class="card-body">
        <!-- 空闲态：可发起检查 -->
        <template v-if="status === 'idle'">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">点击下方按钮检查是否有新版本。</p>
          <button type="button" class="btn primary" @click="checkForUpdate()">检查更新</button>
        </template>
        <!-- 检查中态 -->
        <template v-else-if="status === 'checking'">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">正在检查更新…</p>
          <button type="button" class="btn" disabled>检查中…</button>
        </template>
        <!-- 已是最新版本 -->
        <template v-else-if="status === 'up-to-date'">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">当前已是最新版本。</p>
          <button type="button" class="btn primary" @click="checkForUpdate()">重新检查</button>
        </template>
        <!-- 发现新版本：展示发版说明与下载按钮 -->
        <template v-else-if="status === 'available' && updateInfo">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">
            发现新版本 <strong>v{{ updateInfo.version }}</strong>
            <span v-if="updateInfo.date"> · {{ updateInfo.date.slice(0, 10) }}</span>
          </p>
          <div v-if="updateInfo.body" class="release-notes mt-8px mb-12px py-10px px-12px bg-surface-3 border border-line rounded-sm max-h-200px overflow-y-auto">
            <pre>{{ updateInfo.body }}</pre>
          </div>
          <div class="btn-row flex gap-8px">
            <button type="button" class="btn primary" @click="downloadAndInstall()">下载并安装</button>
            <button type="button" class="btn" @click="checkForUpdate()">重新检查</button>
          </div>
        </template>
        <!-- 下载中态：展示进度条 -->
        <template v-else-if="status === 'downloading'">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">正在下载更新… {{ downloadProgress }}%</p>
          <div class="progress-bar h-6px bg-surface-3 rounded-3px overflow-hidden">
            <div class="progress-fill h-full bg-accent rounded-3px" :style="{ width: downloadProgress + '%' }" />
          </div>
        </template>
        <!-- 下载完成待重启 -->
        <template v-else-if="status === 'ready'">
          <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">更新已下载完成，重启应用以完成安装。</p>
          <button type="button" class="btn primary" @click="relaunchApp()">重启应用</button>
        </template>
        <!-- 出错态 -->
        <template v-else-if="status === 'error'">
          <p class="para error-text text-13px text-ink-2 leading-[1.75] m-0 mb-10px text-err">{{ errorMsg || '检查更新失败' }}</p>
          <button type="button" class="btn primary" @click="checkForUpdate()">重试</button>
        </template>
      </div>
    </div>

    <!-- 通知测试：模拟免费 Token 发现通知 -->
    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">通知测试</div>
          <div class="card-sub">模拟免费 Token 发现通知</div>
        </div>
      </div>
      <div class="card-body">
        <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">点击按钮测试通知弹出效果，通知会显示在右下角，点击通知可跳转到免费 Token 页面。</p>
        <button type="button" class="btn primary" @click="testNotification()">发送测试通知</button>
      </div>
    </div>

    <!-- 免责声明 -->
    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">免责声明</div>
        </div>
      </div>
      <div class="card-body">
        <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">本软件仅作为本地 API 网关代理工具，不存储、不转发你的对话内容到任何第三方服务。所有请求直接从你的设备发往你配置的 AI 提供商。</p>
        <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">请确保你拥有所使用 AI 提供商 API 的合法访问权限，并遵守各提供商的服务条款。使用本软件产生的任何费用由用户自行承担。</p>
        <p class="para text-13px text-ink-2 leading-[1.75] m-0 mb-10px">本软件按「现状」提供，不提供任何明示或暗示的担保。</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 关于页面。
 * 职责：展示应用基本信息、版本号、检查更新流程、通知测试入口与免责声明。
 * 更新检查与下载安装的逻辑由 useUpdater 组合式函数提供。
 */
import { computed, onMounted } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import { useUpdater } from '@/composables/useUpdater'
import { useNotifications } from '@/composables/useNotifications'

// 解构更新器状态与方法
const {
  status,
  updateInfo,
  errorMsg,
  downloadProgress,
  currentVersion,
  checkForUpdate,
  downloadAndInstall,
  relaunchApp,
} = useUpdater()

// 通知组合式函数：提供测试通知能力
const { testNotification } = useNotifications()

// 更新状态对应的徽标色调
const statusTone = computed(() => {
  switch (status.value) {
    case 'up-to-date': return 'ok'
    case 'available': return 'warn'
    case 'downloading': return 'info'
    case 'ready': return 'ok'
    case 'error': return 'err'
    default: return 'neutral'
  }
})

// 更新状态对应的中文标签
const statusLabel = computed(() => {
  switch (status.value) {
    case 'idle': return '未检查'
    case 'checking': return '检查中'
    case 'up-to-date': return '已是最新'
    case 'available': return '有新版本'
    case 'downloading': return '下载中'
    case 'ready': return '待重启'
    case 'error': return '出错'
    default: return '未知'
  }
})

/**
 * 组件挂载时若尚未检查过更新，则自动静默检查一次。
 */
onMounted(() => {
  if (status.value === 'idle') {
    checkForUpdate(true) // true 表示静默检查
  }
})
</script>

<style scoped>
.section { margin-bottom: var(--gap-lg); }
.about-links { display: flex; gap: var(--gap-sm); }
.para:last-child { margin-bottom: 0; }
.release-notes pre {
  font-size: 12px; color: var(--ink-2); line-height: 1.6;
  white-space: pre-wrap; word-break: break-word; margin: 0;
}
.progress-fill { transition: width 0.2s ease; }
</style>
