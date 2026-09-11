<template>
  <div class="wizard max-w-560px mx-auto">
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="添加提供方" sub="从内置提供商中选择一个接入 AI 服务" />

    <div class="card">
      <div class="card-body form flex flex-col pt-24px">
        <!-- 提供方选择 -->
        <div class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">提供方</label>
          <el-select v-model="form.provider" placeholder="选择预置的提供商" filterable style="width: 100%">
            <el-option
              v-for="p in allProviders"
              :key="p.id"
              :label="p.name"
              :value="p.id"
            >
              <span style="float: left">{{ p.name }}</span>
              <span class="opt-id text-11px text-ink-4 ml-8px font-mono">{{ p.id }}</span>
              <span v-if="p.hasFree" class="provider-free float-right text-10px text-ok bg-ok-bg py-1px px-5px rounded-3px">免费</span>
            </el-option>
          </el-select>
          <div v-if="selectedDef" class="field-hint text-11.5px text-ink-4">{{ providerHint }}</div>
        </div>

        <!-- API 密钥：无需鉴权的提供方不显示 -->
        <div v-if="form.provider && !isNoAuth" class="field flex flex-col gap-6px">
          <label class="field-label text-12px font-medium text-ink-2">API 密钥</label>
          <el-input
            v-model="form.apiKey"
            type="password"
            show-password
            :placeholder="authPlaceholder"
          />
          <div v-if="authHint" class="field-hint text-11.5px text-ink-4">{{ authHint }}</div>
        </div>

        <!-- 自定义设置：可折叠，用于覆盖默认 base_url -->
        <details class="custom-block border-t border-line pt-14px" :open="needsBaseUrl">
          <summary>自定义设置</summary>
          <div class="custom-body flex flex-col gap-14px pt-14px pb-4px">
            <div class="field flex flex-col gap-6px">
              <label class="field-label text-12px font-medium text-ink-2">API 地址 <em v-if="needsBaseUrl">*</em></label>
              <el-input v-model="form.baseUrl" :placeholder="needsBaseUrl ? 'https://api.your-provider.com/v1' : '提供方默认'" />
              <div class="field-hint text-11.5px text-ink-4">{{ needsBaseUrl ? '请填写实际可用的 API 端点地址' : '仅在官方端点被墙或你想走代理/中转时填写' }}</div>
            </div>
          </div>
        </details>

        <!-- 模型目录：可拉取可用模型并多选 -->
        <div class="field flex flex-col gap-6px">
          <div class="row-between flex items-center justify-between">
            <span class="field-label static text-12px font-medium text-ink-2">模型目录</span>
            <button type="button" class="btn sm" :disabled="!canFetchModels || fetchingModels" @click="fetchModels">
              {{ fetchingModels ? '获取中…' : '获取可用模型' }}
            </button>
          </div>
          <!-- 已选模型列表：第一个为默认模型 -->
          <div v-if="selectedModels.length" class="model-list flex flex-col gap-6px mt-8px p-10px bg-surface-2 border border-line rounded-sm">
            <div class="model-row flex items-center gap-8px" v-for="(m, i) in selectedModels" :key="m.id">
              <span class="model-idx shrink-0 w-34px text-11px text-ink-4 text-center" :class="{ primary: i === 0 }" :title="i === 0 ? '默认模型（用于路由回退）' : ''">{{ i === 0 ? '默认' : i + 1 }}</span>
              <span class="model-id mono shrink-0 w-150px text-12px text-ink-2 whitespace-nowrap overflow-hidden text-ellipsis" :title="m.id">{{ m.id }}</span>
              <el-input v-model="m.name" placeholder="自定义名称（可选）" size="small" class="model-name flex-1" />
              <button type="button" class="btn sm ghost" @click="removeModel(m.id)" title="移除">×</button>
            </div>
          </div>
          <div v-else class="model-line text-12px text-ink-3 py-8px px-12px bg-surface-2 border border-line rounded-sm">尚未选择模型</div>
          <div class="model-actions flex items-center gap-10px mt-8px">
            <button type="button" class="btn sm" :disabled="!canFetchModels || fetchingModels || availableModels.length === 0" @click="modelDialogVisible = true">
              选择模型
            </button>
            <span class="model-hint-inline text-11.5px text-ink-4">
              点击「获取可用模型」从远程加载，再通过弹窗勾选；首个即默认模型。
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- 模型选择对话框 -->
    <ModelSelectDialog
      v-model:visible="modelDialogVisible"
      :models="availableModels"
      :selected="selIds"
      :loading="fetchingModels"
      @confirm="onModelConfirm"
    />

    <!-- 底部操作按钮 -->
    <div class="wizard-actions flex justify-end">
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
import ModelSelectDialog from '@/components/ui/ModelSelectDialog.vue'
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
// 模型选择对话框可见性
const modelDialogVisible = ref(false)

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
      modelDialogVisible.value = true
      ElMessage.success(`已获取 ${res.models.length} 个可用模型，请在弹窗中勾选`)
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
 * 模型选择对话框确认回调：更新已选模型列表，保留已有自定义名称。
 * @param ids 选中的模型 ID 列表（有序）
 */
function onModelConfirm(ids: string[]) {
  const next: { id: string; name: string }[] = []
  for (const id of ids) {
    const existing = selectedModels.value.find((m) => m.id === id)
    next.push(existing ? { ...existing } : { id, name: '' })
  }
  selectedModels.value = next
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
.form { gap: var(--gap-lg); }
.field-label.static { font-weight: 600; }
.wizard-actions { gap: var(--gap-sm); margin-top: var(--gap-lg); }

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
