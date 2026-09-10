<template>
  <div class="wizard">
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="添加提供方" sub="从内置提供商中选择一个接入 AI 服务" />

    <div class="card">
      <div class="card-body form">
        <!-- 提供方选择 -->
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

        <!-- API 密钥：无需鉴权的提供方不显示 -->
        <div v-if="form.provider && !isNoAuth" class="field">
          <label class="field-label">API 密钥</label>
          <el-input
            v-model="form.apiKey"
            type="password"
            show-password
            :placeholder="authPlaceholder"
          />
          <div v-if="authHint" class="field-hint">{{ authHint }}</div>
        </div>

        <!-- 自定义设置：可折叠，用于覆盖默认 base_url -->
        <details class="custom-block" :open="needsBaseUrl">
          <summary>自定义设置</summary>
          <div class="custom-body">
            <div class="field">
              <label class="field-label">API 地址 <em v-if="needsBaseUrl">*</em></label>
              <el-input v-model="form.baseUrl" :placeholder="needsBaseUrl ? 'https://api.your-provider.com/v1' : '提供方默认'" />
              <div class="field-hint">{{ needsBaseUrl ? '请填写实际可用的 API 端点地址' : '仅在官方端点被墙或你想走代理/中转时填写' }}</div>
            </div>
          </div>
        </details>

        <!-- 模型目录：可拉取可用模型并多选 -->
        <div class="field">
          <div class="row-between">
            <span class="field-label static">模型目录</span>
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
          <div v-if="selectedModels.length" class="model-list">
            <div class="model-row" v-for="(m, i) in selectedModels" :key="m.id">
              <span class="model-idx" :class="{ primary: i === 0 }" :title="i === 0 ? '默认模型（用于路由回退）' : ''">{{ i === 0 ? '默认' : i + 1 }}</span>
              <span class="model-id mono" :title="m.id">{{ m.id }}</span>
              <el-input v-model="m.name" placeholder="自定义名称（可选）" size="small" class="model-name" />
              <button type="button" class="btn sm ghost" @click="removeModel(m.id)" title="移除">×</button>
            </div>
          </div>
          <div v-else class="model-line">尚未选择模型</div>
          <div class="model-hint">
            点击「获取可用模型」会基于上方 API 密钥与地址直接拉取，<b>无需先保存连接</b>；可多选，并为每个模型设置自定义名称。列表中<b>第一个</b>模型作为默认模型用于路由回退。
          </div>
        </div>
      </div>
    </div>

    <!-- 底部操作按钮 -->
    <div class="wizard-actions">
      <button type="button" class="btn" @click="$router.back()">取消</button>
      <button type="button" class="btn primary" :disabled="!canSubmit || saving" @click="submit">
        {{ saving ? '保存中…' : '添加模型' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 新建订阅页面。
 * 职责：从内置提供商列表中选择一个并接入，填写 API 密钥与可选的自定义地址，
 * 可预览可用模型并多选，列表首个模型作为默认模型用于路由回退。
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import PageHeader from '@/components/ui/PageHeader.vue'
import { listProviders, createProvider, previewModels, type ProviderDef } from '@/api/providers'

const router = useRouter()
// 内置提供商定义列表
const allProviders = ref<ProviderDef[]>([])
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

// 表单状态
const form = reactive({
  provider: '',
  apiKey: '',
  baseUrl: '',
  priority: 0,
})

// 当前选中的提供商定义
const selectedDef = computed(() => allProviders.value.find((p) => p.id === form.provider))
// 该提供方是否无需鉴权
const isNoAuth = computed(() => selectedDef.value?.noAuth ?? false)
// 是否为自定义 OpenAI 提供方（需要必填 base_url）
const needsBaseUrl = computed(() => form.provider === 'custom-openai')
// 鉴权提示文案
const authHint = computed(() => selectedDef.value?.authHint ?? '')

// API 密钥输入框 placeholder
const authPlaceholder = computed(() => (selectedDef.value?.id === 'amazon-bedrock' ? '输入 API 密钥，或留空使用环境认证' : 'sk-...'))

// 提供方提示文案：免鉴权 / 免费额度 / 通用鉴权提示
const providerHint = computed(() => {
  const p = selectedDef.value
  if (!p) return ''
  if (p.noAuth) return '此提供方无需 API 密钥（如本地推理）'
  if (p.hasFree) return '此提供方提供免费额度，可在应用内「免费 Token」页查看申请入口'
  return p.authHint || ''
})

// 是否可提交：至少选择了提供方
const canSubmit = computed(() => !!form.provider)

// 获取模型按钮可用条件：已选提供方，且（无需鉴权，或已填 API 密钥）
const canFetchModels = computed(() => !!form.provider && (isNoAuth.value || form.apiKey.trim() !== ''))

/**
 * 拉取当前提供方的可用模型列表。
 */
async function fetchModels() {
  if (!canFetchModels.value) return
  // 仅当用户自行填写了自定义地址时才做合法性校验；否则使用内置提供方的真实默认地址。
  const raw = form.baseUrl.trim()
  if (raw && !/^https?:\/\//i.test(raw)) {
    ElMessage.warning('自定义 API 地址需以 http:// 或 https:// 开头')
    return
  }
  fetchingModels.value = true
  try {
    const res = await previewModels({
      provider: form.provider,
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl || undefined,
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
 * 重置表单中与提供方相关的字段。
 */
function resetForm() {
  form.apiKey = ''
  form.baseUrl = ''
  selectedModels.value = []
  availableModels.value = []
}

/**
 * 提交创建连接。
 */
async function submit() {
  saving.value = true
  try {
    // 连接名称默认为「提供方名 连接」
    const name = selectedDef.value ? `${selectedDef.value.name} 连接` : '连接'
    // 模型列表：空名称会被过滤为 undefined
    const models = selectedModels.value.length
      ? selectedModels.value.map((m) => ({ id: m.id, name: m.name.trim() || undefined }))
      : undefined
    await createProvider({
      provider: form.provider,
      name,
      apiKey: form.apiKey || undefined,
      baseUrl: form.baseUrl || undefined,
      defaultModel: selectedModels.value[0]?.id || undefined, // 首个模型作为默认
      models,
      priority: form.priority,
    })
    router.push('/subscriptions')
  } finally {
    saving.value = false
  }
}

/**
 * 组件挂载时加载内置提供商定义列表。
 */
onMounted(async () => {
  try {
    const data = await listProviders()
    allProviders.value = data.providers ?? []
  } catch {
    /* ignore */
  }
})

// 切换提供方时重置密钥、地址与模型选择
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
.model-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
  padding: 10px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}
.model-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.model-idx {
  flex-shrink: 0;
  width: 34px;
  font-size: 11px;
  color: var(--ink-4);
  text-align: center;
}
.model-idx.primary {
  color: var(--ok);
  font-weight: 600;
}
.model-id {
  flex-shrink: 0;
  width: 150px;
  font-size: 12px;
  color: var(--ink-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.model-name {
  flex: 1;
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
.model-hint {
  font-size: 11.5px;
  color: var(--ink-4);
  padding: 10px 12px;
  background: var(--surface-2);
  border: 1px dashed var(--line);
  border-radius: var(--r-sm);
  text-align: center;
}
</style>
