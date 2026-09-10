<template>
  <div class="live-root">
    <div class="page-bar">
      <span class="page-head">实时路由</span>
      <StatusBadge :tone="running ? 'ok' : 'err'" :label="running ? '网关运行中' : '网关未运行'" />
    </div>

    <el-scrollbar class="live-scroll">
      <div class="page-col live-col">
        <!-- 拓扑简图 -->
        <div class="card topo-card">
          <div class="topo">
            <div class="topo-col">
              <div class="topo-title">客户端</div>
              <div class="rf-client" v-for="c in clients" :key="c">
                <span class="rf-win-dots"><i /><i /><i /></span>
                <span class="rf-name mono">{{ c }}</span>
              </div>
            </div>
            <div class="topo-mid">
              <div class="rf-flow-line" />
              <div class="rf-hub">Vortex</div>
              <div class="rf-flow-line" />
            </div>
            <div class="topo-col">
              <div class="topo-title">上游提供商</div>
              <div class="rf-up" v-for="p in upstreams" :key="p">
                <ProviderLogo :name="p" :size="16" />
                <span class="rf-up-name mono">{{ p }}</span>
              </div>
              <div v-if="upstreams.length === 0" class="topo-empty">暂无提供商连接</div>
            </div>
          </div>
        </div>

        <!-- 接入信息 -->
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">接入信息</div>
              <div class="card-sub">将客户端 base_url 指向 Vortex</div>
            </div>
          </div>
          <div class="card-body">
            <div class="access-row">
              <span class="access-label">Base URL</span>
              <CopyableBlock :text="baseUrl" variant="inline">{{ baseUrl }}</CopyableBlock>
            </div>
          </div>
        </div>

        <!-- API 端点 -->
        <div class="card section">
          <div class="card-head">
            <div>
              <div class="card-title">API 端点</div>
              <div class="card-sub">OpenAI 兼容 + 管理接口</div>
            </div>
          </div>
          <div class="card-body api-list">
            <div v-for="a in apis" :key="a.method + a.path" class="api-row">
              <span class="api-method mono">{{ a.method }}</span>
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
import { onMounted, ref } from 'vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { listProviders } from '@/api/providers'

const baseUrl = 'http://localhost:20128/v1'
const clients = ['OpenAI SDK', 'Claude Code', 'curl', '自定义客户端']
const running = ref(false)
const upstreams = ref<string[]>([])

const apis = [
  { method: 'POST', path: '/v1/chat/completions', desc: '聊天补全（流式）' },
  { method: 'GET', path: '/v1/models', desc: '模型列表' },
  { method: 'POST', path: '/v1/embeddings', desc: '文本嵌入' },
  { method: 'POST', path: '/v1/images/generations', desc: '图像生成' },
  { method: 'GET', path: '/api/health', desc: '健康检查' },
]

onMounted(async () => {
  try {
    const data = await listProviders()
    upstreams.value = (data.providers ?? []).map((p) => p.id)
  } catch {
    /* ignore */
  }
  try {
    const res = await fetch('http://localhost:20128/api/health')
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
.topo-col { display: flex; flex-direction: column; gap: 8px; min-width: 150px; }
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
.rf-client {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 10px;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  background: var(--surface-2);
  font-size: 12px;
}
.rf-win-dots { display: flex; gap: 3px; }
.rf-win-dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--ink-5); }
.rf-up { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: var(--r-sm); background: var(--surface-2); border: 1px solid var(--line); }
.rf-up-name { font-size: 12px; }
.topo-empty { font-size: 12px; color: var(--ink-4); padding: 8px; }

.section { margin-bottom: var(--gap-lg); }
.access-row { display: flex; align-items: center; gap: 12px; }
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
.api-method { font-size: 11px; font-weight: 700; color: var(--accent-ink); }
.api-path { font-size: 12.5px; color: var(--ink-2); }
.api-desc { font-size: 12px; color: var(--ink-3); }
</style>