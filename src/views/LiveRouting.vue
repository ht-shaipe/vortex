<template>
  <div class="live-root">
    <!-- 顶部标题栏与网关运行状态徽标 -->
    <div class="page-bar">
      <span class="page-head">实时路由</span>
      <StatusBadge :tone="running ? 'ok' : 'err'" :label="running ? '网关运行中' : '网关未运行'" />
    </div>

    <el-scrollbar class="live-scroll">
      <div class="page-col live-col">
        <!-- 拓扑简图：对外协议 → Vortex → 上游提供商 -->
        <div class="card topo-card">
          <div class="topo">
            <!-- 左列：对外协议 -->
            <div class="topo-col">
              <div class="topo-title">对外协议</div>
              <div class="rf-proto" v-for="p in protocols" :key="p.name">
                <span class="rf-proto-icon" :class="p.cls">{{ p.tag }}</span>
                <span class="rf-proto-name">{{ p.name }}</span>
              </div>
            </div>
            <!-- 中列：Vortex 网关中心节点 -->
            <div class="topo-mid">
              <div class="rf-flow-line" />
              <div class="rf-hub">Vortex</div>
              <div class="rf-flow-line" />
            </div>
            <!-- 右列：上游提供商列表 -->
            <div class="topo-col">
              <div class="topo-title">上游提供商</div>
              <template v-if="upstreams.length > 0">
                <div class="rf-up" v-for="p in upstreams" :key="p.id">
                  <ProviderLogo :name="p.provider" :hint="`${p.name} ${p.baseUrl || ''}`" :size="16" />
                  <span class="rf-up-name">{{ p.name }}</span>
                  <span class="rf-up-badge">{{ p.model || '默认' }}</span>
                </div>
              </template>
              <div v-else class="topo-empty">暂无活跃连接，请先在订阅页添加</div>
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
            <div class="access-row" v-for="e in endpoints" :key="e.label">
              <span class="access-label">{{ e.label }}</span>
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
          <div class="card-body api-list">
            <div v-for="a in apis" :key="a.method + a.path" class="api-row">
              <span class="api-method mono" :class="a.group">{{ a.method }}</span>
              <span class="api-path mono">{{ a.path }}</span>
              <span class="api-desc">{{ a.desc }}</span>
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
.live-root { height: 100vh; display: flex; flex-direction: column; }
.live-scroll { flex: 1; }
.live-col { padding: 24px; }

.topo-card { padding: var(--pad-card); margin-bottom: var(--gap-lg); }
.topo { display: flex; align-items: center; justify-content: center; gap: 24px; }
.topo-col { display: flex; flex-direction: column; gap: 8px; min-width: 160px; }
.topo-title { font-size: 11px; color: var(--ink-4); text-transform: uppercase; letter-spacing: 0.04em; margin-bottom: 4px; }
.topo-mid { display: flex; flex-direction: column; align-items: center; gap: 4px; }
.rf-hub {
  width: 64px; height: 64px;
  border-radius: 16px;
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 13px;
  background: var(--accent-bg);
  color: var(--accent-ink);
  border: 1px solid var(--accent-line);
}
.rf-flow-line { width: 2px; height: 24px; background: var(--line-2); border-radius: 1px; }

.rf-proto {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 10px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--surface-2);
  font-size: 13px;
}
.rf-proto-icon {
  display: grid; place-items: center;
  width: 28px; height: 20px;
  border-radius: 4px;
  font-size: 10px; font-weight: 700;
  flex-shrink: 0;
}
.rf-proto-icon.oai { background: oklch(0.90 0.05 150); color: oklch(0.40 0.12 150); }
.rf-proto-icon.ant { background: oklch(0.88 0.08 280); color: oklch(0.40 0.15 280); }
.rf-proto-name { color: var(--ink-2); }

.rf-up { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: var(--r-sm); background: var(--surface-2); border: 1px solid var(--line); }
.rf-up-name { font-size: 12px; color: var(--ink-2); }
.rf-up-badge {
  margin-left: auto;
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--ink-4);
  background: var(--surface-3);
  padding: 1px 5px;
  border-radius: 3px;
}
.topo-empty { font-size: 12px; color: var(--ink-4); padding: 8px; }

.section { margin-bottom: var(--gap-lg); }
.access-row { display: flex; align-items: center; gap: 12px; padding: 4px 0; }
.access-label { width: 90px; flex-shrink: 0; font-size: 12px; font-weight: 500; color: var(--ink-3); }

.api-list { display: flex; flex-direction: column; }
.api-row {
  display: grid;
  grid-template-columns: 56px 1fr auto;
  gap: 12px;
  align-items: center;
  padding: 9px 0;
  border-bottom: 1px solid var(--line);
}
.api-row:last-child { border-bottom: none; }
.api-method { font-size: 11px; font-weight: 700; }
.api-method.oai { color: oklch(0.45 0.12 150); }
.api-method.ant { color: oklch(0.50 0.15 280); }
.api-method.mgmt { color: var(--ink-4); }
.api-path { font-size: 12.5px; color: var(--ink-2); }
.api-desc { font-size: 12px; color: var(--ink-3); }
</style>
