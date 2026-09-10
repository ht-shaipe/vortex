<template>
  <div class="wizard">
    <PageHeader title="添加提供方" sub="从内置提供商中选择一个接入 AI 服务" />

    <div class="card">
      <div class="card-body form">
        <div class="field">
          <label class="field-label">提供方</label>
          <el-select v-model="form.provider" placeholder="选择预置的提供商" filterable style="width: 100%">
            <el-option
              v-for="p in allProviders"
              :key="p.id"
              :label="p.name"
              :value="p.id"
            >
              <span style="float: left">{{ p.name }}</span>
              <span class="opt-id">{{ p.id }}</span>
              <span v-if="p.hasFree" class="provider-free">免费</span>
            </el-option>
          </el-select>
          <div v-if="selectedDef" class="field-hint">{{ providerHint }}</div>
        </div>

        <div v-if="form.provider && !isNoAuth" class="field">
          <label class="field-label">API 密钥</label>
          <el-input
            v-model="form.apiKey"
            type="password"
            show-password
            :placeholder="selectedDef?.noAuth ? '本提供方无需密钥' : authPlaceholder"
          />
          <div v-if="authHint" class="field-hint">{{ authHint }}</div>
        </div>

        <details class="custom-block">
          <summary>自定义设置</summary>
          <div class="custom-body">
            <div class="field">
              <label class="field-label">API 地址</label>
              <el-input v-model="form.baseUrl" placeholder="提供方默认" />
              <div class="field-hint">仅在官方端点被墙或你想走代理/中转时填写</div>
            </div>
          </div>
        </details>

        <div class="field">
          <div class="row-between">
            <span class="field-label static">模型目录</span>
            <button type="button" class="btn sm ghost" :disabled="!form.provider || fetchingModels" @click="fetchModels">
              {{ fetchingModels ? '获取中…' : '获取可用模型' }}
            </button>
          </div>
          <div class="model-line">正在使用适配器默认模型</div>
          <div class="model-hint">模型选择器中将不显示任何模型；目录外 ID 仍可直接发送。</div>
        </div>

        <div class="field">
          <label class="field-label">默认模型</label>
          <el-input v-model="form.defaultModel" placeholder="可选，如 gpt-4o" />
        </div>
      </div>
    </div>

    <div class="wizard-actions">
      <button type="button" class="btn" @click="$router.back()">取消</button>
      <button type="button" class="btn primary" :disabled="!canSubmit || saving" @click="submit">
        {{ saving ? '保存中…' : '添加模型' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/ui/PageHeader.vue'
import { listProviders, createProvider, type ProviderDef } from '@/api/providers'

const router = useRouter()
const allProviders = ref<ProviderDef[]>([])
const saving = ref(false)
const fetchingModels = ref(false)

const form = reactive({
  provider: '',
  apiKey: '',
  baseUrl: '',
  defaultModel: '',
  priority: 0,
})

const selectedDef = computed(() => allProviders.value.find((p) => p.id === form.provider))
const isNoAuth = computed(() => selectedDef.value?.noAuth ?? false)
const authHint = computed(() => selectedDef.value?.authHint ?? '')

const authPlaceholder = computed(() => (selectedDef.value?.id === 'amazon-bedrock' ? '输入 API 密钥，或留空使用环境认证' : 'sk-...'))

const providerHint = computed(() => {
  const p = selectedDef.value
  if (!p) return ''
  if (p.noAuth) return '此提供方无需 API 密钥（如本地推理）'
  if (p.hasFree) return '此提供方提供免费额度，可在应用内「免费 Token」页查看申请入口'
  return p.authHint || ''
})

const canSubmit = computed(() => !!form.provider)

function resetForm() {
  form.apiKey = ''
  form.baseUrl = ''
  form.defaultModel = ''
}

async function fetchModels() {
  if (!form.provider) return
  fetchingModels.value = true
  try {
    // 模型目录实时获取依赖后端 GET /providers/:id/models；当前接口尚未暴露。
    // 先提示用户回到该提供方的编辑页执行「测试连接」即可自动拉取模型。
    ElMessage.info('该功能即将上线；保存后可到订阅详情测试连接以拉取模型。')
  } finally {
    fetchingModels.value = false
  }
}

async function submit() {
  saving.value = true
  try {
    const name = selectedDef.value ? `${selectedDef.value.name} 连接` : '连接'
    await createProvider({
      provider: form.provider,
      name,
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl || undefined,
      defaultModel: form.defaultModel || undefined,
      priority: form.priority,
    })
    router.push('/subscriptions')
  } finally {
    saving.value = false
  }
}

onMounted(async () => {
  try {
    const data = await listProviders()
    allProviders.value = data.providers ?? []
  } catch {
    /* ignore */
  }
})

// 选项变化时重置 provider 专属字段
import { watch } from 'vue'
watch(() => form.provider, () => resetForm())
</script>

<style scoped>
.wizard { max-width: 560px; margin: 0 auto; }
.form { display: flex; flex-direction: column; gap: var(--gap-lg); padding-top: 24px; }
.field { display: flex; flex-direction: column; gap: 6px; }
.field-label { font-size: 12px; font-weight: 500; color: var(--ink-2); }
.field-label.static { font-weight: 600; }
.field-hint { font-size: 11.5px; color: var(--ink-4); }
.wizard-actions { display: flex; justify-content: flex-end; gap: var(--gap-sm); margin-top: var(--gap-lg); }

.row-between { display: flex; align-items: center; justify-content: space-between; }

.opt-id { font-size: 11px; color: var(--ink-4); margin-left: 8px; font-family: var(--font-mono); }
.provider-free {
  float: right;
  font-size: 10px;
  color: var(--ok);
  background: var(--ok-bg);
  padding: 1px 5px;
  border-radius: 3px;
}

.custom-block {
  border-top: 1px solid var(--line);
  padding-top: 14px;
}
.custom-block > summary {
  font-size: 12px;
  font-weight: 600;
  color: var(--ink-2);
  cursor: pointer;
  list-style: none;
  display: flex;
  align-items: center;
  gap: 4px;
}
.custom-block > summary::before {
  content: '▸';
  font-size: 10px;
  color: var(--ink-3);
  transition: transform 0.15s;
}
.custom-block[open] > summary::before { transform: rotate(90deg); }
.custom-block > summary::-webkit-details-marker { display: none; }
.custom-body { padding: 14px 0 4px; display: flex; flex-direction: column; gap: 14px; }

.model-line {
  font-size: 12px;
  color: var(--ink-3);
  padding: 8px 12px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}
.model-hint {
  font-size: 11.5px;
  color: var(--ink-4);
  padding: 10px 12px;
  background: var(--surface-2);
  border: 1px dashed var(--line);
  border-radius: var(--r-sm);
  text-align: center;
}

.btn.sm.ghost {
  background: transparent;
  border-color: transparent;
  color: var(--accent-ink);
}
</style>