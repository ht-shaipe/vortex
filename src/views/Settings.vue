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
              <div class="setting-label">界面语言</div>
              <div class="setting-desc">当前仅支持简体中文</div>
            </div>
            <StatusBadge tone="neutral" label="简体中文" :dot="false" />
          </div>
        </div>
      </div>

      <div v-else-if="active === 'routing'" class="card section">
        <div class="card-head">
          <div><div class="card-title">路由配置</div><div class="card-sub">来自 /api/settings · routing</div></div>
        </div>
        <div class="card-body">
          <div v-if="routingEntries.length === 0" class="kv-empty">暂无路由配置项</div>
          <div v-for="e in routingEntries" :key="e.key" class="setting-row">
            <div><div class="setting-label">{{ e.key }}</div></div>
            <el-input v-model="e.value" style="max-width: 320px" />
          </div>
        </div>
      </div>

      <div v-else class="card section">
        <div class="card-head">
          <div><div class="card-title">高级</div><div class="card-sub">来自 /api/settings · general</div></div>
        </div>
        <div class="card-body">
          <div v-if="generalEntries.length === 0" class="kv-empty">暂无高级配置项</div>
          <div v-for="e in generalEntries" :key="e.key" class="setting-row">
            <div><div class="setting-label">{{ e.key }}</div></div>
            <el-input v-model="e.value" style="max-width: 320px" />
          </div>
        </div>
      </div>

      <div v-if="active !== 'general'" class="save-row">
        <button type="button" class="btn primary" :disabled="saving" @click="save">{{ saving ? '保存中…' : '保存设置' }}</button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { Loading, Setting, Grid, Tools } from '@element-plus/icons-vue'
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
  { id: 'routing', label: '路由', icon: Grid },
  { id: 'advanced', label: '高级', icon: Tools },
]
const active = ref('general')
const loading = ref(true)
const saving = ref(false)

interface Entry { key: string; value: string }
const generalEntries = reactive<Entry[]>([])
const routingEntries = reactive<Entry[]>([])

function toEntries(map: Record<string, unknown> | undefined): Entry[] {
  return Object.entries(map ?? {}).map(([key, value]) => ({ key, value: String(value ?? '') }))
}

async function load() {
  loading.value = true
  try {
    const data = await getSettings()
    generalEntries.splice(0, generalEntries.length, ...toEntries(data.general))
    routingEntries.splice(0, routingEntries.length, ...toEntries(data.routing))
  } catch {
    /* 后端未就绪 */
  } finally {
    loading.value = false
  }
}

function fromEntries(entries: Entry[]): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const e of entries) out[e.key] = e.value
  return out
}

async function save() {
  saving.value = true
  try {
    await updateSettings({ general: fromEntries(generalEntries), routing: fromEntries(routingEntries) })
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.section { margin-bottom: var(--gap-lg); }
.kv-empty { font-size: 12.5px; color: var(--ink-4); padding: 8px 0; }
.save-row { display: flex; justify-content: flex-end; }
</style>