<template>
  <div class="wizard">
    <PageHeader title="新建订阅" sub="添加一个 AI 提供商连接" />

    <div class="card">
      <div class="card-body form">
        <div class="field">
          <label class="field-label">提供商</label>
          <el-select v-model="form.provider" placeholder="选择提供商" style="width: 100%">
            <el-option v-for="p in providers" :key="p.id" :label="`${p.name} (${p.id})`" :value="p.id">
              <div class="opt">
                <ProviderLogo :name="p.id" :color="p.color" :size="18" />
                <span>{{ p.name }}</span>
                <span class="opt-id mono">{{ p.id }}</span>
              </div>
            </el-option>
          </el-select>
        </div>

        <div class="field">
          <label class="field-label">名称</label>
          <el-input v-model="form.name" :placeholder="autoName" />
          <div class="field-hint">留空将自动生成名称</div>
        </div>

        <div class="field">
          <label class="field-label">API 密钥</label>
          <el-input v-model="form.apiKey" type="password" show-password placeholder="sk-..." />
        </div>

        <div class="field">
          <label class="field-label">默认模型</label>
          <el-input v-model="form.defaultModel" placeholder="可选，如 gpt-4o" />
        </div>

        <div class="field">
          <label class="field-label">优先级</label>
          <el-input-number v-model="form.priority" :min="0" :max="100" />
        </div>
      </div>
    </div>

    <div class="wizard-actions">
      <button type="button" class="btn" @click="$router.back()">取消</button>
      <button type="button" class="btn primary" :disabled="!canSubmit || saving" @click="submit">
        {{ saving ? '保存中…' : '保存订阅' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import PageHeader from '@/components/ui/PageHeader.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listProviders, createProvider, type ProviderDef } from '@/api/providers'

const router = useRouter()
const providers = ref<ProviderDef[]>([])
const saving = ref(false)

const form = reactive({
  provider: '',
  name: '',
  apiKey: '',
  defaultModel: '',
  priority: 0,
})

const autoName = computed(() => {
  const p = providers.value.find((x) => x.id === form.provider)
  return p ? `${p.name} 连接` : '连接名称'
})

const canSubmit = computed(() => !!form.provider)

async function submit() {
  saving.value = true
  try {
    await createProvider({
      provider: form.provider,
      name: form.name || autoName.value,
      apiKey: form.apiKey || undefined,
      defaultModel: form.defaultModel || undefined,
      priority: form.priority,
    })
    router.push('/subscriptions')
  } finally {
    saving.value = false
  }
}

onMounted(async () => {
  const data = await listProviders()
  providers.value = data.providers ?? []
})
</script>

<style scoped>
.wizard { max-width: 560px; margin: 0 auto; }
.form { display: flex; flex-direction: column; gap: var(--gap-lg); padding-top: 24px; }
.field { display: flex; flex-direction: column; gap: 6px; }
.wizard-actions { display: flex; justify-content: flex-end; gap: var(--gap-sm); margin-top: var(--gap-lg); }
.opt { display: flex; align-items: center; gap: 8px; }
.opt-id { color: var(--ink-4); font-size: 11px; margin-left: auto; }
</style>