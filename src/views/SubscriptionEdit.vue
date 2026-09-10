<template>
  <div class="edit-root">
    <div class="page-bar">
      <button type="button" class="btn bare" @click="$router.push('/subscriptions')">
        <el-icon :size="15"><ArrowLeft /></el-icon>
      </button>
      <span class="page-head">{{ conn?.name ?? '订阅详情' }}</span>
      <StatusBadge v-if="conn" :tone="statusTone(conn)" :label="statusLabel(conn)" />
      <span class="spacer" />
      <StatusBadge v-if="testResult" :tone="testResult.status === 'ok' ? 'ok' : 'err'" :label="testResult.status === 'ok' ? `可用 · ${testResult.latencyMs}ms` : '不可用'" />
    </div>

    <el-scrollbar class="edit-scroll">
      <div class="page-col edit-col" v-if="conn">
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">基本信息</div>
              <div class="card-sub">提供商连接配置</div>
            </div>
          </div>
          <div class="card-body">
            <div class="kv">
              <div class="kv-row">
                <span class="kv-label">提供商</span>
                <ProviderLogo :name="conn.provider" :size="18" />
                <span class="mono">{{ conn.provider }}</span>
              </div>
              <div class="kv-row">
                <span class="kv-label">名称</span>
                <el-input v-model="form.name" style="max-width: 320px" />
              </div>
              <div class="kv-row">
                <span class="kv-label">API 密钥</span>
                <el-input v-model="form.apiKey" type="password" show-password placeholder="保持留空则不变更" style="max-width: 320px" />
              </div>
              <div class="kv-row">
                <span class="kv-label">默认模型</span>
                <el-input v-model="form.defaultModel" placeholder="如 gpt-4o" style="max-width: 320px" />
              </div>
              <div class="kv-row">
                <span class="kv-label">优先级</span>
                <el-input-number v-model="form.priority" :min="0" :max="100" />
              </div>
              <div class="kv-row">
                <span class="kv-label">启用</span>
                <button type="button" class="toggle" :class="{ on: form.isActive }" @click="form.isActive = !form.isActive" />
              </div>
            </div>
          </div>
        </div>

        <div class="actions">
          <button type="button" class="btn" :disabled="testing" @click="test">{{ testing ? '测试中…' : '测试连接' }}</button>
          <button type="button" class="btn danger" @click="remove">删除</button>
          <span class="spacer" />
          <button type="button" class="btn primary" :disabled="saving" @click="save">{{ saving ? '保存中…' : '保存' }}</button>
        </div>
      </div>
    </el-scrollbar>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft } from '@element-plus/icons-vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { getProvider, updateProvider, deleteProvider, testProvider, type ProviderConnection } from '@/api/providers'

const route = useRoute()
const router = useRouter()
const conn = ref<ProviderConnection | null>(null)
const saving = ref(false)
const testing = ref(false)
const testResult = ref<{ status: string; latencyMs?: number } | null>(null)

const form = reactive({ name: '', apiKey: '', defaultModel: '', priority: 0, isActive: true })

function statusLabel(c: ProviderConnection): string {
  if (!c.isActive) return '已禁用'
  const s = c.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) return '健康'
  if (['failed', 'error'].includes(s)) return '失败'
  return '未测试'
}
function statusTone(c: ProviderConnection): 'ok' | 'warn' | 'err' | 'neutral' {
  if (!c.isActive) return 'neutral'
  const s = c.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) return 'ok'
  if (['failed', 'error'].includes(s)) return 'err'
  return 'neutral'
}

async function load() {
  const data = await getProvider(route.params.id as string)
  conn.value = data
  Object.assign(form, {
    name: data.name,
    apiKey: '',
    defaultModel: data.defaultModel ?? '',
    priority: data.priority,
    isActive: data.isActive,
  })
}

async function save() {
  saving.value = true
  try {
    const updates: Record<string, unknown> = {
      name: form.name,
      defaultModel: form.defaultModel || undefined,
      priority: form.priority,
      isActive: form.isActive,
    }
    if (form.apiKey) updates.apiKey = form.apiKey
    await updateProvider(route.params.id as string, updates)
    await load()
  } finally {
    saving.value = false
  }
}

async function test() {
  testing.value = true
  try {
    testResult.value = await testProvider(route.params.id as string)
  } catch {
    testResult.value = { status: 'error' }
  } finally {
    testing.value = false
  }
}

async function remove() {
  await deleteProvider(route.params.id as string)
  router.push('/subscriptions')
}

onMounted(load)
</script>

<style scoped>
.edit-root { height: 100vh; display: flex; flex-direction: column; }
.page-bar { flex-shrink: 0; }
.spacer { flex: 1; }
.edit-scroll { flex: 1; }
.edit-col { padding: 24px; }
.section { margin-bottom: var(--gap-lg); }
.kv { display: flex; flex-direction: column; }
.kv-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: var(--pad-row) 0;
  border-bottom: 1px solid var(--line);
}
.kv-row:last-child { border-bottom: none; }
.kv-label {
  width: 120px;
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 500;
  color: var(--ink-3);
}
.actions { display: flex; align-items: center; gap: var(--gap-sm); }
</style>