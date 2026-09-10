<template>
  <div>
    <!-- 页面头部：标题与新增按钮 -->
    <PageHeader title="订阅管理" sub="管理各 AI 提供商的连接与 API 密钥">
      <template #actions>
        <router-link to="/subscriptions/new" class="btn">
          <el-icon :size="14"><Plus /></el-icon>添加提供方
        </router-link>
        <router-link to="/subscriptions/custom" class="btn primary">
          <el-icon :size="14"><Connection /></el-icon>添加自定义提供方
        </router-link>
      </template>
    </PageHeader>

    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <!-- 空态：引导用户添加第一个连接 -->
    <div v-else-if="connections.length === 0" class="card">
      <EmptyState title="还没有订阅" desc="从内置提供商列表中添加一个连接，或填写自定义端点接入 OpenAI 兼容服务">
        <div class="empty-actions">
          <router-link to="/subscriptions/new" class="btn">添加提供方</router-link>
          <router-link to="/subscriptions/custom" class="btn primary">添加自定义提供方</router-link>
        </div>
      </EmptyState>
    </div>

    <!-- 订阅列表表格 -->
    <div v-else class="card">
      <table class="table">
        <colgroup>
          <col style="width: 110px" />
          <col />
          <col style="width: 240px" />
          <col style="width: 72px" />
          <col style="width: 110px" />
          <col style="width: 88px" />
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
          <tr v-for="conn in connections" :key="conn.id" @click="$router.push(`/subscriptions/${conn.id}`)">
            <!-- 状态徽标 -->
            <td>
              <StatusBadge :tone="statusTone(conn)" :label="statusLabel(conn)" />
            </td>
            <!-- 提供商图标与名称 -->
            <td>
              <div class="prov-cell">
                <ProviderLogo :name="conn.provider" :hint="`${conn.name} ${conn.baseUrl || ''}`" :color="provColor(conn.provider)" :size="24" />
                <div class="prov-meta">
                  <div class="prov-name" :title="conn.name">{{ conn.name }}</div>
                  <div class="prov-id mono" :title="conn.provider">{{ conn.provider }}</div>
                </div>
              </div>
            </td>
            <!-- 模型标签：超出 maxTags 折叠为 +N -->
            <td>
              <div v-if="conn.models && conn.models.length" class="model-tags">
                <span
                  v-for="(m, i) in visibleModels(conn)"
                  :key="m.id"
                  class="model-tag"
                  :class="{ primary: i === 0 }"
                  :title="modelTagTitle(m)"
                >{{ modelTagLabel(m) }}</span>
                <el-tooltip
                  v-if="conn.models.length > maxTags"
                  :content="overflowModels(conn)"
                  placement="top"
                >
                  <span class="model-tag more">+{{ conn.models.length - maxTags }}</span>
                </el-tooltip>
              </div>
              <!-- 无模型列表时回退展示默认模型 -->
              <div
                v-else
                class="model-cell"
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
            <td class="num">{{ fmtTime(conn.createdAt) }}</td>
            <!-- 操作按钮 -->
            <td class="action-cell">
              <button type="button" class="btn sm" @click.stop="$router.push(`/subscriptions/${conn.id}`)">查看</button>
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
 * 职责：展示所有 AI 提供商连接的列表，包括状态、提供商、模型、优先级与更新时间，
 * 并提供新增内置/自定义提供方的入口。点击行可跳转到连接详情。
 */
import { onMounted, ref } from 'vue'
import { Loading, Plus, Connection } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listProviders, type ProviderDef, type ProviderConnection } from '@/api/providers'

// 当前连接列表
const connections = ref<ProviderConnection[]>([])
// 内置提供商定义列表
const providers = ref<ProviderDef[]>([])
// 是否正在加载
const loading = ref(true)

/**
 * 根据提供商 ID 查询其主题色。
 * @param id 提供商标识
 * @returns 颜色字符串，未找到则返回空串
 */
function provColor(id: string): string {
  return providers.value.find((p) => p.id === id)?.color ?? ''
}

/**
 * 根据连接状态计算中文标签。
 * @param conn 提供商连接
 */
function statusLabel(conn: ProviderConnection): string {
  if (!conn.isActive) return '已禁用'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) return '健康'
  if (['failed', 'error', 'invalid'].includes(s)) return '失败'
  return '未测试'
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
 * 格式化 ISO 时间为 M/D HH:mm。
 * @param iso ISO 时间字符串
 */
function fmtTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
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
 * 组件挂载时加载内置提供商定义与连接列表。
 */
onMounted(async () => {
  try {
    const data = await listProviders()
    providers.value = data.providers ?? []
    connections.value = data.connections ?? []
  } finally {
    loading.value = false
  }
})
</script>

<style scoped>
.spin-wrap { padding: 40px; text-align: center; color: var(--ink-4); }
.prov-cell { display: flex; align-items: center; gap: 10px; }
.prov-meta { min-width: 0; }
.prov-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}
.prov-id {
  font-size: 11px;
  color: var(--ink-4);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.model-cell {
  font-size: 12.5px;
  color: var(--ink-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}
.model-cell.empty { color: var(--ink-4); }
.model-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
}
.model-tag {
  display: inline-block;
  font-size: 11.5px;
  padding: 2px 8px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: 10px;
  color: var(--ink-2);
  white-space: nowrap;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.model-tag.primary {
  color: var(--ok);
  border-color: rgba(0, 0, 0, 0.05);
  background: var(--ok-bg);
}
.model-tag.more {
  color: var(--ink-3);
  cursor: default;
}
.empty-actions { display: flex; gap: 8px; }
</style>
