<template>
  <div class="edit-root flex-1 min-h-0 flex flex-col">
    <!-- 顶部标题栏：返回按钮、连接名、状态徽标、ID 与测试结果 -->
    <div class="page-bar shrink-0 flex items-center gap-10px px-28px bg-surface border-b border-line border-solid border-0">
      <button type="button" class="btn bare" @click="$router.push('/subscriptions')">
        <el-icon :size="18"><ArrowLeft /></el-icon>
      </button>
      <span class="page-head text-15px font-semibold text-ink min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">{{ conn?.name ?? '订阅详情' }}</span>
      <StatusBadge v-if="conn" :tone="statusTone(conn)" :label="statusLabel(conn)" />
      <span class="spacer flex-1" />
      <span v-if="conn" class="mono conn-id text-11px text-ink-4 py-2px px-6px bg-surface-2 rounded-4px shrink-0">{{ conn.id }}</span>
      <StatusBadge v-if="testResult" :tone="testResult.status === 'ok' ? 'ok' : 'err'" :label="testResult.status === 'ok' ? `可用 · ${testResult.latencyMs}ms` : '不可用'" />
    </div>

    <el-scrollbar class="edit-scroll flex-1">
      <div class="page-col edit-col pt-16px px-24px pb-32px max-w-880px mx-auto w-full" v-if="conn">
        <!-- 测试失败的醒目提示 -->
        <div v-if="testResult && testResult.status !== 'ok'" class="callout err flex flex-col gap-4px py-10px px-14px rounded-md border border-line text-12.5px">
          <div class="callout-title font-medium text-12px uppercase tracking-0.04em">连接测试失败</div>
          <div class="callout-body text-ink-2 break-words">{{ testResult.error || '未返回详细错误' }}</div>
        </div>

        <!-- 基本信息 -->
        <div class="card section">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">基本信息</div>
              <div class="card-sub">连接的展示名、提供方与默认启用状态</div>
            </div>
          </div>
          <div class="card-body">
            <!-- 提供方：只读 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">提供方</div>
                <div class="setting-desc">底层服务识别符，由创建时决定，不可修改</div>
              </div>
              <div class="row flex items-center gap-8px">
                <ProviderLogo :name="conn.provider" :hint="`${conn.name} ${conn.baseUrl || ''}`" :size="20" />
                <span class="mono">{{ conn.provider }}</span>
                <span class="auth-tag text-11px text-ink-3 bg-surface-2 py-2px px-8px rounded-full border border-line">{{ authTypeLabel }}</span>
              </div>
            </div>
            <!-- 名称：可编辑 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">名称</div>
                <div class="setting-desc">显示在订阅列表中的别称</div>
              </div>
              <el-input v-model="form.name" style="max-width: 360px" />
            </div>
            <!-- 显示名称：可选 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">显示名称</div>
                <div class="setting-desc">用于在内部 UI 中区分多个同名提供方</div>
              </div>
              <el-input v-model="form.displayName" placeholder="可选" style="max-width: 360px" />
            </div>
            <!-- 自定义 ID：仅自定义提供方显示，只读 -->
            <div v-if="isCustom" class="setting-row">
              <div>
                <div class="setting-label">自定义 ID</div>
                <div class="setting-desc">自定义提供方的唯一标识，只能在创建时设置</div>
              </div>
              <span class="mono readonly">{{ conn.customProviderId || '—' }}</span>
            </div>
            <!-- 启用开关 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">启用</div>
                <div class="setting-desc">关闭后该连接不参与路由，仅作保留</div>
              </div>
              <button type="button" class="toggle" :class="{ on: form.isActive }" @click="form.isActive = !form.isActive" />
            </div>
          </div>
        </div>

        <!-- 认证凭据 -->
        <div class="card section">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">认证凭据</div>
              <div class="card-sub">API 密钥与 OAuth 令牌（仅 API Key 字段可直接覆盖，其余为只读）</div>
            </div>
          </div>
          <div class="card-body">
            <!-- API 密钥：可覆盖，留空保持原值 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">API 密钥</div>
                <div class="setting-desc">明文保存于本地数据库（已加密）；留空则保持原值</div>
              </div>
              <div class="field-controls flex items-center gap-10px shrink-0 min-w-0">
                <el-input
                  v-model="form.apiKey"
                  type="password"
                  show-password
                  :placeholder="hasApiKey ? '已设置（已掩码）— 留空保持不变' : '尚未配置密钥，填写后保存'"
                  style="max-width: 360px"
                />
                <!-- 已配置：展示掩码，hover 可看完整掩码串 -->
                <el-tooltip v-if="hasApiKey" :content="`当前：${conn.apiKey}`" placement="top">
                  <span class="mono masked text-12px text-ink-3 bg-surface-2 py-4px px-10px rounded-sm font-mono max-w-260px overflow-hidden text-ellipsis whitespace-nowrap">{{ conn.apiKey }}</span>
                </el-tooltip>
                <!-- 未配置：明确告知，不再用「请填写」暗示为空 -->
                <span v-else class="key-missing text-11.5px text-warn py-3px px-8px rounded-full border border-line bg-warn-bg shrink-0">
                  {{ hasAccessToken ? 'OAuth 令牌已配置' : '未配置密钥' }}
                </span>
              </div>
            </div>

            <!-- OAuth 只读信息（外部系统签发的 access/refresh token 在本系统不能直接编辑） -->
            <template v-if="authTypeLabel !== 'API Key'">
              <div class="setting-row">
                <div>
                  <div class="setting-label">Access Token</div>
                  <div class="setting-desc">外部登录后获取的访问令牌，本系统不开放编辑；如需变更请重新授权</div>
                </div>
                <span class="mono readonly">已加密存储（不可读）</span>
              </div>
              <div class="setting-row">
                <div>
                  <div class="setting-label">Refresh Token</div>
                  <div class="setting-desc">刷新令牌，过期后通过 OAuth 流程重新签发</div>
                </div>
                <span class="mono readonly">已加密存储（不可读）</span>
              </div>
            </template>
          </div>
        </div>

        <!-- 自定义协议（仅自定义提供方） -->
        <div v-if="isCustom" class="card section">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">自定义协议</div>
              <div class="card-sub">自定义网关地址与上游 API 协议；用于替换官方端点或接入第三方中转</div>
            </div>
          </div>
          <div class="card-body">
            <!-- API 协议选择 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">API 协议</div>
                <div class="setting-desc">决定鉴权方式（Bearer / x-api-key / query）与请求格式</div>
              </div>
              <el-select v-model="form.apiProtocol" style="width: 240px">
                <el-option label="openai-completions" value="openai-completions" />
                <el-option label="openai-responses" value="openai-responses" />
                <el-option label="anthropic-messages" value="anthropic-messages" />
              </el-select>
            </div>
            <!-- API 地址 -->
            <div class="setting-row col">
              <div>
                <div class="setting-label">API 地址</div>
                <div class="setting-desc">上游网关根地址，需以 <span class="mono">http://</span> 或 <span class="mono">https://</span> 开头</div>
              </div>
              <el-input v-model="form.baseUrl" placeholder="https://api.example.com/v1" />
            </div>
            <!-- 接口路径 -->
            <div class="setting-row col">
              <div>
                <div class="setting-label">接口路径</div>
                <div class="setting-desc">补全接口路径后缀，默认 <span class="mono">/v1/chat/completions</span>。若上游无 <span class="mono">/v1</span> 前缀（如 Z.AI Coding Plan 为 <span class="mono">/chat/completions</span>），请改填对应路径</div>
              </div>
              <el-input v-model="form.chatPath" placeholder="/v1/chat/completions" />
            </div>
          </div>
        </div>

        <!-- 模型列表 -->
        <div class="card section">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">模型列表</div>
              <div class="card-sub">该连接下挂载的模型，第一个作为默认模型用于路由回退</div>
            </div>
            <button type="button" class="btn sm" :disabled="!canFetchModels || fetchingModels" @click="fetchModels">
              {{ fetchingModels ? '获取中…' : '获取可用模型' }}
            </button>
          </div>
          <div class="card-body">
            <!-- 已选模型列表：首位为默认模型 -->
            <div v-if="selectedModels.length" class="model-list flex flex-col gap-6px mt-8px p-10px bg-surface-2 border border-line rounded-sm">
              <div class="model-row flex items-center gap-8px" v-for="(m, i) in selectedModels" :key="m.id">
                <span class="model-idx shrink-0 w-34px text-11px text-ink-4 text-center" :class="{ primary: i === 0 }">{{ i === 0 ? '默认' : i + 1 }}</span>
                <span class="model-id mono min-w-120px text-12px text-ink-2 whitespace-nowrap overflow-hidden text-ellipsis" :title="m.id">{{ m.id }}</span>
                <el-input v-model="m.name" placeholder="自定义名称（可选）" size="small" class="model-name flex-1" />
                <button type="button" class="btn sm ghost" @click="removeModel(m.id)" title="移除">×</button>
              </div>
            </div>
            <div v-else class="empty-tip text-12px text-ink-4 p-12px bg-surface-2 border border-dashed border-line rounded-sm mt-8px text-center">尚未选择任何模型，路由时将无法匹配该连接</div>
            <div class="model-actions flex items-center gap-12px mt-10px">
              <button type="button" class="btn sm" :disabled="!canFetchModels || fetchingModels || availableModels.length === 0" @click="modelDialogVisible = true">
                选择模型
              </button>
              <span class="setting-desc">
                点击「获取可用模型」从远程加载列表，再通过弹窗勾选；首位即默认模型。
              </span>
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

        <!-- 路由与限制 -->
        <div class="card section">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">路由与限制</div>
              <div class="card-sub">多连接调度时的优先级与并发/限流策略</div>
            </div>
          </div>
          <div class="card-body">
            <!-- 优先级 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">优先级</div>
                <div class="setting-desc">值越大越优先被路由选中（范围 0-100）</div>
              </div>
              <el-input-number v-model="form.priority" :min="0" :max="100" :step="1" controls-position="right" />
            </div>
            <!-- 分组 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">分组</div>
                <div class="setting-desc">供路由规则按组筛选，留空表示默认分组</div>
              </div>
              <el-input v-model="form.groupName" placeholder="默认分组" style="max-width: 240px" />
            </div>
            <!-- 最大并发 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">最大并发</div>
                <div class="setting-desc">同时转发请求上限，留空表示不限制</div>
              </div>
              <el-input-number v-model="form.maxConcurrent" :min="1" :step="1" controls-position="right" placeholder="不限" />
            </div>
            <!-- 限流保护开关 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">限流保护</div>
                <div class="setting-desc">遇到 429 时自动降级（指数退避）</div>
              </div>
              <button type="button" class="toggle" :class="{ on: form.rateLimitProtection }" @click="form.rateLimitProtection = !form.rateLimitProtection" />
            </div>
            <!-- 代理启用开关 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">代理启用</div>
                <div class="setting-desc">转发该连接时启用本地代理</div>
              </div>
              <button type="button" class="toggle" :class="{ on: form.proxyEnabled }" @click="form.proxyEnabled = !form.proxyEnabled" />
            </div>
            <!-- 健康检查间隔 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">健康检查间隔（秒）</div>
                <div class="setting-desc">后台定期探测该连接的频率</div>
              </div>
              <el-input-number v-model="form.healthCheckInterval" :min="30" :step="30" controls-position="right" />
            </div>
          </div>
        </div>

        <!-- 状态（只读） -->
        <div class="card section readonly-card bg-surface-2">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">状态</div>
              <div class="card-sub">由系统自动维护的运行状态字段</div>
            </div>
            <span class="readonly-tag text-11px text-ink-4 bg-surface border border-line rounded-full py-2px px-10px">只读</span>
          </div>
          <div class="card-body">
            <!-- 最近测试结果 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">最近测试</div>
                <div class="setting-desc">上次「测试连接」的结果</div>
              </div>
              <StatusBadge v-if="conn" :tone="testStatusTone" :label="testStatusLabel" />
            </div>
            <!-- 最近错误信息 -->
            <div v-if="conn?.lastError" class="setting-row">
              <div>
                <div class="setting-label">最近错误</div>
                <div class="setting-desc">{{ conn.lastErrorAt ? fmtTime(conn.lastErrorAt) : '无时间' }}</div>
              </div>
              <div class="error-text mono text-12px text-err py-6px px-12px rounded-sm max-w-480px break-words">{{ conn.lastError }}</div>
            </div>
            <!-- 累计使用次数 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">累计使用次数</div>
                <div class="setting-desc">成功转发的请求总数</div>
              </div>
              <span class="mono">{{ conn.consecutiveUseCount ?? 0 }}</span>
            </div>
            <!-- 退避等级 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">退避等级</div>
                <div class="setting-desc">最近一次连续失败后设置的指数退避级数</div>
              </div>
              <span class="mono">{{ conn.backoffLevel ?? 0 }}</span>
            </div>
            <!-- 限流到期时间 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">限流到期</div>
                <div class="setting-desc">若被限流，显示解锁时间</div>
              </div>
              <span class="mono">{{ conn.rateLimitedUntil ? fmtTime(conn.rateLimitedUntil) : '—' }}</span>
            </div>
          </div>
        </div>

        <!-- 元数据（只读） -->
        <div class="card section readonly-card bg-surface-2">
          <div class="card-head flex items-center justify-between gap-12px">
            <div>
              <div class="card-title">元数据</div>
              <div class="card-sub">账号归属与时间戳信息，仅供查阅</div>
            </div>
            <span class="readonly-tag text-11px text-ink-4 bg-surface border border-line rounded-full py-2px px-10px">只读</span>
          </div>
          <div class="card-body">
            <!-- 连接 ID -->
            <div class="setting-row">
              <div>
                <div class="setting-label">连接 ID</div>
                <div class="setting-desc">系统内部唯一标识</div>
              </div>
              <span class="mono small">{{ conn.id }}</span>
            </div>
            <!-- 账号邮箱 -->
            <div v-if="hasField('email')" class="setting-row">
              <div>
                <div class="setting-label">账号邮箱</div>
                <div class="setting-desc">OAuth 登录后绑定的邮箱</div>
              </div>
              <el-input v-model="form.email" placeholder="可选" style="max-width: 320px" />
            </div>
            <!-- Project ID -->
            <div v-if="hasField('projectId')" class="setting-row">
              <div>
                <div class="setting-label">Project ID</div>
                <div class="setting-desc">部分平台要求绑定 Project 用于计费</div>
              </div>
              <el-input v-model="form.projectId" placeholder="可选" style="max-width: 320px" />
            </div>
            <!-- 创建时间 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">创建于</div>
                <div class="setting-desc">该连接第一次入库的时间</div>
              </div>
              <span class="mono">{{ fmtTime(conn.createdAt) }}</span>
            </div>
            <!-- 更新时间 -->
            <div class="setting-row">
              <div>
                <div class="setting-label">更新于</div>
                <div class="setting-desc">任意字段最后一次写入的时间</div>
              </div>
              <span class="mono">{{ fmtTime(conn.updatedAt) }}</span>
            </div>
          </div>
        </div>

        <!-- 底部操作：测试、删除、保存 -->
        <div class="actions sticky bottom-0 flex items-center pt-14px pb-6px">
          <button type="button" class="btn" :disabled="testing" @click="test">{{ testing ? '测试中…' : '测试连接' }}</button>
          <button type="button" class="btn danger" @click="remove">删除</button>
          <span class="spacer flex-1" />
          <button type="button" class="btn primary" :disabled="saving" @click="save">{{ saving ? '保存中…' : '保存' }}</button>
        </div>
      </div>
    </el-scrollbar>
  </div>
</template>

<script setup lang="ts">
/**
 * 编辑订阅页面。
 * 职责：查看与编辑单个提供商连接的详情，包括基本信息、认证凭据、自定义协议、
 * 模型列表、路由与限制策略，以及只读的运行状态与元数据；支持测试连接与删除。
 */
import { computed, onMounted, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import ModelSelectDialog from '@/components/ui/ModelSelectDialog.vue'
import { getProvider, updateProvider, deleteProvider, testProvider, previewModels, type ProviderConnection } from '@/api/providers'

const route = useRoute()
const router = useRouter()
// 当前连接详情
const conn = ref<ProviderConnection | null>(null)
// 是否正在保存
const saving = ref(false)
// 是否正在测试连接
const testing = ref(false)
// 是否正在拉取模型
const fetchingModels = ref(false)
// 拉取到的可用模型列表
const availableModels = ref<string[]>([])
// 模型选择对话框可见性
const modelDialogVisible = ref(false)
// 最近一次测试结果
const testResult = ref<{ status: string; latencyMs?: number; error?: string } | null>(null)

/** 已选模型（含自定义名称） */
const selectedModels = ref<{ id: string; name: string }[]>([])
// 多选框绑定值：与 selectedModels 同步
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

/** 表单状态：覆盖 mask_connection 中所有可编辑字段 */
const form = reactive({
  name: '',
  displayName: '',
  apiKey: '',
  apiProtocol: 'openai-completions',
  baseUrl: '',
  chatPath: '/v1/chat/completions',
  email: '',
  projectId: '',
  groupName: '',
  priority: 0,
  maxConcurrent: null as number | null,
  rateLimitProtection: false,
  proxyEnabled: false,
  healthCheckInterval: 300,
  isActive: true,
})

// 是否为自定义提供方
const isCustom = computed(() => conn.value?.provider === 'custom-openai')
// 是否已配置 API 密钥：以后端 hasApiKey 为准，不再用掩码串是否为空推断
const hasApiKey = computed(() => !!conn.value?.hasApiKey)
// 是否已配置 OAuth access token
const hasAccessToken = computed(() => !!conn.value?.hasAccessToken)
// 认证类型标签
const authTypeLabel = computed(() => {
  const t = (conn.value?.authType || '').toLowerCase()
  if (t === 'apikey' || t === 'api_key') return 'API Key'
  if (t === 'oauth') return 'OAuth'
  return conn.value?.authType || '未指定'
})

/**
 * 判断连接是否拥有某字段（当前简化为始终允许编辑 email / projectId）。
 * @param _k 字段名
 */
function hasField(_k: string) {
  // 当前不依赖后端预声明字段集，简化：所有连接都允许编辑 email / projectId。
  return true
}

/**
 * 根据连接状态计算中文标签。
 * @param c 提供商连接
 */
function statusLabel(c: ProviderConnection): string {
  if (!c.isActive) return '已禁用'
  const s = c.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) {
    return typeof c.lastLatencyMs === 'number' ? fmtLatency(c.lastLatencyMs) : '通过'
  }
  if (['failed', 'error'].includes(s)) return '失败'
  return '未测试'
}

/**
 * 格式化耗时：小于 1000ms 用毫秒，否则折算为秒并保留一位小数。
 * @param ms 毫秒
 */
function fmtLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '通过'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}
/**
 * 根据连接状态计算徽标色调。
 * @param c 提供商连接
 */
function statusTone(c: ProviderConnection): 'ok' | 'warn' | 'err' | 'neutral' {
  if (!c.isActive) return 'neutral'
  const s = c.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) return 'ok'
  if (['failed', 'error'].includes(s)) return 'err'
  return 'neutral'
}
// 测试状态中文标签：健康时直接展示响应耗时，未测到耗时则显示「通过」
const testStatusLabel = computed(() => {
  if (!conn.value) return '未测试'
  const s = conn.value.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) {
    return typeof conn.value.lastLatencyMs === 'number' ? fmtLatency(conn.value.lastLatencyMs) : '通过'
  }
  if (['failed', 'error'].includes(s)) return '失败'
  return '未测试'
})
// 测试状态徽标色调
const testStatusTone = computed<'ok' | 'err' | 'neutral'>(() => {
  const s = conn.value?.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success'].includes(s)) return 'ok'
  if (['failed', 'error'].includes(s)) return 'err'
  return 'neutral'
})

/**
 * 加载连接详情并填充表单。
 */
async function load() {
  const data = await getProvider(route.params.id as string)
  conn.value = data
  // 初始化已选模型：优先用模型列表，回退到默认模型
  selectedModels.value =
    data.models && data.models.length
      ? data.models.map((m) => ({ id: m.id, name: m.name ?? '' }))
      : data.defaultModel
        ? [{ id: data.defaultModel, name: '' }]
        : []
  // 将后端数据回填到表单
  Object.assign(form, {
    name: data.name,
    displayName: data.displayName ?? '',
    apiKey: '',
    apiProtocol: data.apiProtocol ?? 'openai-completions',
    baseUrl: data.baseUrl ?? '',
    chatPath: data.chatPath ?? '/v1/chat/completions',
    email: data.email ?? '',
    projectId: data.projectId ?? '',
    groupName: data.groupName ?? '',
    priority: data.priority ?? 0,
    maxConcurrent: data.maxConcurrent ?? null,
    rateLimitProtection: !!data.rateLimitProtection,
    proxyEnabled: !!data.proxyEnabled,
    healthCheckInterval: data.healthCheckInterval ?? 300,
    isActive: !!data.isActive,
  })
}

// 是否可拉取模型：自定义提供方需填写 base_url
const canFetchModels = computed(() => {
  if (!conn.value) return false
  if (conn.value.provider === 'custom-openai') return !!form.baseUrl
  return true
})

/**
 * 拉取当前连接的可用模型列表。
 */
async function fetchModels() {
  if (!canFetchModels.value || !conn.value) return
  const raw = form.baseUrl?.trim()
  if (raw && !/^https?:\/\//i.test(raw)) {
    ElMessage.warning('自定义 API 地址需以 http:// 或 https:// 开头')
    return
  }
  fetchingModels.value = true
  try {
    const res = await previewModels({
      provider: conn.value.provider,
      apiKey: form.apiKey || undefined,
      baseUrl: raw || undefined,
      apiProtocol: form.apiProtocol || undefined,
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
 * 格式化 ISO 时间为 YYYY-MM-DD HH:mm。
 * @param iso ISO 时间字符串
 */
function fmtTime(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (isNaN(d.getTime())) return iso
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}
/**
 * 数字补零到两位。
 * @param n 待补零的数字
 */
function pad(n: number): string {
  return String(n).padStart(2, '0')
}

/**
 * 保存连接修改。
 */
async function save() {
  if (!conn.value) return
  saving.value = true
  try {
    // 组装模型列表：空名称过滤为 undefined
    const models = selectedModels.value.map((m) => ({
      id: m.id,
      name: m.name.trim() ? m.name.trim() : undefined,
    }))
    const updates: Record<string, unknown> = {
      name: form.name,
      displayName: form.displayName.trim() || null,
      models,
      priority: form.priority,
      maxConcurrent: form.maxConcurrent,
      rateLimitProtection: form.rateLimitProtection,
      proxyEnabled: form.proxyEnabled,
      healthCheckInterval: form.healthCheckInterval,
      isActive: form.isActive,
      email: form.email.trim() || null,
      projectId: form.projectId.trim() || null,
      groupName: form.groupName.trim() || null,
    }
    // 自定义提供方额外更新协议与地址
    if (isCustom.value) {
      updates.apiProtocol = form.apiProtocol
      updates.baseUrl = form.baseUrl.trim() || null
      updates.chatPath = form.chatPath.trim() || null
    }
    // 仅当填写了新密钥时才更新
    if (form.apiKey) updates.apiKey = form.apiKey
    await updateProvider(route.params.id as string, updates)
    ElMessage.success('已保存')
    await load()
  } finally {
    saving.value = false
  }
}

/**
 * 测试当前连接是否可用。
 */
async function test() {
  if (!conn.value) return
  testing.value = true
  testResult.value = null
  try {
    const result = await testProvider(route.params.id as string)
    testResult.value = {
      status: result.status,
      latencyMs: result.latencyMs,
      error: result.error,
    }
    await load()
  } catch (e: any) {
    // 优先显示后端返回的错误信息，兜底显示网络错误
    const backendError = e?.response?.data?.error
    const detail = backendError || e?.message || '未知错误'
    testResult.value = {
      status: 'error',
      error: `请求失败：${detail}`,
    }
  } finally {
    testing.value = false
  }
}

/**
 * 删除当前连接并返回列表页。
 */
async function remove() {
  await deleteProvider(route.params.id as string)
  router.push('/subscriptions')
}

onMounted(load)
</script>

<style scoped>
.section { margin-bottom: var(--gap-lg); }

/* 设置行：左侧标签区允许收缩，右侧控件不被挤压 */
.setting-row > :first-child { min-width: 0; }
.setting-row:not(.col) > :last-child { flex-shrink: 0; }

/* 纵向设置行：标签在上，控件占满整行 */
.setting-row.col { flex-direction: column; align-items: stretch; gap: 8px; }

.mono.readonly { font-size: 12px; color: var(--ink-3); background: var(--surface-2); padding: 4px 10px; border-radius: var(--r-sm); }
.error-text { background: rgba(220, 80, 80, 0.06); }

.model-id { flex: 0 1 220px; }
.model-idx.primary { color: var(--ok); font-weight: 600; }
.model-name :deep(.el-input__inner) { font-size: 12px; }
.model-actions .setting-desc { font-size: 12px; color: var(--ink-4); }

.btn.sm.ghost {
  border-color: var(--line);
  color: var(--ink-3);
  font-size: 14px;
  line-height: 1;
  padding: 0 8px;
  background: transparent;
}
.btn.sm.ghost:hover { color: var(--err); border-color: var(--err); }

/* 底部操作栏：吸底 + 渐变背景，长表单滚动时保存按钮始终可见 */
.actions {
  gap: var(--gap-sm);
  margin-top: var(--gap-md);
  background: linear-gradient(to top, var(--bg) 55%, transparent);
}

/* 统一数字输入宽度，各行控件右缘对齐 */
.card-body :deep(.el-input-number) { width: 150px; }
.callout { margin-bottom: var(--gap-md); }
.callout.err {
  background: rgba(220, 80, 80, 0.08);
  border-color: rgba(220, 80, 80, 0.35);
}
.callout.err .callout-title { color: #c04949; }
.callout.err .callout-body { color: var(--ink-2); font-family: var(--font-mono); }
</style>
