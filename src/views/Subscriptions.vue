<template>
  <div class="mt-4px">
    <!-- 页面头部：标题与新增按钮 -->
    <PageHeader title="订阅管理" sub="管理各 AI 提供商的连接与 API 密钥">
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load">
          <el-icon :size="14"><Refresh /></el-icon>刷新
        </button>
        <router-link to="/subscriptions/new" class="btn">
          <el-icon :size="14"><Plus /></el-icon>添加提供方
        </router-link>
        <router-link to="/subscriptions/custom" class="btn primary">
          <el-icon :size="14"><Connection /></el-icon>添加自定义提供方
        </router-link>
      </template>
    </PageHeader>

    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap p-40px text-center text-ink-4">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <!-- 空态：引导用户添加第一个连接 -->
    <div v-else-if="connections.length === 0" class="card">
      <EmptyState title="还没有订阅" desc="从内置提供商列表中添加一个连接，或填写自定义端点接入 OpenAI 兼容服务">
        <div class="empty-actions flex gap-8px">
          <router-link to="/subscriptions/new" class="btn">添加提供方</router-link>
          <router-link to="/subscriptions/custom" class="btn primary">添加自定义提供方</router-link>
        </div>
      </EmptyState>
    </div>

    <!-- 订阅列表 -->
    <div v-else class="card">
      <!-- 工具条：搜索 / 计数 / 批量测试 -->
      <div class="list-bar flex items-center gap-10px px-14px py-10px border-b border-line">
        <el-input
          v-model="keyword"
          placeholder="搜索名称 / 提供方 / 模型"
          clearable
          size="small"
          class="list-search"
        />
        <span class="spacer flex-1" />
        <span class="list-count text-11.5px text-ink-4 shrink-0">共 {{ filtered.length }} 个连接</span>
        <button type="button" class="btn sm shrink-0" :disabled="testingAll || filtered.length === 0" @click="testAll">
          <el-icon v-if="testingAll" class="spin" :size="12"><Loading /></el-icon>
          {{ testingAll ? `测试中 ${progress.done}/${progress.total}` : '全部测试' }}
        </button>
      </div>

      <table class="table">
        <colgroup>
          <col style="width: 96px" />
          <col style="width: 240px" />
          <col style="width: 220px" />
          <col style="width: 56px" />
          <col style="width: 92px" />
          <col style="width: 118px" />
        </colgroup>
        <thead>
          <tr>
            <th>状态</th>
            <th>提供商</th>
            <th>模型</th>
            <th class="num">优先级</th>
            <th class="num">更新于</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="conn in filtered" :key="conn.id" @click="$router.push(`/subscriptions/${conn.id}`)">
            <!-- 状态列：健康时直接显示响应耗时（绿色），失败/禁用/未测试给出文字状态 -->
            <td>
              <div class="status-cell flex items-center gap-6px">
                <span v-if="isTesting(conn.id)" class="testing inline-flex items-center gap-4px text-11.5px text-ink-3">
                  <el-icon class="spin" :size="12"><Loading /></el-icon>测试中
                </span>
                <el-tooltip v-else :content="statusTip(conn)" placement="top" :show-after="200">
                  <span class="lat-pill" :class="statusTone(conn)">
                    <i class="lat-dot" aria-hidden="true" />
                    <span class="lat-text">{{ statusLabel(conn) }}</span>
                  </span>
                </el-tooltip>
                <el-tooltip v-if="statusTone(conn) === 'err' && conn.lastError" :content="conn.lastError" placement="top">
                  <el-icon class="err-icon shrink-0" :size="13"><WarningFilled /></el-icon>
                </el-tooltip>
              </div>
            </td>

            <!-- 提供商图标与名称 -->
            <td>
              <div class="prov-cell flex items-center gap-10px min-w-0">
                <ProviderLogo :name="conn.provider" :hint="`${conn.name} ${conn.baseUrl || ''}`" :color="provColor(conn.provider)" :size="24" />
                <div class="prov-meta min-w-0">
                  <div class="prov-name text-13px font-medium text-ink whitespace-nowrap overflow-hidden text-ellipsis" :title="conn.name">{{ conn.name }}</div>
                  <div class="prov-id mono text-11px text-ink-4 whitespace-nowrap overflow-hidden text-ellipsis" :title="conn.baseUrl || conn.provider">
                    {{ conn.baseUrl || conn.provider }}
                  </div>
                </div>
              </div>
            </td>

            <!-- 模型标签：超出 maxTags 折叠为 +N -->
            <td>
              <div v-if="conn.models && conn.models.length" class="model-tags flex flex-wrap gap-4px items-center">
                <span
                  v-for="(m, i) in visibleModels(conn)"
                  :key="m.id"
                  class="model-tag inline-flex items-center gap-4px text-11px py-1px px-6px bg-surface-2 border border-line rounded-8px text-ink-2 whitespace-nowrap max-w-150px overflow-hidden text-ellipsis"
                  :title="modelTagTitle(m)"
                >
                  <i v-if="i === 0" class="def-dot" aria-hidden="true" />
                  {{ modelTagLabel(m) }}
                </span>
                <el-tooltip
                  v-if="conn.models.length > maxTags"
                  :content="overflowModels(conn)"
                  placement="top"
                >
                  <span class="model-tag more inline-flex items-center text-11px py-1px px-6px bg-surface-2 border border-line rounded-8px text-ink-2 whitespace-nowrap">+{{ conn.models.length - maxTags }}</span>
                </el-tooltip>
              </div>
              <!-- 无模型列表时回退展示默认模型 -->
              <div
                v-else
                class="model-cell text-12.5px text-ink-2 whitespace-nowrap overflow-hidden text-ellipsis tabular-nums"
                :class="{ empty: !conn.defaultModel }"
              >
                <el-tooltip
                  v-if="conn.defaultModel"
                  :content="conn.defaultModel"
                  placement="top"
                >
                  <span>{{ conn.defaultModel }}</span>
                </el-tooltip>
                <span v-else>—</span>
              </div>
            </td>

            <!-- 优先级 -->
            <td class="num">{{ conn.priority }}</td>
            <!-- 更新时间 -->
            <td class="num">{{ fmtTime(conn.updatedAt || conn.createdAt) }}</td>

            <!-- 行内操作：直接测试健康状态 + 进入详情 -->
            <td class="action-cell">
              <div class="action-btns">
                <button
                  type="button"
                  class="btn sm"
                  :disabled="isTesting(conn.id) || testingAll"
                  @click.stop="testOne(conn)"
                >
                  {{ isTesting(conn.id) ? '测试中' : '测试' }}
                </button>
                <button type="button" class="btn sm" @click.stop="$router.push(`/subscriptions/${conn.id}`)">查看</button>
              </div>
            </td>
          </tr>

          <!-- 搜索无结果 -->
          <tr v-if="filtered.length === 0">
            <td colspan="6" class="text-center text-ink-4 py-24px text-12.5px">
              没有匹配「{{ keyword }}」的连接
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 订阅列表页面。
 *
 * 职责：展示所有 AI 提供商连接的列表，包括状态、提供商、模型、优先级与更新时间，
 * 并提供新增内置/自定义提供方的入口。点击行可跳转到连接详情。
 *
 * 与旧版的差异：状态不再只是「进去详情页才能测」——列表每行都有「测试」按钮，
 * 顶部可一键批量测试，测试完成后就地回写该行的状态徽标与错误信息，不必刷新页面。
 */
import { computed, onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Loading, Plus, Connection, Refresh, WarningFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listProviders, testProvider, type ProviderDef, type ProviderConnection } from '@/api/providers'

// 当前连接列表
const connections = ref<ProviderConnection[]>([])
// 内置提供商定义列表
const providers = ref<ProviderDef[]>([])
// 是否正在加载
const loading = ref(true)
// 搜索关键字
const keyword = ref('')
// 正在测试中的连接 ID → true
const testingMap = ref<Record<string, boolean>>({})
// 是否正在批量测试
const testingAll = ref(false)
// 批量测试进度
const progress = ref({ done: 0, total: 0 })

/**
 * 根据提供商 ID 查询其主题色。
 * @param id 提供商标识
 * @returns 颜色字符串，未找到则返回空串
 */
function provColor(id: string): string {
  return providers.value.find((p) => p.id === id)?.color ?? ''
}

/**
 * 按关键字过滤连接：匹配名称、显示名、提供方、分组与模型 ID/别名。
 */
const filtered = computed(() => {
  const k = keyword.value.trim().toLowerCase()
  if (!k) return connections.value
  return connections.value.filter((c) => {
    const parts = [
      c.name,
      c.displayName ?? '',
      c.provider,
      c.groupName ?? '',
      c.defaultModel ?? '',
      c.baseUrl ?? '',
      ...(c.models ?? []).map((m) => `${m.id} ${m.name ?? ''}`),
    ]
    return parts.join(' ').toLowerCase().includes(k)
  })
})

/**
 * 判断某连接是否正在测试中。
 * @param id 连接 ID
 */
function isTesting(id: string): boolean {
  return !!testingMap.value[id]
}

/**
 * 状态列展示文本。
 *
 * 健康态不再显示「健康」二字，而是直接显示最近一次测试的响应耗时（如 `128ms`），
 * 让用户一眼看出快慢；其余状态仍用文字。
 *
 * @param conn 提供商连接
 */
function statusLabel(conn: ProviderConnection): string {
  if (!conn.isActive) return '已禁用'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) {
    return typeof conn.lastLatencyMs === 'number' ? fmtLatency(conn.lastLatencyMs) : '可用'
  }
  if (['failed', 'error', 'invalid'].includes(s)) return '失败'
  return '未测试'
}

/**
 * 格式化耗时：小于 1000ms 用毫秒，否则折算为秒并保留一位小数。
 * @param ms 毫秒
 */
function fmtLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '可用'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}

/**
 * 根据连接状态计算徽标色调。
 * @param conn 提供商连接
 */
function statusTone(conn: ProviderConnection): 'ok' | 'warn' | 'err' | 'neutral' {
  if (!conn.isActive) return 'neutral'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) return 'ok'
  if (['failed', 'error', 'invalid'].includes(s)) return 'err'
  return 'neutral'
}

/**
 * 状态徽标的悬浮说明：最近测试时间 + 失败原因，未测试时给出操作引导。
 * @param conn 提供商连接
 */
function statusTip(conn: ProviderConnection): string {
  if (!conn.isActive) return '该连接已禁用，不参与路由'
  const bits: string[] = []
  bits.push(conn.lastTestedAt ? `最近测试：${fmtDateTime(conn.lastTestedAt)}` : '尚未测试，点击「测试」检查连通性')
  if (typeof conn.lastLatencyMs === 'number') bits.push(`响应耗时：${fmtLatency(conn.lastLatencyMs)}`)
  if (conn.lastError) bits.push(`错误：${conn.lastError}`)
  if (!conn.hasApiKey && !conn.hasAccessToken) bits.push('提示：该连接未配置密钥')
  return bits.join('\n')
}

/**
 * 把测试结果就地写回列表项，避免刷新整页。
 * @param conn 被测试的连接
 * @param r 测试结果
 */
function applyResult(conn: ProviderConnection, r: { status: string; error?: string; latencyMs?: number }) {
  conn.testStatus = r.status
  conn.lastError = r.error || undefined
  conn.lastTestedAt = new Date().toISOString()
  // 耗时随之持久化到数据库（last_latency_ms），刷新后列表仍能显示
  conn.lastLatencyMs = typeof r.latencyMs === 'number' ? r.latencyMs : undefined
}

/**
 * 测试单个连接的健康状态（行内按钮）。
 * @param conn 待测试的连接
 */
async function testOne(conn: ProviderConnection) {
  if (isTesting(conn.id)) return
  testingMap.value[conn.id] = true
  try {
    const r = await testProvider(conn.id)
    applyResult(conn, r)
    if (r.status === 'ok') {
      ElMessage.success(`${conn.name}：可用${r.latencyMs ? ` · ${fmtLatency(r.latencyMs)}` : ''}`)
    } else {
      ElMessage.error(`${conn.name}：${r.error || '连接失败'}`)
    }
  } catch (e: any) {
    const msg = e?.response?.data?.error || e?.message || '请求未到达网关'
    applyResult(conn, { status: 'error', error: msg })
    ElMessage.error(`${conn.name}：测试请求失败`)
  } finally {
    delete testingMap.value[conn.id]
  }
}

/**
 * 批量测试当前列表中的所有连接。
 *
 * 串行执行而非并发：后端每个测试自带 15 秒超时，并发打爆上游容易被限流，
 * 串行也能让进度条读数准确。
 */
async function testAll() {
  const list = filtered.value
  if (!list.length) return
  testingAll.value = true
  progress.value = { done: 0, total: list.length }
  let ok = 0
  let fail = 0
  try {
    for (const conn of list) {
      try {
        const r = await testProvider(conn.id)
        applyResult(conn, r)
        if (r.status === 'ok') ok++
        else fail++
      } catch {
        applyResult(conn, { status: 'error', error: '测试请求失败' })
        fail++
      }
      progress.value.done++
    }
    if (fail === 0) ElMessage.success(`全部 ${ok} 个连接均可用`)
    else ElMessage.warning(`测试完成：${ok} 个可用，${fail} 个失败（悬浮状态徽标可看原因）`)
  } finally {
    testingAll.value = false
  }
}

/**
 * 格式化 ISO 时间为 M/D HH:mm。
 * @param iso ISO 时间字符串
 */
function fmtTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

/**
 * 格式化 ISO 时间为 M/D HH:mm:ss（用于 tooltip 里的精确时间）。
 * @param iso ISO 时间字符串
 */
function fmtDateTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
}

/** 列表中最多展示的模型标签数；超出折叠为 +N */
const maxTags = 3

/**
 * 计算模型标签显示文本：优先用自定义名称，回退到模型 ID。
 * @param m 模型对象
 */
function modelTagLabel(m: { id: string; name?: string }): string {
  const trimmed = m.name?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : m.id
}

/**
 * 计算模型标签的 tooltip 文本：形如「id（名称）」。
 * @param m 模型对象
 */
function modelTagTitle(m: { id: string; name?: string }): string {
  const trimmed = m.name?.trim()
  return trimmed && trimmed.length > 0 ? `${m.id}（${trimmed}）` : m.id
}

/**
 * 取前 maxTags 个模型用于列表展示。
 * @param conn 提供商连接
 */
function visibleModels(conn: ProviderConnection) {
  return (conn.models ?? []).slice(0, maxTags)
}

/**
 * 将超出 maxTags 的模型拼接为 tooltip 内容。
 * @param conn 提供商连接
 */
function overflowModels(conn: ProviderConnection): string {
  const rest = (conn.models ?? []).slice(maxTags)
  return rest.map((m) => modelTagTitle(m)).join('\n')
}

/**
 * 加载内置提供商定义与连接列表。
 */
async function load() {
  loading.value = true
  try {
    const data = await listProviders()
    providers.value = data.providers ?? []
    connections.value = data.connections ?? []
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.list-bar .list-search { max-width: 240px; }
.list-count { font-variant-numeric: tabular-nums; }

.testing { color: var(--ink-3); }
.err-icon { color: var(--err); cursor: help; }

/* 状态列：胶囊 + 圆点。健康态用绿色展示响应耗时，不再出现「健康」二字 */
.status-cell { white-space: nowrap; }
.lat-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 100%;
  padding: 1.5px 7px;
  border: 1px solid transparent;
  border-radius: 999px;
  font-size: 11.5px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  background: var(--surface-2);
  color: var(--ink-3);
  cursor: default;
}
.lat-dot {
  width: 5px;
  height: 5px;
  flex-shrink: 0;
  border-radius: 50%;
  background: currentColor;
}
.lat-text { overflow: hidden; text-overflow: ellipsis; }
.lat-pill.ok { background: var(--ok-bg); color: var(--ok); }
.lat-pill.err { background: var(--err-bg); color: var(--err); }
.lat-pill.warn { background: var(--warn-bg); color: var(--warn); }
.lat-pill.neutral { background: var(--surface-2); color: var(--ink-4); }

/* 提供商列：限制最大宽度，防止挤压模型列 */
.prov-cell { max-width: 100%; }
.prov-meta { max-width: 180px; }

/* 模型标签：紧凑、统一，默认模型用小点标识而非绿色背景 */
.model-tags { row-gap: 3px; }
.model-tag {
  line-height: 1.55;
  border-color: var(--line);
}
.def-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--accent);
  flex-shrink: 0;
}
.model-tag.more {
  color: var(--ink-3);
  cursor: default;
}
.model-cell.empty { color: var(--ink-4); }

/* 行内操作：按钮水平排列，避免堆叠撑高行 */
.action-cell { padding-left: 8px; padding-right: 14px; }
.action-btns {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
}
.action-btns .btn.sm { padding: 4px 8px; font-size: 11.5px; }
</style>
