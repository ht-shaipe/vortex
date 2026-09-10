<template>
  <div>
    <PageHeader title="检查更新" sub="查看并安装新版本" />

    <div class="grid-2">
      <div class="card section">
        <div class="card-head">
          <div>
            <div class="card-title">版本信息</div>
            <div class="card-sub">当前安装的版本</div>
          </div>
        </div>
        <div class="card-body">
          <div class="setting-row">
            <div>
              <div class="setting-label">当前版本</div>
            </div>
            <span class="mono num">v{{ currentVersion || '...' }}</span>
          </div>
          <div class="setting-row">
            <div>
              <div class="setting-label">更新源</div>
              <div class="setting-desc">从 GitHub Releases 拉取最新版本</div>
            </div>
            <StatusBadge tone="ok" label="GitHub" :dot="false" />
          </div>
          <div class="setting-row">
            <div>
              <div class="setting-label">自动检查</div>
              <div class="setting-desc">应用启动时自动检查更新</div>
            </div>
            <StatusBadge tone="ok" label="已启用" />
          </div>
        </div>
      </div>

      <div class="card section">
        <div class="card-head">
          <div>
            <div class="card-title">更新状态</div>
          </div>
          <StatusBadge
            :tone="statusTone"
            :label="statusLabel"
          />
        </div>
        <div class="card-body">
          <!-- idle -->
          <template v-if="status === 'idle'">
            <p class="para">点击下方按钮检查是否有新版本。</p>
            <button type="button" class="btn primary" @click="checkForUpdate()">检查更新</button>
          </template>

          <!-- checking -->
          <template v-else-if="status === 'checking'">
            <p class="para">正在检查更新…</p>
            <button type="button" class="btn" disabled>检查中…</button>
          </template>

          <!-- up-to-date -->
          <template v-else-if="status === 'up-to-date'">
            <p class="para">当前已是最新版本。</p>
            <button type="button" class="btn primary" @click="checkForUpdate()">重新检查</button>
          </template>

          <!-- available -->
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

          <!-- downloading -->
          <template v-else-if="status === 'downloading'">
            <p class="para">正在下载更新… {{ downloadProgress }}%</p>
            <div class="progress-bar">
              <div class="progress-fill" :style="{ width: downloadProgress + '%' }" />
            </div>
          </template>

          <!-- installing -->
          <template v-else-if="status === 'installing'">
            <p class="para">安装完成，正在重启应用…</p>
          </template>

          <!-- error -->
          <template v-else-if="status === 'error'">
            <p class="para error-text">{{ errorMsg || '检查更新失败' }}</p>
            <button type="button" class="btn primary" @click="checkForUpdate()">重试</button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import { useUpdater } from '@/composables/useUpdater'

const {
  status,
  updateInfo,
  errorMsg,
  downloadProgress,
  currentVersion,
  checkForUpdate,
} = useUpdater()

const statusTone = computed(() => {
  switch (status.value) {
    case 'up-to-date': return 'ok'
    case 'available': return 'warn'
    case 'downloading':
    case 'installing': return 'info'
    case 'error': return 'err'
    default: return 'neutral'
  }
})

const statusLabel = computed(() => {
  switch (status.value) {
    case 'idle': return '未检查'
    case 'checking': return '检查中'
    case 'up-to-date': return '已是最新'
    case 'available': return '有新版本'
    case 'downloading': return '下载中'
    case 'installing': return '安装中'
    case 'error': return '出错'
    default: return '未知'
  }
})

onMounted(() => {
  if (status.value === 'idle') {
    checkForUpdate(true)
  }
})
</script>

<style scoped>
.grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: var(--gap-md); }
@media (max-width: 900px) { .grid-2 { grid-template-columns: 1fr; } }
.section { height: fit-content; }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 0 0 12px; }
.error-text { color: var(--err); }
.btn-row { display: flex; gap: 8px; }
.btn {
  padding: 6px 16px; border-radius: var(--r-sm); font-size: 13px;
  border: 1px solid var(--border-2); background: var(--surface-2); color: var(--ink-1);
  cursor: pointer; transition: all 0.15s;
}
.btn:hover:not(:disabled) { border-color: var(--border-3); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn.primary {
  background: var(--accent); border-color: var(--accent); color: #fff;
}
.btn.primary:hover:not(:disabled) { filter: brightness(1.1); }
.release-notes {
  margin: 8px 0 12px; padding: 10px 12px;
  background: var(--surface-1); border: 1px solid var(--border-1);
  border-radius: var(--r-sm); max-height: 200px; overflow-y: auto;
}
.release-notes pre {
  font-size: 12px; color: var(--ink-2); line-height: 1.6;
  white-space: pre-wrap; word-break: break-word; margin: 0;
}
.progress-bar {
  height: 6px; background: var(--surface-1); border-radius: 3px; overflow: hidden;
}
.progress-fill {
  height: 100%; background: var(--accent); border-radius: 3px;
  transition: width 0.2s ease;
}
</style>
