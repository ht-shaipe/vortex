<template>
  <div class="wizard">
    <PageHeader title="自定义提供方" sub="接入任意 OpenAI 兼容端点" />

    <div class="card">
      <div class="card-body form">
        <div class="field">
          <label class="field-label">Provider ID <span class="req">*</span></label>
          <el-input v-model="form.customProviderId" placeholder="acme-gateway" />
          <div class="field-hint">以小写字母开头的标识，在请求中唯一标识该提供方，并用于派生凭据名。</div>
        </div>

        <div class="field">
          <label class="field-label">显示名称</label>
          <el-input v-model="form.displayName" placeholder="显示名称" />
        </div>

        <div class="field">
          <label class="field-label">API 地址 <span class="req">*</span></label>
          <el-input v-model="form.baseUrl" placeholder="https://gateway.example.com/v1" />
        </div>

        <div class="field">
          <label class="field-label">API 协议 <span class="req">*</span></label>
          <el-select v-model="form.apiProtocol" placeholder="选择协议" style="width: 100%">
            <el-option
              v-for="p in protocols"
              :key="p.value"
              :label="p.label"
              :value="p.value"
            />
          </el-select>
        </div>

        <div class="field">
          <label class="field-label">API 密钥</label>
          <el-input v-model="form.apiKey" type="password" show-password placeholder="输入 API 密钥" />
        </div>

        <div class="field">
          <div class="row-between">
            <span class="field-label static">模型目录</span>
            <button type="button" class="btn sm ghost" :disabled="!form.baseUrl || fetchingModels" @click="fetchModels">
              {{ fetchingModels ? '获取中…' : '获取可用模型' }}
            </button>
          </div>
          <div class="model-hint">模型选择器中将不显示任何模型；目录外 ID 仍可直接发送。</div>
        </div>

        <div class="field">
          <label class="field-label">默认模型</label>
          <el-input v-model="form.defaultModel" placeholder="可选，如 my-custom-model" />
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
import { computed, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/ui/PageHeader.vue'
import { createProvider } from '@/api/providers'

const router = useRouter()
const saving = ref(false)
const fetchingModels = ref(false)

const protocols = [
  { value: 'openai', label: 'openai-completions' },
  { value: 'anthropic', label: 'anthropic-messages' },
  { value: 'gemini', label: 'gemini-generate' },
  { value: 'cohere', label: 'cohere-v2' },
  { value: 'cloudflare', label: 'cloudflare-ai' },
]

const form = reactive({
  customProviderId: '',
  displayName: '',
  baseUrl: '',
  apiProtocol: 'openai',
  apiKey: '',
  defaultModel: '',
})

// Provider ID 校验：小写字母开头，仅含小写字母、数字、短横线，长度 3-30。
const customIdPattern = /^[a-z][a-z0-9-]{2,29}$/

const canSubmit = computed(() => {
  return customIdPattern.test(form.customProviderId)
    && !!form.baseUrl.trim()
    && !!form.apiProtocol
})

async function fetchModels() {
  fetchingModels.value = true
  try {
    ElMessage.info('请先保存连接，再在订阅详情里测试连接以拉取模型。')
  } finally {
    fetchingModels.value = false
  }
}

async function submit() {
  if (!canSubmit.value) {
    if (!customIdPattern.test(form.customProviderId)) {
      ElMessage.warning('Provider ID 需以小写字母开头，长度 3-30，仅含字母数字与短横线')
    } else if (!form.baseUrl.trim()) {
      ElMessage.warning('API 地址不能为空')
    }
    return
  }
  saving.value = true
  try {
    // 统一落到内置 custom-openai 槽位；用户自定义的 ID 通过 provider_specific_data.customId 存储。
    await createProvider({
      provider: 'custom-openai',
      name: form.displayName || form.customProviderId,
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl,
      defaultModel: form.defaultModel || undefined,
      displayName: form.displayName || undefined,
      apiProtocol: form.apiProtocol,
      customProviderId: form.customProviderId,
    })
    router.push('/subscriptions')
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.wizard { max-width: 560px; margin: 0 auto; }
.form { display: flex; flex-direction: column; gap: var(--gap-lg); padding-top: 24px; }
.field { display: flex; flex-direction: column; gap: 6px; }
.field-label { font-size: 12px; font-weight: 500; color: var(--ink-2); }
.field-label.static { font-weight: 600; }
.req { color: var(--err); margin-left: 2px; }
.field-hint { font-size: 11.5px; color: var(--ink-4); }
.wizard-actions { display: flex; justify-content: flex-end; gap: var(--gap-sm); margin-top: var(--gap-lg); }

.row-between { display: flex; align-items: center; justify-content: space-between; }

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