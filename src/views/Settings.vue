<template>
  <div>
    <PageHeader title="设置" sub="网关与界面配置" />

    <div class="tabs">
      <button v-for="t in tabs" :key="t.id" class="tab" :class="{ active: active === t.id }" @click="active = t.id">
        <el-icon :size="14" class="tab-icon"><component :is="t.icon" /></el-icon>{{ t.label }}
      </button>
    </div>

    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <template v-else>
      <!-- 通用 -->
      <div v-if="active === 'general'" class="card section">
        <div class="card-head">
          <div><div class="card-title">通用</div><div class="card-sub">界面与语言</div></div>
        </div>
        <div class="card-body">
          <div class="setting-row">
            <div>
              <div class="setting-label">主题</div>
              <div class="setting-desc">跟随系统 / 浅色 / 深色</div>
            </div>
            <div class="radio-group">
              <button v-for="m in themes" :key="m.id" class="radio-option" :class="{ active: theme.mode.value === m.id }" @click="theme.setMode(m.id)">{{ m.label }}</button>
            </div>
          </div>
          <div class="setting-row">
            <div>
              <div class="setting-label">启动端口</div>
              <div class="setting-desc">网关 API 服务器监听端口，修改后重启代理生效</div>
            </div>
            <el-input-number v-model="form.proxy_port" :min="1024" :max="65535" :step="1" controls-position="right" style="width: 140px" @change="saveField('proxy_port', form.proxy_port)" />
          </div>
          <div class="setting-row">
            <div>
              <div class="setting-label">界面语言</div>
              <div class="setting-desc">当前仅支持简体中文</div>
            </div>
            <StatusBadge tone="neutral" label="简体中文" :dot="false" />
          </div>
        </div>
      </div>

      <!-- 高级 -->
      <template v-else>

        <!-- User-Agent 覆盖 -->
        <div class="card section">
          <div class="card-head">
            <div><div class="card-title">User-Agent 覆盖</div><div class="card-sub">自定义发往上游的 UA，清空则透传客户端原始 UA</div></div>
          </div>
          <div class="card-body">
            <div class="setting-row">
              <div>
                <div class="setting-label">OpenAI 端点 UA</div>
                <div class="setting-desc">覆盖发往 OpenAI 兼容上游的 User-Agent 头</div>
              </div>
              <el-input v-model="form.openai_ua" placeholder="清空后透传客户端 UA" style="max-width: 360px" @blur="saveField('openai_ua', form.openai_ua)" />
            </div>
            <div class="setting-row">
              <div>
                <div class="setting-label">Anthropic 端点 UA</div>
                <div class="setting-desc">覆盖发往 Anthropic 兼容上游的 User-Agent 头</div>
              </div>
              <el-input v-model="form.anthropic_ua" placeholder="清空后透传客户端 UA" style="max-width: 360px" @blur="saveField('anthropic_ua', form.anthropic_ua)" />
            </div>
          </div>
        </div>


        <!-- 危险区域 -->
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

    <!-- 恢复出厂确认 -->
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
import { onMounted, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Loading, Setting, Tools, WarningFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import { useTheme } from '@/composables/useTheme'
import { getSettings, updateSettings } from '@/api/settings'

const theme = useTheme()
const themes = [
  { id: 'system' as const, label: '跟随系统' },
  { id: 'light' as const, label: '浅色' },
  { id: 'dark' as const, label: '深色' },
]
const tabs = [
  { id: 'general', label: '通用', icon: Setting },
  { id: 'advanced', label: '高级', icon: Tools },
]
const active = ref('general')
const loading = ref(true)

const form = reactive({
  proxy_port: 20128,
  openai_ua: '',
  anthropic_ua: '',
})

const resetOpen = ref(false)
const resetting = ref(false)

function applySettings(data: Record<string, unknown>) {
  const g = (data.general ?? {}) as Record<string, unknown>
  if (g.proxy_port != null) form.proxy_port = Number(g.proxy_port)
  if (g.openai_ua != null) form.openai_ua = String(g.openai_ua)
  if (g.anthropic_ua != null) form.anthropic_ua = String(g.anthropic_ua)
}

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

async function saveField(key: string, value: unknown) {
  try {
    await updateSettings({ general: { [key]: value } })
  } catch {
    ElMessage.error('保存失败')
  }
}


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
</style>

<style>
.reset-warn { margin: 0; font-size: 14px; color: var(--ink-2); line-height: 1.6; }
</style>
