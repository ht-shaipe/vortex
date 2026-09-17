<template>
  <div class="live-root flex-1 min-h-0 flex flex-col" data-tauri-drag-region>
    <PageHeader title="实时路由" sub="网关拓扑、接入地址与 API 端点一览" class="px-28px py-8px" />

    <el-scrollbar class="live-scroll flex-1">
      <div class="w-full p-24px">
        <!-- 拓扑简图（统计面板悬浮在左上角） -->
        <div class="flex gap-[var(--gap-lg)] items-start mb-[var(--gap-lg)]">
          <!-- 拓扑简图：对外协议 → Vortex → 上游提供商 -->
          <div class="card p-[var(--pad-card)] flex-1 min-w-0 relative min-h-360px">
            <!-- 左上悬浮：今日运行 + 网关概览 -->
            <div class="absolute left-18px top-18px z-2 w-232px rounded-10px border border-solid border-line bg-surface-2 shadow-lg p-14px flex flex-col gap-11px">
              <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">今日</div>
              <div class="flex items-baseline justify-between gap-8px">
                <span class="text-12px text-ink-3">请求</span>
                <span class="flex items-center gap-5px">
                  <span class="font-mono text-16px font-semibold text-ink leading-none tabular-nums">{{ todayStats?.requests ?? '—' }}</span>
                  <TrendBadge v-if="reqTrendPct !== null" :pct="reqTrendPct ?? 0" />
                </span>
              </div>
              <div class="flex items-baseline justify-between gap-8px">
                <span class="text-12px text-ink-3">错误</span>
                <span
                  class="font-mono text-16px font-semibold leading-none tabular-nums"
                  :class="(todayStats?.errors ?? 0) > 0 ? 'text-err' : 'text-ink'"
                >{{ todayStats?.errors ?? '—' }}</span>
              </div>
              <div class="flex items-baseline justify-between gap-8px">
                <span class="text-12px text-ink-3">输入 Token</span>
                <span
                  class="font-mono text-16px font-semibold text-ink leading-none tabular-nums"
                  :title="fmtInt(todayStats?.inputTokens)"
                >{{ todayStats ? formatTokenCompact(todayStats.inputTokens) : '—' }}</span>
              </div>
              <div class="flex items-baseline justify-between gap-8px">
                <span class="text-12px text-ink-3">输出 Token</span>
                <span
                  class="font-mono text-16px font-semibold text-ink leading-none tabular-nums"
                  :title="fmtInt(todayStats?.outputTokens)"
                >{{ todayStats ? formatTokenCompact(todayStats.outputTokens) : '—' }}</span>
              </div>
              <div class="h-1px shrink-0 bg-line-2 my-1px"></div>
              <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">网关概览</div>
              <div class="flex items-baseline justify-between gap-8px" v-for="s in overview" :key="s.label">
                <span class="text-12px text-ink-3">{{ s.label }}</span>
                <span
                  class="font-mono text-16px font-semibold text-ink leading-none tabular-nums"
                  :title="s.hint"
                >{{ s.value }}</span>
              </div>
            </div>

            <div ref="topoEl" class="topology relative flex items-center justify-end gap-32px py-14px pl-250px pr-32px">
              <!-- 连接线 SVG 覆盖层（曲线） -->
              <svg v-show="links.length > 0" class="topo-links absolute inset-0 pointer-events-none" :width="topoSize.w"
                :height="topoSize.h" :viewBox="`0 0 ${topoSize.w} ${topoSize.h}`" preserveAspectRatio="none"
                aria-hidden="true">
                <path v-for="l in links" :key="l.key" :d="l.d" fill="none" stroke="var(--line-2)" stroke-width="1.5"
                  stroke-linecap="round" />
              </svg>

              <!-- 左列：对外协议 -->
              <div class="flex flex-col gap-8px min-w-222px w-222px pt-4px items-center">
                <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">对外协议</div>
                <div class="relative flex flex-col gap-8px w-full">
                  <div ref="protocolEls"
                    class="relative z-1 flex items-center gap-10px py-9px px-12px rounded-md border border-solid border-line bg-surface-2 text-body transition-[background-color,border-color,transform] duration-150 hover:bg-surface-3 hover:border-line-2 hover:-translate-y-1px"
                    v-for="p in protocols" :key="p.name">
                    <span
                      class="grid place-items-center w-32px h-22px rounded-5px text-10.5px font-extrabold shrink-0 tracking-[0.02em]"
                      :class="[p.cls, p.cls === 'oai' ? 'bg-[oklch(0.90_0.05_150)] text-[oklch(0.36_0.12_150)]' : 'bg-[oklch(0.88_0.08_280)] text-[oklch(0.38_0.15_280)]']">{{
                      p.tag }}</span>
                    <span class="text-ink-2 font-medium">{{ p.name }}</span>
                  </div>
                </div>
              </div>

              <!-- 中列：Vortex 网关中心节点 -->
              <div ref="vortexEl" class="relative z-1 flex flex-col items-center gap-8px">
                <div
                  class="w-2px h-52px bg-gradient-to-b from-line-2 to-line rounded-1px relative after:content-[''] after:absolute after:left-1/2 after:top-1/2 after:-translate-x-1/2 after:-translate-y-1/2 after:w-5px after:h-5px after:rounded-full after:bg-line-2" />
                <div
                  class="w-116px h-116px rounded-30px grid place-items-center text-16px font-bold bg-accent-bg text-accent-ink border border-solid border-accent-line shadow-sm tracking-[0.01em]">
                  Vortex</div>
                <div
                  class="w-2px h-52px bg-gradient-to-b from-line-2 to-line rounded-1px relative after:content-[''] after:absolute after:left-1/2 after:top-1/2 after:-translate-x-1/2 after:-translate-y-1/2 after:w-5px after:h-5px after:rounded-full after:bg-line-2" />
              </div>

              <!-- 右列：上游提供商列表 -->
              <div class="flex flex-col gap-8px min-w-240px w-240px pt-4px items-center">
                <div class="text-xs text-ink-4 uppercase tracking-[0.04em] leading-none">上游提供商</div>
                <div class="relative flex flex-col gap-8px w-full">
                  <template v-if="upstreams.length > 0">
                    <div ref="upstreamEls"
                      class="relative z-1 flex items-center gap-10px py-9px px-12px rounded-md bg-surface-2 border border-solid border-line transition-[background-color,border-color,transform] duration-150 hover:bg-surface-3 hover:border-line-2 hover:-translate-y-1px"
                      v-for="p in upstreams" :key="p.id">
                      <ProviderLogo :name="p.provider" :hint="`${p.name} ${p.baseUrl || ''}`" :size="18"
                        class="shrink-0" />
                      <span class="text-body text-ink-2 font-medium flex-1 min-w-0 truncate" :title="p.name">{{ p.name
                        }}</span>
                      <span
                        class="shrink-0 ml-2px max-w-96px text-right text-10.5px font-mono leading-tight text-ink-3 bg-surface-3 border border-solid border-line rounded-4px py-2px px-6px whitespace-nowrap overflow-hidden text-ellipsis"
                        :title="p.model || '默认模型'">{{ p.model || '默认' }}</span>
                    </div>
                  </template>
                  <div v-else
                    class="relative z-1 text-sm text-ink-4 p-10px text-center border border-dashed border-line rounded-md">
                    暂无活跃连接，请先在订阅页添加</div>
                </div>
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
            <div v-for="a in apis" :key="a.method + a.path"
              class="grid grid-cols-[56px_1fr_auto] gap-12px items-center py-9px border-b border-line border-solid border-0 last:border-b-0">
              <span
                class="api-method mono text-11px font-bold [&.oai]:text-[oklch(0.45_0.12_150)] [&.ant]:text-[oklch(0.50_0.15_280)] [&.mgmt]:text-ink-4"
                :class="a.group">{{ a.method }}</span>
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
import { onBeforeUnmount, onMounted, onUpdated, nextTick, ref, computed, watch } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { listProviders } from '@/api/providers'
import { modelCatalogApi } from '@/api/modelCatalog'
import { statsApi, type StatsOverview } from '@/api/stats'
import TrendBadge from '@/components/stats/TrendBadge.vue'
import { formatTokenCompact, fmtInt } from '@/lib/format'

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

// ===== 今日运行数据 =====
const statsOverview = ref<StatsOverview | null>(null)
const todayStats = computed(() => statsOverview.value?.today)
/** 请求数环比（今日 vs 昨日），无数据时为 null */
const reqTrendPct = computed<number | null>(() => statsOverview.value?.trend?.requestsPct ?? null)

// ===== 网关概览统计 =====
const providersCount = ref<number | '—'>('—')
const activeConns = ref<number | '—'>('—')
const catalogCount = ref<number | '—'>('—')
const overview = computed(() => [
  { label: '供应方', value: providersCount.value, hint: '已配置的上游供应方（连接）数量' },
  { label: '活跃连接', value: activeConns.value, hint: '当前启用的连接数量' },
  { label: '模型目录', value: catalogCount.value, hint: '模型目录中的模型总数' },
])

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
  const nextSize = { w: Math.round(cr.width), h: Math.round(cr.height) }
  if (nextSize.w !== topoSize.value.w || nextSize.h !== topoSize.value.h) topoSize.value = nextSize

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
  // 仅在路径实际变化时更新，避免「测量 → 重渲染 → 再测量」的循环
  const changed =
    out.length !== links.value.length || out.some((l, i) => l.key !== links.value[i]?.key || l.d !== links.value[i]?.d)
  if (changed) links.value = out
}

let drawScheduled = false
function scheduleDraw(): void {
  // 等布局稳定后再测量；同一帧内多次请求只执行一次
  if (drawScheduled) return
  drawScheduled = true
  nextTick(() => {
    requestAnimationFrame(() => {
      drawScheduled = false
      drawLinks()
    })
  })
}

let resizeObserver: ResizeObserver | null = null

// 窗口尺寸变化时重算
function onResize(): void {
  drawLinks()
}

onMounted(() => {
  window.addEventListener('resize', onResize)
  // 容器尺寸变化（窗口缩放、侧栏折叠等）时重算连线
  if (topoEl.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => scheduleDraw())
    resizeObserver.observe(topoEl.value)
  }
  scheduleDraw()
  // upstreams 异步加载完成后重绘
  listProviders()
    .then((data) => {
      providersCount.value = data.connections?.length ?? 0
      activeConns.value = (data.connections ?? []).filter((c) => c.isActive).length
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
  // 模型目录数量（失败时保持 —）
  modelCatalogApi
    .list()
    .then((entries) => {
      catalogCount.value = entries?.length ?? 0
    })
    .catch(() => {
      /* ignore */
    })
  // 今日运行数据（请求/错误/Token，失败时保持 —）
  statsApi
    .getStats()
    .then((ov) => {
      statsOverview.value = ov
    })
    .catch(() => {
      /* ignore */
    })
})

watch(upstreams, () => scheduleDraw())

// 组件重新渲染（HMR、布局变化、数据刷新）后重绘连线，避免曲线停留在旧坐标
onUpdated(() => scheduleDraw())

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize)
  resizeObserver?.disconnect()
  resizeObserver = null
})
</script>
