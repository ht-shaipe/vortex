<template>
  <div class="live-root flex-1 min-h-0 flex flex-col" data-tauri-drag-region>
    <PageHeader title="实时路由" sub="网关拓扑、接入地址与 API 端点一览" class="px-28px py-8px" />

    <el-scrollbar class="live-scroll flex-1">
      <div class="page-col live-col p-24px">
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
              <div class="rf-flow-node">Vortex</div>
              <div class="rf-flow-line" />
            </div>
            <!-- 右列：上游提供商列表 -->
            <div class="topo-col">
              <div class="topo-title">上游提供商</div>
              <template v-if="upstreams.length > 0">
                <div class="rf-up" v-for="p in upstreams" :key="p.id">
                  <ProviderLogo :name="p.provider" :hint="`${p.name} ${p.baseUrl || ''}`" :size="18" />
                  <span class="rf-up-name" :title="p.name">{{ p.name }}</span>
                  <span class="rf-up-badge" :title="p.model || '默认模型'">{{ p.model || '默认' }}</span>
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
import PageHeader from '@/components/ui/PageHeader.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { listProviders } from '@/api/providers'

// 网关本地基础地址
const baseHost = 'http://localhost:10168'

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
})
</script>

<style scoped>
.topo-card { padding: var(--pad-card); margin-bottom: var(--gap-lg); }

.topo {
  display: flex;
  align-items: flex-start;
  justify-content: center;
  gap: 44px;
  padding: 18px 0 14px;
}

.topo-col {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 190px;
  width: 190px;
  padding-top: 4px;
}
.topo-title {
  font-size: var(--fs-xs);
  color: var(--ink-4);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding-left: 2px;
  line-height: 1;
}

.rf-proto {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border-radius: var(--r-md);
  border: 1px solid var(--line);
  background: var(--surface-2);
  font-size: var(--fs-body);
  transition: background 0.15s ease, border-color 0.15s ease, transform 0.12s ease;
}
.rf-proto:hover {
  background: var(--surface-3);
  border-color: var(--line-2);
  transform: translateY(-1px);
}
.rf-proto-icon {
  display: grid;
  place-items: center;
  width: 32px;
  height: 22px;
  border-radius: 5px;
  font-size: 10.5px;
  font-weight: 800;
  flex-shrink: 0;
  letter-spacing: 0.02em;
}
.rf-proto-icon.oai { background: oklch(0.90 0.05 150); color: oklch(0.36 0.12 150); }
.rf-proto-icon.ant { background: oklch(0.88 0.08 280); color: oklch(0.38 0.15 280); }
.rf-proto-name { color: var(--ink-2); font-weight: 500; }

.topo-mid {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.rf-flow-line {
  width: 2px;
  height: 42px;
  background: linear-gradient(180deg, var(--line-2), var(--line));
  border-radius: 1px;
  position: relative;
}
.rf-flow-line::after {
  content: '';
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--line-2);
}
.rf-flow-node {
  width: 84px;
  height: 84px;
  border-radius: 22px;
  display: grid;
  place-items: center;
  font-size: 15px;
  font-weight: 700;
  background: var(--accent-bg);
  color: var(--accent-ink);
  border: 1px solid var(--accent-line);
  box-shadow: var(--shadow-sm);
  letter-spacing: 0.01em;
}

.rf-up {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 8px 11px;
  border-radius: var(--r-md);
  background: var(--surface-2);
  border: 1px solid var(--line);
  transition: background 0.15s ease, border-color 0.15s ease, transform 0.12s ease;
}
.rf-up:hover {
  background: var(--surface-3);
  border-color: var(--line-2);
  transform: translateY(-1px);
}
.rf-up-name {
  font-size: var(--fs-body);
  color: var(--ink-2);
  font-weight: 500;
  flex: 1 1 auto;
  min-width: 0;
  max-width: 100px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.rf-up-badge {
  margin-left: auto;
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--ink-4);
  background: var(--surface-3);
  padding: 2px 6px;
  border-radius: 4px;
  flex-shrink: 0;
  min-width: 34px;
  max-width: 72px;
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.topo-empty {
  font-size: var(--fs-sm);
  color: var(--ink-4);
  padding: 10px;
  text-align: center;
  border: 1px dashed var(--line);
  border-radius: var(--r-md);
}

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
