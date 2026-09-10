<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="关于" sub="Vortex AI Gateway" />

    <!-- 应用信息卡片：图标、版本、简介与相关链接 -->
    <div class="card section">
      <div class="card-body about-hero">
        <div class="app-mark">V</div>
        <div class="about-info">
          <div class="about-name">Vortex</div>
          <div class="about-meta mono">v{{ currentVersion || '0.1.0' }} · MIT License · Tauri 2 + Rust + Vue 3</div>
          <p class="about-desc">统一的 AI 网关桌面应用，将 21+ 个 AI 提供商聚合为 OpenAI 兼容 API。支持 17 种路由策略、组合模型、API 密钥管理与用量统计。</p>
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
          <p class="para">点击下方按钮检查是否有新版本。</p>
          <button type="button" class="btn primary" @click="checkForUpdate()">检查更新</button>
        </template>
        <!-- 检查中态 -->
        <template v-else-if="status === 'checking'">
          <p class="para">正在检查更新…</p>
          <button type="button" class="btn" disabled>检查中…</button>
        </template>
        <!-- 已是最新版本 -->
        <template v-else-if="status === 'up-to-date'">
          <p class="para">当前已是最新版本。</p>
          <button type="button" class="btn primary" @click="checkForUpdate()">重新检查</button>
        </template>
        <!-- 发现新版本：展示发版说明与下载按钮 -->
        <template v-else-if="status === 'available' && updateInfo">
          <p class="para">
            发现新版本 <strong>v{{ updateInfo.version }}</strong>
            <span v-if="updateInfo.date"> · {{ updateInfo.date.slice(0, 10) }}</span>
          </p>
          <div v-if="updateInfo.body" class="release-notes">
            <pre>{{ updateInfo.body }}</pre>
          </div>
          <div class="btn-row">
            <button type="button" class="btn primary" @click="downloadAndInstall()">下载并安装</button>
            <button type="button" class="btn" @click="checkForUpdate()">重新检查</button>
          </div>
        </template>
        <!-- 下载中态：展示进度条 -->
        <template v-else-if="status === 'downloading'">
          <p class="para">正在下载更新… {{ downloadProgress }}%</p>
          <div class="progress-bar">
            <div class="progress-fill" :style="{ width: downloadProgress + '%' }" />
          </div>
        </template>
        <!-- 下载完成待重启 -->
        <template v-else-if="status === 'ready'">
          <p class="para">更新已下载完成，重启应用以完成安装。</p>
          <button type="button" class="btn primary" @click="relaunchApp()">重启应用</button>
        </template>
        <!-- 出错态 -->
        <template v-else-if="status === 'error'">
          <p class="para error-text">{{ errorMsg || '检查更新失败' }}</p>
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
        <p class="para">点击按钮测试通知弹出效果，通知会显示在右下角，点击通知可跳转到免费 Token 页面。</p>
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
        <p class="para">本软件仅作为本地 API 网关代理工具，不存储、不转发你的对话内容到任何第三方服务。所有请求直接从你的设备发往你配置的 AI 提供商。</p>
        <p class="para">请确保你拥有所使用 AI 提供商 API 的合法访问权限，并遵守各提供商的服务条款。使用本软件产生的任何费用由用户自行承担。</p>
        <p class="para">本软件按「现状」提供，不提供任何明示或暗示的担保。</p>
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
.about-hero { display: flex; gap: 20px; align-items: flex-start; }
.app-mark {
  width: 56px; height: 56px;
  border-radius: 14px;
  background: var(--ink);
  color: var(--accent);
  display: grid;
  place-items: center;
  font-weight: 800;
  font-size: 26px;
  flex-shrink: 0;
}
.about-name { font-size: 20px; font-weight: 700; }
.about-meta { font-size: 11.5px; color: var(--ink-3); margin-top: 4px; }
.about-desc { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 12px 0; }
.about-links { display: flex; gap: var(--gap-sm); }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.75; margin: 0 0 10px; }
.para:last-child { margin-bottom: 0; }
.error-text { color: var(--err); }
.btn-row { display: flex; gap: 8px; }
.release-notes {
  margin: 8px 0 12px; padding: 10px 12px;
  background: var(--surface-3); border: 1px solid var(--line);
  border-radius: var(--r-sm); max-height: 200px; overflow-y: auto;
}
.release-notes pre {
  font-size: 12px; color: var(--ink-2); line-height: 1.6;
  white-space: pre-wrap; word-break: break-word; margin: 0;
}
.progress-bar {
  height: 6px; background: var(--surface-3); border-radius: 3px; overflow: hidden;
}
.progress-fill {
  height: 100%; background: var(--accent); border-radius: 3px;
  transition: width 0.2s ease;
}
</style>
