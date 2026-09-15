<template>
  <div class="live-root flex-1 min-h-0 flex flex-col" data-tauri-drag-region>
    <PageHeader title="实时路由" sub="网关拓扑、接入地址与 API 端点一览" class="px-28px py-8px" />

    <el-scrollbar class="live-scroll flex-1">
      <div class="page-col live-col p-24px">
        <!-- 拓扑简图：对外协议 → Vortex → 上游提供商 -->
        <div class="card p-[var(--pad-card)] mb-[var(--gap-lg)]">
          <div ref="topoEl" class="topology relative flex items-center justify-center gap-44px py-14px">
            <!-- 连接线 SVG 覆盖层（曲线） -->
            <svg
              v-show="links.length > 0"
              class="topo-links absolute inset-0 pointer-events-none"
              :width="topoSize.w"
              :height="topoSize.h"
              :viewBox="`0 0 ${topoSize.w} ${topoSize.h}`"
              preserveAspectRatio="none"
              aria-hidden="true"
            >
              <path
                v-for="l in links"
                :key="l.key"
                :d="l.d"
                fill="none"
                stroke="var(--line-2)"
                stroke-width="1.5"
                stroke-linecap="round"
              />
            </svg>

            <!-- 左列：对外协议 -->
            <div class="flex flex-col gap-8px min-w-190px w-190px pt-4px items-center">
              <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">对外协议</div>
              <div class="relative flex flex-col gap-8px w-full">
                <div
                  ref="protocolEls"
                  class="relative z-1 flex items-center gap-10px py-9px px-12px rounded-md border border-solid border-line bg-surface-2 text-body transition-[background-color,border-color,transform] duration-150 hover:bg-surface-3 hover:border-line-2 hover:-translate-y-1px"
                  v-for="p in protocols"
                  :key="p.name"
                >
                  <span class="grid place-items-center w-32px h-22px rounded-5px text-10.5px font-extrabold shrink-0 tracking-[0.02em]" :class="[p.cls, p.cls === 'oai' ? 'bg-[oklch(0.90_0.05_150)] text-[oklch(0.36_0.12_150)]' : 'bg-[oklch(0.88_0.08_280)] text-[oklch(0.38_0.15_280)]']">{{ p.tag }}</span>
                  <span class="text-ink-2 font-medium">{{ p.name }}</span>
                </div>
              </div>
            </div>

            <!-- 中列：Vortex 网关中心节点 -->
            <div ref="vortexEl" class="relative z-1 flex flex-col items-center gap-8px">
              <div class="w-2px h-42px bg-gradient-to-b from-line-2 to-line rounded-1px relative after:content-[''] after:absolute after:left-1/2 after:top-1/2 after:-translate-x-1/2 after:-translate-y-1/2 after:w-5px after:h-5px after:rounded-full after:bg-line-2" />
              <div class="w-84px h-84px rounded-22px grid place-items-center text-15px font-bold bg-accent-bg text-accent-ink border border-solid border-accent-line shadow-sm tracking-[0.01em]">Vortex</div>
              <div class="w-2px h-42px bg-gradient-to-b from-line-2 to-line rounded-1px relative after:content-[''] after:absolute after:left-1/2 after:top-1/2 after:-translate-x-1/2 after:-translate-y-1/2 after:w-5px after:h-5px after:rounded-full after:bg-line-2" />
            </div>

            <!-- 右列：上游提供商列表 -->
            <div class="flex flex-col gap-8px min-w-200px w-210px pt-4px items-center">
              <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">上游提供商</div>
              <div class="relative flex flex-col gap-8px w-full">
                <template v-if="upstreams.length > 0">
                  <div
                    ref="upstreamEls"
                    class="relative z-1 flex items-center gap-10px py-9px px-12px rounded-md bg-surface-2 border border-solid border-line transition-[background-color,border-color,transform] duration-150 hover:bg-surface-3 hover:border-line-2 hover:-translate-y-1px"
                    v-for="p in upstreams"
                    :key="p.id"
                  >
                    <ProviderLogo :name="p.provider" :hint="`${p.name} ${p.baseUrl || ''}`" :size="18" class="shrink-0" />
                    <span
                      class="text-body text-ink-2 font-medium flex-1 min-w-0 truncate"
                      :title="p.name"
                    >{{ p.name }}</span>
                    <span
                      class="shrink-0 ml-2px max-w-96px text-right text-10.5px font-mono leading-tight text-ink-3 bg-surface-3 border border-solid border-line rounded-4px py-2px px-6px whitespace-nowrap overflow-hidden text-ellipsis"
                      :title="p.model || '默认模型'"
                    >{{ p.model || '默认' }}</span>
                  </div>
                </template>
                <div v-else class="relative z-1 text-sm text-ink-4 p-10px text-center border border-dashed border-line rounded-md">暂无活跃连接，请先在订阅页添加</div>
              </div>
            </div>
          </div>
        </div>

        <!-- 接入信息：展示各协议的 base_url -->
        <div class="card mb-[var(--gap-lg)]">
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
        <div class="card mb-[var(--gap-lg)]">
          <div class="card-head">
            <div>
              <div class="card-title">API 端点</div>
              <div class="card-sub">OpenAI 兼容 + Anthropic 兼容 + 管理接口</div>
            </div>
          </div>
          <div class="card-body api-list flex flex-col">
            <div v-for="a in apis" :key="a.method + a.path" class="grid grid-cols-[56px_1fr_auto] gap-12px items-center py-9px border-b border-line border-solid border-0 last:border-b-0">
              <span class="api-method mono text-11px font-bold [&.oai]:text-[oklch(0.45_0.12_150)] [&.ant]:text-[oklch(0.50_0.15_280)] [&.mgmt]:text-ink-4" :class="a.group">{{ a.method }}</span>
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
import { onBeforeUnmount, onMounted, nextTick, ref, watch } from 'vue'
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

// ===== 拓扑连接线（SVG 贝塞尔曲线）=====
const topoEl = ref<HTMLElement | null>(null)
const vortexEl = ref<HTMLElement | null>(null)
const protocolEls = ref<HTMLElement[]>([])
const upstreamEls = ref<HTMLElement[]>([])
const links = ref<{ key: string; d: string }[]>([])
const topoSize = ref({ w: 0, h: 0 })

/** 计算元素边缘中心相对容器的坐标。 */
function edgeCenter(el: HTMLElement, container: DOMRect, side: 'left' | 'right') {
  const r = el.getBoundingClientRect()
  return {
    x: side === 'right' ? r.right - container.left : r.left - container.left,
    y: r.top - container.top + r.height / 2,
  }
}

/** 重算所有连接曲线的 path。 */
function drawLinks(): void {
  const container = topoEl.value
  const vortex = vortexEl.value
  if (!container || !vortex) return
  const cr = container.getBoundingClientRect()
  if (cr.width === 0 || cr.height === 0) return
  topoSize.value = { w: Math.round(cr.width), h: Math.round(cr.height) }

  const vLeft = edgeCenter(vortex, cr, 'left')
  const vRight = edgeCenter(vortex, cr, 'right')

  const out: { key: string; d: string }[] = []
  const push = (el: HTMLElement, target: { x: number; y: number }, side: 'left' | 'right', key: string) => {
    const s = edgeCenter(el, cr, side)
    const dx = (target.x - s.x) * 0.5
    // 三次贝塞尔：水平进出、中间平滑过渡的 S 形曲线
    out.push({ key, d: `M ${s.x} ${s.y} C ${s.x + dx} ${s.y}, ${target.x - dx} ${target.y}, ${target.x} ${target.y}` })
  }
  protocolEls.value.forEach((el, i) => push(el, vLeft, 'right', `p${i}`))
  upstreamEls.value.forEach((el, i) => push(el, vRight, 'left', `u${i}`))
  links.value = out
}

function scheduleDraw(): void {
  // 等布局稳定后再测量
  nextTick(() => requestAnimationFrame(drawLinks))
}

// 窗口尺寸变化时重算
function onResize(): void {
  drawLinks()
}

onMounted(() => {
  window.addEventListener('resize', onResize)
  scheduleDraw()
  // upstreams 异步加载完成后重绘
  listProviders()
    .then((data) => {
      upstreams.value = (data.connections ?? [])
        .filter((c) => c.isActive)
        .map((c) => ({
          id: c.id,
          provider: c.provider,
          name: c.name,
          model: c.defaultModel,
          baseUrl: c.baseUrl,
        }))
      scheduleDraw()
    })
    .catch(() => {
      /* ignore */
    })
})

watch(upstreams, () => scheduleDraw())

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize)
})
</script>
