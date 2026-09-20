<template>
  <div>
    <!-- 顶部操作栏 -->
    <div class="agent-toolbar">
      <div class="toolbar-left">
        <p class="toolbar-desc">
          自动检测本机已安装的 AI 编程智能体，一键写入 Vortex 网关配置。
        </p>
        <div v-if="lastDetected" class="toolbar-meta">
          <el-icon :size="13"><Clock /></el-icon>
          <span>上次检测: {{ formatTime(lastDetected) }}</span>
          <span v-if="isCacheStale" class="stale-hint">（已超过 1 小时，将自动刷新）</span>
        </div>
      </div>
      <button class="btn sm ghost scan-btn" :disabled="detecting" @click="detectAgents(true)">
        <el-icon :class="{ 'is-loading': detecting }"><Refresh /></el-icon>
        {{ detecting ? '检测中...' : '重新检测' }}
      </button>
    </div>

    <!-- 首次加载骨架屏 -->
    <div v-if="detecting && agents.length === 0" class="agent-grid">
      <div v-for="i in 6" :key="i" class="agent-card skeleton">
        <div class="skeleton-icon"></div>
        <div class="skeleton-body">
          <div class="skeleton-line w60"></div>
          <div class="skeleton-line w40"></div>
          <div class="skeleton-line w80"></div>
        </div>
      </div>
    </div>

    <!-- 智能体卡片网格 -->
    <div v-else-if="agents.length > 0" class="agent-grid">
      <div
        v-for="agent in agents"
        :key="agent.kind"
        class="agent-card"
        :class="[
          `status-${agent.vortex_status}`,
          { installed: agent.installed, 'not-installed': !agent.installed, 'is-configuring': configuring === agent.kind }
        ]"
      >
        <!-- 卡片头部 -->
        <div class="card-top">
          <div class="agent-icon-wrap" :class="getIconClass(agent.kind)">
            <img v-if="getAgentLogo(agent.kind)" :src="getAgentLogo(agent.kind)!" class="agent-icon-img" alt="" />
            <span v-else class="agent-icon-letter">{{ getAgentIcon(agent.kind) }}</span>
          </div>
          <div class="agent-identity">
            <div class="agent-name-row">
              <span class="agent-name">{{ getAgentDisplayName(agent.kind) }}</span>
              <span v-if="agent.installed" class="installed-tag">已安装</span>
            </div>
            <span v-if="agent.version" class="agent-version">v{{ agent.version }}</span>
          </div>
          <span class="status-badge" :class="agent.vortex_status">
            {{ getStatusText(agent.vortex_status) }}
          </span>
        </div>

        <!-- 卡片内容 -->
        <div class="card-mid">
          <template v-if="agent.installed">
            <div v-if="agent.install_path" class="meta-row">
              <span class="meta-label">路径</span>
              <el-tooltip :content="shortenPath(agent.install_path)" placement="top" :show-after="300">
                <span class="meta-value mono truncate">{{ shortenPath(agent.install_path) }}</span>
              </el-tooltip>
            </div>
            <div v-if="agent.config_path" class="meta-row">
              <span class="meta-label">配置</span>
              <el-tooltip :content="shortenPath(agent.config_path)" placement="top" :show-after="300">
                <span class="meta-value mono truncate">{{ shortenPath(agent.config_path) }}</span>
              </el-tooltip>
            </div>
          </template>
          <template v-else>
            <p class="not-installed-hint">未检测到安装</p>
          </template>
          <p v-if="agent.unsupported_reason" class="unsupported-hint">
            {{ agent.unsupported_reason }}
          </p>
        </div>

        <!-- 卡片底部操作 -->
        <div class="card-bottom">
          <button
            v-if="agent.installed && agent.supports_auto_config && agent.vortex_status !== 'configured'"
            class="btn sm accent action-btn"
            :disabled="configuring === agent.kind"
            @click="previewConfig(agent)"
          >
            <el-icon v-if="configuring === agent.kind" class="is-loading"><Loading /></el-icon>
            <el-icon v-else :size="13"><Setting /></el-icon>
            {{ configuring === agent.kind ? '配置中...' : '一键配置' }}
          </button>
          <button
            v-else-if="agent.vortex_status === 'configured'"
            class="btn sm ghost action-btn"
            @click="showRestore(agent)"
          >
            <el-icon :size="13"><RefreshLeft /></el-icon>
            恢复配置
          </button>
          <button
            v-else-if="agent.vortex_status === 'needs_update'"
            class="btn sm accent action-btn"
            :disabled="configuring === agent.kind"
            @click="previewConfig(agent)"
          >
            <el-icon v-if="configuring === agent.kind" class="is-loading"><Loading /></el-icon>
            <el-icon v-else :size="13"><Refresh /></el-icon>
            {{ configuring === agent.kind ? '更新中...' : '更新配置' }}
          </button>
          <span v-else class="action-placeholder"></span>
        </div>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-else-if="!detecting" class="empty-state">
      <el-icon class="empty-icon" :size="42"><Monitor /></el-icon>
      <p class="empty-text">未检测到已安装的 AI 编程智能体</p>
      <p class="empty-hint">
        请先安装支持的智能体（如 Claude Code、Codex 等），然后点击重新检测
      </p>
    </div>

    <!-- 配置预览对话框 -->
    <el-dialog
      v-model="showPreview"
      title="配置预览"
      width="600"
      :close-on-click-modal="false"
    >
      <div v-if="preview" class="preview-content">
        <p class="preview-desc">
          将为 <strong>{{ getAgentDisplayName(preview.agent) }}</strong> 写入以下配置：
        </p>
        <div class="file-changes">
          <div v-for="file in preview.files_to_modify" :key="file.path" class="file-change">
            <div class="file-header">
              <el-icon class="file-icon"><Document /></el-icon>
              <span class="file-path">{{ file.path }}</span>
              <span class="file-op" :class="file.operation">{{ getOperationText(file.operation) }}</span>
            </div>
            <div class="file-summary">{{ file.summary }}</div>
          </div>
        </div>
        <div v-if="preview.warnings.length > 0" class="preview-warnings">
          <el-icon class="warning-icon"><WarningFilled /></el-icon>
          <div class="warning-list">
            <div v-for="(warning, idx) in preview.warnings" :key="idx" class="warning-item">{{ warning }}</div>
          </div>
        </div>
        <div v-if="preview.requires_restart" class="preview-restart">
          <el-icon class="restart-icon"><InfoFilled /></el-icon>
          <span>配置完成后需要重启智能体才能生效</span>
        </div>
      </div>
      <template #footer>
        <button class="btn ghost" @click="showPreview = false">取消</button>
        <button class="btn accent" :disabled="applying" @click="applyConfig">
          {{ applying ? '应用中...' : '确认应用' }}
        </button>
      </template>
    </el-dialog>

    <!-- 恢复配置对话框 -->
    <el-dialog v-model="showRestoreDialog" title="恢复配置" width="500" :close-on-click-modal="false">
      <div v-if="restoreAgent" class="restore-content">
        <p>确定要恢复 <strong>{{ getAgentDisplayName(restoreAgent.kind) }}</strong> 的配置到之前的状态吗？</p>
        <p class="restore-hint">这将撤销最近的自动配置更改，恢复到配置前的状态。</p>
      </div>
      <template #footer>
        <button class="btn ghost" @click="showRestoreDialog = false">取消</button>
        <button class="btn accent" :disabled="restoring" @click="restoreConfig">
          {{ restoring ? '恢复中...' : '确认恢复' }}
        </button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 智能体集成组件。
 * 职责：检测本机已安装的 AI 编程智能体，提供自动配置和恢复功能。
 * 缓存策略：检测结果缓存 1 小时，过期后自动重新检测。
 */
import { ref, computed, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { Refresh, Loading, Monitor, Document, WarningFilled, InfoFilled, Clock, Setting, RefreshLeft } from '@element-plus/icons-vue'
import logoDeepseek from '@/images/llm/deepseek.svg?url'
import logoClaude from '@/images/llm/claude.svg?url'
import logoOpencode from '@/images/llm/opencode.svg?url'
import logoCodex from '@/images/llm/codex.svg?url'
import logoQwen from '@/images/llm/qwen.svg?url'
import logoGemini from '@/images/llm/gemini.svg?url'
import logoCursor from '@/images/llm/cursor.svg?url'
import logoWindsurf from '@/images/llm/windsurf.svg?url'
import logoCopilot from '@/images/llm/copilot.svg?url'
import logoAider from '@/images/llm/aider.svg?url'
import logoZed from '@/images/llm/zed.svg?url'
import logoAmp from '@/images/llm/amp.svg?url'
import {
  detectAgents as apiDetectAgents,
  previewConfig as apiPreviewConfig,
  applyConfig as apiApplyConfig,
  restoreConfig as apiRestoreConfig,
  listBackups as apiListBackups,
  type AgentKind,
  type VortexConfigStatus,
  type FileOperation,
  type DetectedAgent,
  type FileChange,
  type ConfigPreview,
  type ConfigResult,
} from '@/api/agents'

// ─── 缓存 ───

const CACHE_KEY = 'vortex_agent_detection'
const CACHE_TTL_MS = 60 * 60 * 1000 // 1 小时

interface CacheEntry {
  agents: DetectedAgent[]
  timestamp: number
}

function loadCache(): CacheEntry | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    return JSON.parse(raw) as CacheEntry
  } catch {
    return null
  }
}

function saveCache(agents: DetectedAgent[]) {
  try {
    const entry: CacheEntry = { agents, timestamp: Date.now() }
    localStorage.setItem(CACHE_KEY, JSON.stringify(entry))
  } catch {
    // 忽略存储错误
  }
}

// ─── 状态 ───

const detecting = ref(false)
const agents = ref<DetectedAgent[]>([])
const lastDetected = ref<number | null>(null)

const configuring = ref<AgentKind | null>(null)
const applying = ref(false)
const preview = ref<ConfigPreview | null>(null)
const showPreview = ref(false)

const restoreAgent = ref<DetectedAgent | null>(null)
const showRestoreDialog = ref(false)
const restoring = ref(false)

// 缓存是否过期
const isCacheStale = computed(() => {
  if (!lastDetected.value) return true
  return Date.now() - lastDetected.value > CACHE_TTL_MS
})

// ─── 智能体 Logo ───

const agentLogoMap: Record<AgentKind, string> = {
  dsh: logoDeepseek,
  claude_code: logoClaude,
  opencode: logoOpencode,
  codex: logoCodex,
  qwen_code: logoQwen,
  gemini_cli: logoGemini,
  cursor_agent: logoCursor,
  windsurf: logoWindsurf,
  copilot: logoCopilot,
  aider: logoAider,
  zed: logoZed,
  amp: logoAmp,
}

// ─── 映射表 ───

const agentDisplayNames: Record<AgentKind, string> = {
  dsh: 'DeepSeek Harness',
  claude_code: 'Claude Code',
  opencode: 'OpenCode',
  codex: 'Codex',
  qwen_code: 'Qwen Code',
  gemini_cli: 'Gemini CLI',
  cursor_agent: 'Cursor Agent',
  windsurf: 'Windsurf',
  copilot: 'GitHub Copilot',
  aider: 'Aider',
  zed: 'Zed',
  amp: 'Amp',
}

const agentIcons: Record<AgentKind, string> = {
  dsh: 'D',
  claude_code: 'C',
  opencode: 'O',
  codex: 'Cx',
  qwen_code: 'Q',
  gemini_cli: 'G',
  cursor_agent: 'A',
  windsurf: 'W',
  copilot: 'Co',
  aider: 'Ai',
  zed: 'Z',
  amp: 'Am',
}

const agentIconClasses: Record<AgentKind, string> = {
  dsh: 'icon-dsh',
  claude_code: 'icon-claude',
  opencode: 'icon-opencode',
  codex: 'icon-codex',
  qwen_code: 'icon-qwen',
  gemini_cli: 'icon-gemini',
  cursor_agent: 'icon-cursor',
  windsurf: 'icon-windsurf',
  copilot: 'icon-copilot',
  aider: 'icon-aider',
  zed: 'icon-zed',
  amp: 'icon-amp',
}

const statusTextMap: Record<VortexConfigStatus, string> = {
  not_configured: '未配置',
  configured: '已接入',
  needs_update: '待更新',
  conflict: '冲突',
  failed: '失败',
}

const operationTextMap: Record<FileOperation, string> = {
  create: '新建',
  modify: '修改',
  delete: '删除',
}

// ─── 工具函数 ───

function getAgentDisplayName(kind: AgentKind): string {
  return agentDisplayNames[kind] || kind
}

function getAgentIcon(kind: AgentKind): string {
  return agentIcons[kind] || '?'
}

function getAgentLogo(kind: AgentKind): string | null {
  return agentLogoMap[kind] ?? null
}

function getIconClass(kind: AgentKind): string {
  return agentIconClasses[kind] || ''
}

function getStatusText(status: VortexConfigStatus): string {
  return statusTextMap[status] || status
}

function getOperationText(op: FileOperation): string {
  return operationTextMap[op] || op
}

function shortenPath(p: string): string {
  // 缩短路径显示：将 /Users/xxx 替换为 ~
  return p.replace(/^\/Users\/[^/]+/, '~')
}

function formatTime(ts: number): string {
  const d = new Date(ts)
  const now = new Date()
  const diffMs = now.getTime() - d.getTime()
  const diffMin = Math.floor(diffMs / 60000)

  if (diffMin < 1) return '刚刚'
  if (diffMin < 60) return `${diffMin} 分钟前`
  const diffHour = Math.floor(diffMin / 60)
  if (diffHour < 24) return `${diffHour} 小时前`
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

// ─── 核心操作 ───

/** 排序：已安装的排在最前，其余保持原始顺序 */
function sortAgents(list: DetectedAgent[]): DetectedAgent[] {
  return [...list].sort((a, b) => Number(b.installed) - Number(a.installed))
}

async function detectAgents(force = false) {
  // 如果有缓存且未过期且非强制，直接使用缓存
  if (!force) {
    const cached = loadCache()
    if (cached && Date.now() - cached.timestamp < CACHE_TTL_MS) {
      agents.value = sortAgents(cached.agents)
      lastDetected.value = cached.timestamp
      return
    }
  }

  detecting.value = true
  try {
    const result = await apiDetectAgents()
    agents.value = sortAgents(result.agents)
    lastDetected.value = Date.now()
    saveCache(result.agents)
  } catch (e) {
    console.error('检测智能体失败:', e)
  } finally {
    detecting.value = false
  }
}

async function previewConfig(agent: DetectedAgent) {
  configuring.value = agent.kind
  try {
    const result = await apiPreviewConfig(agent.kind)
    preview.value = result.preview
    showPreview.value = true
  } catch (e) {
    console.error('预览配置失败:', e)
  } finally {
    configuring.value = null
  }
}

async function applyConfig() {
  if (!preview.value) return
  applying.value = true
  try {
    const result = await apiApplyConfig(preview.value.agent, true)
    if (result.result.success) {
      const agent = agents.value.find((a) => a.kind === preview.value!.agent)
      if (agent) agent.vortex_status = 'configured'
      showPreview.value = false
      // 更新缓存
      saveCache(agents.value)
    } else {
      console.error('应用配置失败:', result.result.error)
    }
  } catch (e) {
    console.error('应用配置失败:', e)
  } finally {
    applying.value = false
  }
}

function showRestore(agent: DetectedAgent) {
  restoreAgent.value = agent
  showRestoreDialog.value = true
}

async function restoreConfig() {
  if (!restoreAgent.value || restoring.value) return
  restoring.value = true
  try {
    const backups = await apiListBackups()
    const latestBackup = backups.backups.find((b) => b.agent === restoreAgent.value!.kind)
    if (!latestBackup) {
      ElMessage.warning('未找到可恢复的备份')
      return
    }
    await apiRestoreConfig(restoreAgent.value.kind, latestBackup.id)
    restoreAgent.value.vortex_status = 'not_configured'
    showRestoreDialog.value = false
    saveCache(agents.value)
    ElMessage.success('配置已恢复')
  } catch (e) {
    ElMessage.error(`恢复失败: ${e}`)
  } finally {
    restoring.value = false
  }
}

// ─── 生命周期 ───

onMounted(() => {
  // 尝试从缓存加载
  const cached = loadCache()
  if (cached) {
    agents.value = sortAgents(cached.agents)
    lastDetected.value = cached.timestamp

    // 如果缓存已过期，自动重新检测
    if (Date.now() - cached.timestamp > CACHE_TTL_MS) {
      detectAgents(true)
    }
  } else {
    // 无缓存，首次检测
    detectAgents()
  }
})
</script>

<style scoped>
/* ─── 顶部操作栏 ─── */
.agent-toolbar {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: var(--gap-lg);
}

.toolbar-desc {
  font-size: 13px;
  color: var(--ink-3);
  margin: 0 0 4px;
  line-height: 1.5;
}

.toolbar-meta {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--ink-4);
}

.stale-hint {
  color: var(--warn);
}

.scan-btn {
  flex-shrink: 0;
}

/* ─── 卡片网格 ─── */
.agent-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--gap-md);
}

/* ─── 单张卡片 ─── */
.agent-card {
  display: flex;
  flex-direction: column;
  padding: var(--pad-card);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-sm);
  transition: border-color 0.2s, box-shadow 0.2s;
}

.agent-card:hover {
  border-color: var(--line-2);
  box-shadow: var(--shadow-md);
}

.agent-card.status-needs_update {
  border-color: var(--warn);
  background: var(--warn-bg);
}

.agent-card.status-conflict,
.agent-card.status-failed {
  border-color: var(--err);
  background: var(--err-bg);
}

.agent-card.not-installed {
  opacity: 0.85;
}

.agent-card.is-configuring {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-bg);
  pointer-events: none;
}

/* ─── 卡片顶部：图标 + 名称 + 状态 ─── */
.card-top {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.agent-icon-wrap {
  width: 36px;
  height: 36px;
  border-radius: var(--r-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  font-weight: 700;
  color: #fff;
  flex-shrink: 0;
  background: var(--ink-4);
  overflow: hidden;
}

.agent-icon-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}

.agent-icon-wrap.icon-dsh      { background: #0a0e2c; }
.agent-icon-wrap.icon-claude   { background: #2a1f1a; }
.agent-icon-wrap.icon-opencode { background: #0d0d0d; }
.agent-icon-wrap.icon-codex    { background: #0a0a0a; }
.agent-icon-wrap.icon-qwen     { background: #6d28d9; }
.agent-icon-wrap.icon-gemini   { background: #4285f4; }
.agent-icon-wrap.icon-cursor   { background: #1B1913; }
.agent-icon-wrap.icon-windsurf { background: #0b1e2d; }
.agent-icon-wrap.icon-copilot  { background: #0d1117; }
.agent-icon-wrap.icon-aider    { background: #0d1117; }
.agent-icon-wrap.icon-zed      { background: #1348DC; }
.agent-icon-wrap.icon-amp      { background: transparent; }

.agent-identity {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.agent-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.agent-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.installed-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--ok-bg);
  color: var(--ok);
  flex-shrink: 0;
  white-space: nowrap;
}

.agent-version {
  font-size: 11px;
  color: var(--ink-4);
  font-family: var(--el-font-family, monospace);
}

.status-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 10px;
  font-weight: 500;
  flex-shrink: 0;
  white-space: nowrap;
}

.status-badge.not_configured {
  background: var(--surface-3);
  color: var(--ink-3);
}

.status-badge.configured {
  background: var(--ok-bg);
  color: var(--ok);
}

.status-badge.needs_update {
  background: var(--warn-bg);
  color: var(--warn);
}

.status-badge.conflict,
.status-badge.failed {
  background: var(--err-bg);
  color: var(--err);
}

/* ─── 卡片中部：元信息 ─── */
.card-mid {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
}

.meta-row {
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
}

.meta-label {
  font-size: 11px;
  color: var(--ink-4);
  flex-shrink: 0;
  min-width: 28px;
}

.meta-value {
  font-size: 12px;
  color: var(--ink-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.meta-value.mono {
  font-family: var(--el-font-family, monospace);
}

.not-installed-hint {
  font-size: 12px;
  color: var(--ink-4);
  margin: 0;
}

.unsupported-hint {
  font-size: 11px;
  color: var(--ink-4);
  font-style: italic;
  margin: 4px 0 0;
}

/* ─── 卡片底部：操作按钮 ─── */
.card-bottom {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}

.action-btn {
  width: 100%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
}

.action-btn .el-icon.is-loading {
  animation: rotating 1.5s linear infinite;
}

@keyframes rotating {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.action-placeholder {
  width: 100%;
  height: 1px;
}

/* ─── 骨架屏 ─── */
.agent-card.skeleton {
  pointer-events: none;
}

.skeleton-icon {
  width: 36px;
  height: 36px;
  border-radius: var(--r-sm);
  background: var(--surface-3);
  animation: pulse 1.5s ease-in-out infinite;
}

.skeleton-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-left: 10px;
}

.skeleton-line {
  height: 12px;
  border-radius: 4px;
  background: var(--surface-3);
  animation: pulse 1.5s ease-in-out infinite;
}

.skeleton-line.w60 { width: 60%; }
.skeleton-line.w40 { width: 40%; }
.skeleton-line.w80 { width: 80%; }

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

/* ─── 空状态 ─── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
}

.empty-icon {
  color: var(--ink-4);
  margin-bottom: 12px;
}

.empty-text {
  font-size: 14px;
  color: var(--ink-2);
  margin: 0 0 6px;
}

.empty-hint {
  font-size: 13px;
  color: var(--ink-4);
  margin: 0;
}

/* ─── 对话框内容 ─── */
.preview-content {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.preview-desc {
  font-size: 14px;
  color: var(--ink);
  margin: 0;
}

.file-changes {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.file-change {
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface-2);
}

.file-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

.file-icon { font-size: 14px; color: var(--ink-4); }

.file-path {
  flex: 1;
  font-size: 12px;
  font-family: var(--el-font-family, monospace);
  color: var(--ink-2);
  word-break: break-all;
}

.file-op {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 8px;
  font-weight: 500;
}

.file-op.create { background: var(--ok-bg); color: var(--ok); }
.file-op.modify { background: var(--accent-bg); color: var(--accent); }
.file-op.delete { background: var(--err-bg); color: var(--err); }

.file-summary { font-size: 12px; color: var(--ink-3); }

.preview-warnings {
  display: flex;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid var(--warn);
  border-radius: var(--r-sm);
  background: var(--warn-bg);
}

.warning-icon { font-size: 18px; color: var(--warn); flex-shrink: 0; margin-top: 1px; }

.warning-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.warning-item { font-size: 12px; color: var(--ink-2); }

.preview-restart {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--accent-line);
  border-radius: var(--r-sm);
  background: var(--accent-bg);
  font-size: 12px;
  color: var(--ink-2);
}

.restart-icon { font-size: 14px; color: var(--accent); }

.restore-content {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.restore-hint {
  font-size: 13px;
  color: var(--ink-3);
  margin: 0;
}
</style>
