<template>
  <div class="live-root flex-1 min-h-0 flex flex-col" data-tauri-drag-region>
    <!-- 顶部标题栏与网关运行状态徽标 -->
    <div class="page-bar" data-tauri-drag-region >
      <span class="page-head">实时路由</span>
      <StatusBadge :tone="running ? 'ok' : 'err'" :label="running ? '网关运行中' : '网关未运行'" />
    </div>

    <el-scrollbar class="live-scroll flex-1">
      <div class="page-col live-col p-24px">
        <!-- 拓扑简图：对外协议 → Vortex → 上游提供商 -->
        <div class="card topo-card">
          <div class="topo flex items-center justify-center gap-24px">
            <!-- 左列：对外协议 -->
            <div class="topo-col flex flex-col gap-8px min-w-160px">
              <div class="topo-title text-11px text-ink-4 uppercase tracking-0.04em mb-4px">对外协议</div>
              <div class="rf-proto flex items-center gap-8px py-8px px-10px rounded-sm border border-line bg-surface-2 text-13px" v-for="p in protocols" :key="p.name">
                <span class="rf-proto-icon grid place-items-center w-28px h-20px rounded-4px text-10px font-bold shrink-0" :class="p.cls">{{ p.tag }}</span>
                <span class="rf-proto-name text-ink-2">{{ p.name }}</span>
              </div>
            </div>
            <!-- 中列：Vortex 网关中心节点 -->
            <div class="topo-mid flex flex-col items-center gap-4px">
              <div class="rf-flow-line w-2px h-24px bg-line-2 rounded-1px" />
              <div class="rf-hub w-64px h-64px rounded-16px grid place-items-center font-bold text-13px bg-accent-bg text-accent-ink border border-accent-line">Vortex</div>
              <div class="rf-flow-line w-2px h-24px bg-line-2 rounded-1px" />
            </div>
            <!-- 右列：上游提供商列表 -->
            <div class="topo-col flex flex-col gap-8px min-w-160px">
              <div class="topo-title text-11px text-ink-4 uppercase tracking-0.04em mb-4px">上游提供商</div>
              <template v-if="upstreams.length > 0">
                <div class="rf-up flex items-center gap-8px py-6px px-8px rounded-sm bg-surface-2 border border-line" v-for="p in upstreams" :key="p.id">
                  <ProviderLogo :name="p.provider" :hint="`${p.name} ${p.baseUrl || ''}`" :size="16" />
                  <span class="rf-up-name text-12px text-ink-2">{{ p.name }}</span>
                  <span class="rf-up-badge ml-auto text-10px font-mono text-ink-4 bg-surface-3 py-1px px-5px rounded-3px">{{ p.model || '默认' }}</span>
                </div>
              </template>
              <div v-else class="topo-empty text-12px text-ink-4 p-8px">暂无活跃连接，请先在订阅页添加</div>
            </div>
          </div>
        </div>

        <!-- 接入信息：展示各协议的 base_url -->
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">接入信息</div>
              <div class="card-sub">将客户端 base_url 指向 Vortex</div>
            </div>
          </div>
          <div class="card-body">
            <div class="access-row flex items-center gap-12px py-4px" v-for="e in endpoints" :key="e.label">
              <span class="access-label w-90px shrink-0 text-12px font-medium text-ink-3">{{ e.label }}</span>
              <CopyableBlock :text="e.url" variant="inline">{{ e.url }}</CopyableBlock>
            </div>
          </div>
        </div>

        <!-- API 端点：列出网关对外暴露的所有接口 -->
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">API 端点</div>
              <div class="card-sub">OpenAI 兼容 + Anthropic 兼容 + 管理接口</div>
            </div>
          </div>
          <div class="card-body api-list flex flex-col">
            <div v-for="a in apis" :key="a.method + a.path" class="api-row">
              <span class="api-method mono text-11px font-bold" :class="a.group">{{ a.method }}</span>
              <span class="api-path mono text-12.5px text-ink-2">{{ a.path }}</span>
              <span class="api-desc text-12px text-ink-3">{{ a.desc }}</span>
            </div>
          </div>
        </div>
      </div>
    </el-scrollbar>
  </div>
</template>

<script setup lang="ts">
/**
 * 实时路由页面。
 * 职责：展示网关当前的运行状态、对外协议与上游提供商的拓扑简图、
 * 客户端接入地址以及网关对外暴露的 API 端点清单。
 */
import { onMounted, ref } from 'vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { listProviders } from '@/api/providers'

// 网关本地基础地址
const baseHost = 'http://localhost:10168'
// 网关是否正在运行
const running = ref(false)

// 对外协议定义
const protocols = [
  { name: 'OpenAI 协议', tag: 'OAI', cls: 'oai' },
  { name: 'Anthropic 协议', tag: 'ANT', cls: 'ant' },
]

// 客户端接入端点
const endpoints = [
  { label: 'OpenAI', url: `${baseHost}/v1` },
  { label: 'Anthropic', url: `${baseHost}/anthropic/v1` },
]

/** 上游提供商连接的简化结构 */
interface Upstream {
  id: string
  provider: string
  name: string
  model?: string
  baseUrl?: string
}

// 当前活跃的上游连接列表
const upstreams = ref<Upstream[]>([])

// 网关对外暴露的 API 端点清单
const apis = [
  { method: 'POST', path: '/v1/chat/completions', desc: 'OpenAI 聊天补全（流式）', group: 'oai' },
  { method: 'GET', path: '/v1/models', desc: 'OpenAI 模型列表', group: 'oai' },
  { method: 'POST', path: '/v1/embeddings', desc: 'OpenAI 文本嵌入', group: 'oai' },
  { method: 'POST', path: '/v1/images/generations', desc: 'OpenAI 图像生成', group: 'oai' },
  { method: 'POST', path: '/anthropic/v1/messages', desc: 'Anthropic Messages（流式）', group: 'ant' },
  { method: 'GET', path: '/api/health', desc: '健康检查', group: 'mgmt' },
]

/**
 * 组件挂载时加载上游提供商连接并探测网关健康状态。
 */
onMounted(async () => {
  // 拉取活跃的上游连接
  try {
    const data = await listProviders()
    upstreams.value = (data.connections ?? [])
      .filter((c) => c.isActive)
      .map((c) => ({
        id: c.id,
        provider: c.provider,
        name: c.name,
        model: c.defaultModel,
        baseUrl: c.baseUrl,
      }))
  } catch {
    /* ignore */
  }
  // 探测网关健康状态
  try {
    const res = await fetch(`${baseHost}/api/health`)
    const d = await res.json()
    running.value = d.status === 'ok'
  } catch {
    running.value = false
  }
})
</script>

<style scoped>
.topo-card { padding: var(--pad-card); margin-bottom: var(--gap-lg); }

.rf-proto-icon.oai { background: oklch(0.90 0.05 150); color: oklch(0.40 0.12 150); }
.rf-proto-icon.ant { background: oklch(0.88 0.08 280); color: oklch(0.40 0.15 280); }

.section { margin-bottom: var(--gap-lg); }

.api-row {
  display: grid;
  grid-template-columns: 56px 1fr auto;
  gap: 12px;
  align-items: center;
  padding: 9px 0;
  border-bottom: 1px solid var(--line);
}
.api-row:last-child { border-bottom: none; }
.api-method.oai { color: oklch(0.45 0.12 150); }
.api-method.ant { color: oklch(0.50 0.15 280); }
.api-method.mgmt { color: var(--ink-4); }
</style>
