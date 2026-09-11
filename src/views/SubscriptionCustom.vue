<template>
  <div class="wizard max-w-560px mx-auto">
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="自定义提供方" sub="接入任意 OpenAI 兼容端点" />

    <div class="card">
      <div class="card-body form flex flex-col gap-[var(--gap-lg)] pt-24px">
        <!-- Provider ID：唯一标识，用于请求路由与凭据派生 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">Provider ID <span class="req text-err ml-2px">*</span></label>
          <el-input v-model="form.customProviderId" placeholder="acme-gateway" />
          <div class="field-hint text-11.5px text-ink-4">以小写字母开头的标识，在请求中唯一标识该提供方，并用于派生凭据名。</div>
        </div>

        <!-- 显示名称 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">显示名称</label>
          <el-input v-model="form.displayName" placeholder="显示名称" />
        </div>

        <!-- API 地址 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">API 地址 <span class="req text-err ml-2px">*</span></label>
          <el-input v-model="form.baseUrl" placeholder="https://api.your-gateway.com/v1" />
        </div>

        <!-- API 协议选择 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">API 协议 <span class="req text-err ml-2px">*</span></label>
          <el-select v-model="form.apiProtocol" placeholder="选择协议" style="width: 100%">
            <el-option
              v-for="p in protocols"
              :key="p.value"
              :label="p.label"
              :value="p.value"
            />
          </el-select>
        </div>

        <!-- API 密钥 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">API 密钥</label>
          <el-input v-model="form.apiKey" type="password" show-password placeholder="输入 API 密钥" />
        </div>

        <!-- 接口路径：覆盖默认 /v1/chat/completions，如 z.ai Coding Plan 需填 /chat/completions -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">接口路径</label>
          <el-input v-model="form.chatPath" placeholder="/v1/chat/completions" />
          <div class="field-hint text-11.5px text-ink-4">补全接口路径后缀，默认 <span class="mono">/v1/chat/completions</span>。若上游无 <span class="mono">/v1</span> 前缀（如 Z.AI Coding Plan 为 <span class="mono">/chat/completions</span>），请改填对应路径。</div>
        </div>

        <!-- 模型目录：可拉取可用模型并多选 -->
        <div class="field flex flex-col gap-6px">
          <div class="row-between flex items-center justify-between">
            <span class="field-label static text-12px font-medium text-ink-2">模型目录</span>
            <button type="button" class="btn sm" :disabled="!canFetchModels || fetchingModels" @click="fetchModels">
              {{ fetchingModels ? '获取中…' : '获取可用模型' }}
            </button>
          </div>
          <el-select
            v-model="selIds"
            multiple
            filterable
            allow-create
            default-first-option
            :reserve-contaminant="true"
            placeholder="选择或输入模型（可多选）"
            style="width: 100%"
          >
            <el-option
              v-for="m in availableModels"
              :key="m"
              :label="m"
              :value="m"
            />
          </el-select>
          <!-- 已选模型列表：第一个为默认模型 -->
          <div v-if="selectedModels.length" class="model-list flex flex-col gap-6px mt-8px p-10px bg-surface-2 border border-line rounded-sm">
            <div class="model-row flex items-center gap-8px" v-for="(m, i) in selectedModels" :key="m.id">
              <span class="model-idx shrink-0 w-34px text-11px text-ink-4 text-center" :class="{ primary: i === 0 }" :title="i === 0 ? '默认模型（用于路由回退）' : ''">{{ i === 0 ? '默认' : i + 1 }}</span>
              <span class="model-id mono shrink-0 w-150px text-12px text-ink-2 whitespace-nowrap overflow-hidden text-ellipsis" :title="m.id">{{ m.id }}</span>
              <el-input v-model="m.name" placeholder="自定义名称（可选）" size="small" class="model-name flex-1" />
              <button type="button" class="btn sm ghost" @click="removeModel(m.id)" title="移除">×</button>
            </div>
          </div>
          <div v-else class="model-hint text-11.5px text-ink-4 py-10px px-12px bg-surface-2 border border-dashed border-line rounded-sm text-center">点击「获取可用模型」将基于上方地址与密钥直接拉取，<b>无需先保存连接</b>；可多选并为每个模型设置自定义名称。</div>
        </div>
      </div>
    </div>

    <!-- 底部操作按钮 -->
    <div class="wizard-actions flex justify-end gap-[var(--gap-sm)] mt-[var(--gap-lg)]">
      <button type="button" class="btn" @click="$router.back()">取消</button>
      <button type="button" class="btn primary" :disabled="!canSubmit || saving" @click="submit">
        {{ saving ? '保存中…' : '添加模型' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 自定义订阅页面。
 * 职责：接入任意 OpenAI 兼容端点，填写自定义 Provider ID、显示名称、API 地址、
 * 协议与密钥，可预览可用模型并多选，列表首个模型作为默认模型用于路由回退。
 */
import { computed, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/ui/PageHeader.vue'
import { createProvider, previewModels } from '@/api/providers'

const router = useRouter()
// 是否正在保存
const saving = ref(false)
// 是否正在拉取模型
const fetchingModels = ref(false)
// 拉取到的可用模型列表
const availableModels = ref<string[]>([])

/** 已选模型（含自定义名称），为提交与展示的真相来源 */
const selectedModels = ref<{ id: string; name: string }[]>([])
/** 多选框绑定值：与 selectedModels 同步 */
const selIds = computed<string[]>({
  get: () => selectedModels.value.map((m) => m.id),
  set: (ids) => {
    const next: { id: string; name: string }[] = []
    for (const id of ids) {
      const existing = selectedModels.value.find((m) => m.id === id)
      next.push(existing ? { ...existing } : { id, name: '' })
    }
    selectedModels.value = next
  },
})

// 可选的 API 协议
const protocols = [
  { value: 'openai-completions', label: 'openai-completions' },
  { value: 'openai-responses', label: 'openai-responses' },
  { value: 'anthropic', label: 'anthropic-messages' }
]

// 表单状态
const form = reactive({
  customProviderId: '',
  displayName: '',
  baseUrl: '',
  apiProtocol: 'openai-completions',
  apiKey: '',
  chatPath: '/v1/chat/completions',
})

// Provider ID 校验：小写字母开头，仅含小写字母、数字、短横线，长度 3-30。
const customIdPattern = /^[a-z][a-z0-9-]{2,29}$/

// 是否可提交：Provider ID 合法、API 地址与协议已填
const canSubmit = computed(() => {
  return customIdPattern.test(form.customProviderId)
    && !!form.baseUrl.trim()
    && !!form.apiProtocol
})

// 是否可拉取模型：API 地址已填
const canFetchModels = computed(() => !!form.baseUrl.trim())

/**
 * 拉取自定义端点的可用模型列表。
 */
async function fetchModels() {
  if (!canFetchModels.value) return
  const raw = form.baseUrl.trim()
  if (!/^https?:\/\//i.test(raw)) {
    ElMessage.warning('请先填写以 http:// 或 https:// 开头的真实 API 地址')
    return
  }
  fetchingModels.value = true
  try {
    const res = await previewModels({
      provider: 'custom-openai',
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl,
      apiProtocol: form.apiProtocol,
    })
    if (res.models && res.models.length) {
      availableModels.value = res.models
      ElMessage.success(`已获取 ${res.models.length} 个可用模型，请勾选需要的模型`)
    } else {
      availableModels.value = []
      ElMessage.warning(res.warning || '未返回模型列表，可手动输入模型 ID')
    }
  } catch (e: any) {
    const msg = e?.response?.data?.error || e?.message || '获取失败'
    ElMessage.error(`获取模型失败：${msg}`)
  } finally {
    fetchingModels.value = false
  }
}

/**
 * 从已选模型中移除指定模型。
 * @param id 模型 ID
 */
function removeModel(id: string) {
  selectedModels.value = selectedModels.value.filter((m) => m.id !== id)
}

/**
 * 提交创建自定义连接。
 */
async function submit() {
  if (!canSubmit.value) {
    // 校验失败时给出具体提示
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
    const models = selectedModels.value.length
      ? selectedModels.value.map((m) => ({ id: m.id, name: m.name.trim() || undefined }))
      : undefined
    await createProvider({
      provider: 'custom-openai',
      name: form.displayName || form.customProviderId,
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl,
      defaultModel: selectedModels.value[0]?.id || undefined, // 首个模型作为默认
      models,
      displayName: form.displayName || undefined,
      apiProtocol: form.apiProtocol,
      chatPath: form.chatPath || undefined,
      customProviderId: form.customProviderId,
    })
    router.push('/subscriptions')
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.field-label.static { font-weight: 600; }
.model-idx.primary {
  color: var(--ok);
  font-weight: 600;
}
.model-name :deep(.el-input__inner) { font-size: 12px; }
.btn.sm.ghost {
  border-color: var(--line);
  color: var(--ink-3);
  font-size: 14px;
  line-height: 1;
  padding: 0 8px;
  background: transparent;
}
.btn.sm.ghost:hover { color: var(--err); border-color: var(--err); }
</style>
