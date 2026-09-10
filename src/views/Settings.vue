<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="设置" sub="网关与界面配置" />

    <!-- 设置分类标签栏 -->
    <div class="tabs">
      <button v-for="t in tabs" :key="t.id" class="tab" :class="{ active: active === t.id }" @click="active = t.id">
        <el-icon :size="14" class="tab-icon"><component :is="t.icon" /></el-icon>{{ t.label }}
      </button>
    </div>

    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <template v-else>
      <!-- 通用设置：主题、启动端口、界面语言 -->
      <div v-if="active === 'general'" class="card section">
        <div class="card-head">
          <div><div class="card-title">通用</div><div class="card-sub">界面与语言</div></div>
        </div>
        <div class="card-body">
          <!-- 主题切换 -->
          <div class="setting-row">
            <div>
              <div class="setting-label">主题</div>
              <div class="setting-desc">跟随系统 / 浅色 / 深色</div>
            </div>
            <div class="radio-group">
              <button v-for="m in themes" :key="m.id" class="radio-option" :class="{ active: theme.mode.value === m.id }" @click="theme.setMode(m.id)">{{ m.label }}</button>
            </div>
          </div>
          <!-- 启动端口 -->
          <div class="setting-row">
            <div>
              <div class="setting-label">启动端口</div>
              <div class="setting-desc">网关 API 服务器监听端口，修改后重启代理生效</div>
            </div>
            <el-input-number v-model="form.proxy_port" :min="1024" :max="65535" :step="1" controls-position="right" style="width: 140px" @change="saveField('proxy_port', form.proxy_port)" />
          </div>
          <!-- 界面语言（当前仅中文） -->
          <div class="setting-row">
            <div>
              <div class="setting-label">界面语言</div>
              <div class="setting-desc">当前仅支持简体中文</div>
            </div>
            <StatusBadge tone="neutral" label="简体中文" :dot="false" />
          </div>
        </div>
      </div>

      <!-- 安全与访问：Token 鉴权与 CORS -->
      <div v-else-if="active === 'security'" class="card section">
        <div class="card-head">
          <div><div class="card-title">安全与访问</div><div class="card-sub">控制网关访问凭据与 CORS 响应头</div></div>
        </div>
        <div class="card-body">
          <!-- Token 鉴权开关 -->
          <div class="setting-row">
            <div>
              <div class="setting-label">Token 鉴权</div>
              <div class="setting-desc">开启后，请求必须在 <span class="mono">Authorization</span> 头携带正确 token</div>
            </div>
            <el-switch v-model="form.security.tokenAuth" @change="(v: boolean) => saveSecurity('tokenAuth', v)" />
          </div>

          <!-- 访问令牌：展示、复制与重新生成 -->
          <div v-if="form.security.tokenAuth" class="setting-row">
            <div>
              <div class="setting-label">访问令牌</div>
              <div class="setting-desc">请妥善保管，重新生成会使旧 token 失效</div>
            </div>
            <div class="token-controls">
              <el-input v-model="form.security.token" class="token-input" readonly placeholder="未生成" />
              <button type="button" class="btn sm" :disabled="!form.security.token" @click="copyToken">
                <el-icon :size="13"><CopyDocument /></el-icon> 复制
              </button>
              <button type="button" class="btn sm accent" @click="regenerateToken">
                <el-icon :size="13"><Refresh /></el-icon> 重新生成
              </button>
            </div>
          </div>

          <!-- CORS 跨域开关 -->
          <div class="setting-row">
            <div>
              <div class="setting-label">CORS 跨域</div>
              <div class="setting-desc">开启后，响应附加 CORS 头，允许指定跨域源访问</div>
            </div>
            <el-switch v-model="form.security.cors" @change="(v: boolean) => saveSecurity('cors', v)" />
          </div>

          <!-- 允许的来源 -->
          <div v-if="form.security.cors" class="setting-row">
            <div>
              <div class="setting-label">允许的来源</div>
              <div class="setting-desc">多个来源用逗号分隔，<span class="mono">*</span> 表示允许全部</div>
            </div>
            <div class="cors-controls">
              <el-input v-model="form.security.corsOrigins" class="cors-input" placeholder="*" @blur="saveSecurity('corsOrigins', form.security.corsOrigins)" />
              <span class="header-tag">Access-Control-Allow-Origin</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 高级设置：User-Agent 覆盖与危险区域 -->
      <template v-else>
        <!-- User-Agent 覆盖 -->
        <div class="card section">
          <div class="card-head">
            <div><div class="card-title">User-Agent 覆盖</div><div class="card-sub">自定义发往上游的 UA，清空则透传客户端原始 UA</div></div>
          </div>
          <div class="card-body">
            <!-- OpenAI 端点 UA -->
            <div class="setting-row">
              <div>
                <div class="setting-label">OpenAI 端点 UA</div>
                <div class="setting-desc">覆盖发往 OpenAI 兼容上游的 User-Agent 头</div>
              </div>
              <el-input v-model="form.openai_ua" placeholder="清空后透传客户端 UA" style="max-width: 360px" @blur="saveField('openai_ua', form.openai_ua)" />
            </div>
            <!-- Anthropic 端点 UA -->
            <div class="setting-row">
              <div>
                <div class="setting-label">Anthropic 端点 UA</div>
                <div class="setting-desc">覆盖发往 Anthropic 兼容上游的 User-Agent 头</div>
              </div>
              <el-input v-model="form.anthropic_ua" placeholder="清空后透传客户端 UA" style="max-width: 360px" @blur="saveField('anthropic_ua', form.anthropic_ua)" />
            </div>
          </div>
        </div>

        <!-- 危险区域：恢复出厂设置 -->
        <div class="danger-card section">
          <div class="danger-head">
            <el-icon :size="14"><WarningFilled /></el-icon>
            <span>危险区域</span>
          </div>
          <div class="danger-body">
            <div>
              <div class="setting-label">恢复出厂设置</div>
              <div class="setting-desc">清空所有订阅、API Key、请求日志与设置，不可撤销</div>
            </div>
            <button type="button" class="btn danger" @click="resetOpen = true">恢复出厂设置</button>
          </div>
        </div>
      </template>
    </template>

    <!-- 恢复出厂确认弹窗 -->
    <el-dialog v-model="resetOpen" title="恢复出厂设置" width="420">
      <p class="reset-warn">此操作将清空所有数据且不可撤销，确定继续吗？</p>
      <template #footer>
        <button type="button" class="btn" @click="resetOpen = false">取消</button>
        <button type="button" class="btn danger" :disabled="resetting" @click="doReset">{{ resetting ? '清理中…' : '确认恢复' }}</button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置页面。
 * 职责：管理网关与界面配置，分为通用（主题、端口、语言）、安全与访问
 * （Token 鉴权、CORS）、高级（User-Agent 覆盖、恢复出厂设置）三个分类。
 */
import { onMounted, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { CopyDocument, Loading, Lock, Refresh, Setting, Tools, WarningFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import { useTheme } from '@/composables/useTheme'
import { getSettings, updateSettings } from '@/api/settings'

// 主题组合式函数
const theme = useTheme()
// 可选主题列表
const themes = [
  { id: 'system' as const, label: '跟随系统' },
  { id: 'light' as const, label: '浅色' },
  { id: 'dark' as const, label: '深色' },
]
// 设置分类标签
const tabs = [
  { id: 'general', label: '通用', icon: Setting },
  { id: 'security', label: '安全与访问', icon: Lock },
  { id: 'advanced', label: '高级', icon: Tools },
]
// 当前选中的分类
const active = ref('general')
// 是否正在加载
const loading = ref(true)

// 表单状态
const form = reactive({
  proxy_port: 20128,
  openai_ua: '',
  anthropic_ua: '',
  security: {
    tokenAuth: false,
    token: '',
    cors: false,
    corsOrigins: '*',
  },
})

// 恢复出厂弹窗是否可见
const resetOpen = ref(false)
// 是否正在执行恢复出厂
const resetting = ref(false)

/**
 * 将后端返回的设置数据应用到表单。
 * @param data 后端设置对象
 */
function applySettings(data: Record<string, unknown>) {
  const g = (data.general ?? {}) as Record<string, unknown>
  if (g.proxy_port != null) form.proxy_port = Number(g.proxy_port)
  if (g.openai_ua != null) form.openai_ua = String(g.openai_ua)
  if (g.anthropic_ua != null) form.anthropic_ua = String(g.anthropic_ua)

  const s = (data.security ?? {}) as Record<string, unknown>
  if (s.tokenAuth != null) form.security.tokenAuth = Boolean(s.tokenAuth)
  if (s.token != null) form.security.token = String(s.token)
  if (s.cors != null) form.security.cors = Boolean(s.cors)
  if (s.corsOrigins != null) form.security.corsOrigins = String(s.corsOrigins)
}

/**
 * 加载设置数据。
 */
async function load() {
  loading.value = true
  try {
    const data = await getSettings()
    applySettings(data)
  } catch {
    /* 后端未就绪 */
  } finally {
    loading.value = false
  }
}

/**
 * 保存通用设置的单个字段。
 * @param key 字段名
 * @param value 字段值
 */
async function saveField(key: string, value: unknown) {
  try {
    await updateSettings({ general: { [key]: value } })
  } catch {
    ElMessage.error('保存失败')
  }
}

/**
 * 保存安全设置的单个字段。
 * @param key 字段名
 * @param value 字段值
 */
async function saveSecurity(key: string, value: unknown) {
  try {
    await updateSettings({ security: { [key]: value } })
  } catch {
    ElMessage.error('保存失败')
  }
}

/**
 * 生成 32 位十六进制随机 Token。
 */
function generateToken(): string {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * 复制当前 Token 到剪贴板。
 */
async function copyToken() {
  if (!form.security.token) return
  try {
    await navigator.clipboard.writeText(form.security.token)
    ElMessage.success('Token 已复制到剪贴板')
  } catch {
    ElMessage.error('复制失败，请手动选择复制')
  }
}

/**
 * 重新生成 Token 并保存。
 */
async function regenerateToken() {
  const next = generateToken()
  form.security.token = next
  try {
    await updateSettings({ security: { token: next } })
    ElMessage.success('已重新生成 Token')
  } catch {
    ElMessage.error('生成失败')
  }
}

/**
 * 执行恢复出厂设置。
 */
async function doReset() {
  resetting.value = true
  try {
    await updateSettings({ general: { reset: true } })
    ElMessage.success('已恢复出厂设置')
    resetOpen.value = false
    await load()
  } catch {
    ElMessage.error('操作失败')
  } finally {
    resetting.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.section { margin-bottom: var(--gap-lg); }
.save-row { display: flex; justify-content: flex-end; }

.danger-card {
  border: 1px solid var(--err);
  border-radius: var(--r-lg);
  background: var(--err-bg);
  padding: 16px 20px;
}
.danger-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--err);
  margin-bottom: 12px;
}
.danger-body {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

/* 安全与访问：与通用/高级一致的标准两栏行（复用全局 .setting-row） */
.mono { font-family: var(--font-mono); font-size: 12px; }
.token-controls,
.cors-controls {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: nowrap;
}
.token-input { width: 300px; flex: 0 0 auto; }
.cors-input { width: 260px; flex: 0 0 auto; }
.header-tag {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--ink-4);
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 3px 8px;
  white-space: nowrap;
}
</style>

<style>
.reset-warn { margin: 0; font-size: 14px; color: var(--ink-2); line-height: 1.6; }
</style>
