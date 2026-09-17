<template>
  <Teleport to="body">
    <div class="ob-overlay fixed inset-0 z-9999 flex items-center justify-center" style="background: rgba(0,0,0,0.45)" @click.self="skip">
      <div class="ob-card w-540px max-h-86vh bg-bg border border-solid border-line rounded-lg flex flex-col overflow-hidden" style="box-shadow: 0 8px 32px rgba(0,0,0,0.18)">
        <!-- 步骤指示器 + 跳过 -->
        <div class="flex items-center justify-between px-24px py-14px border-b border-solid border-0 border-line bg-surface-2">
          <div class="flex items-center gap-4px">
            <template v-for="i in totalSteps" :key="i">
              <div
                class="w-22px h-22px rounded-full flex items-center justify-center text-11px font-medium border border-solid transition-all"
                :class="step === i ? 'border-accent bg-accent text-white' : step > i ? 'border-ok bg-ok text-white' : 'border-line bg-surface text-ink-4'"
              >
                <el-icon v-if="step > i" :size="11"><Check /></el-icon>
                <span v-else>{{ i }}</span>
              </div>
              <div v-if="i < totalSteps" class="w-20px h-1px" :class="step > i ? 'bg-ok' : 'bg-line'" />
            </template>
          </div>
          <button class="text-12px text-ink-4 hover:text-ink-2 transition-colors cursor-pointer" @click="skip">跳过引导</button>
        </div>

        <!-- 内容区 -->
        <div class="flex-1 overflow-y-auto px-32px py-24px">
          <!-- 步骤 1：欢迎 -->
          <template v-if="step === 1">
            <div class="flex flex-col items-center text-center gap-14px">
              <div class="w-52px h-52px rounded-full flex items-center justify-center" style="background: var(--accent); opacity: 0.12">
                <el-icon :size="26" style="color: var(--accent)"><Promotion /></el-icon>
              </div>
              <h2 class="text-19px font-bold text-ink m-0">欢迎使用 Vortex</h2>
              <p class="text-13px text-ink-3 leading-[1.7] m-0 max-w-400px">
                Vortex 是一个 AI API 网关，将多个 AI 提供商统一为一个接口，支持故障转移、负载均衡与实时监控。
              </p>
              <div class="grid grid-cols-1 gap-8px w-full mt-4px">
                <div class="flex items-center gap-10px px-14px py-10px bg-surface-2 rounded-sm">
                  <el-icon :size="16" style="color: var(--accent)" class="shrink-0"><Connection /></el-icon>
                  <span class="text-12.5px text-ink-2">多提供商路由与自动故障转移</span>
                </div>
                <div class="flex items-center gap-10px px-14px py-10px bg-surface-2 rounded-sm">
                  <el-icon :size="16" style="color: var(--accent)" class="shrink-0"><Link /></el-icon>
                  <span class="text-12.5px text-ink-2">OpenAI / Anthropic 协议兼容输出</span>
                </div>
                <div class="flex items-center gap-10px px-14px py-10px bg-surface-2 rounded-sm">
                  <el-icon :size="16" style="color: var(--accent)" class="shrink-0"><DataLine /></el-icon>
                  <span class="text-12.5px text-ink-2">实时路由监控与请求统计</span>
                </div>
              </div>
            </div>
          </template>

          <!-- 步骤 2：添加提供方 -->
          <template v-else-if="step === 2">
            <h2 class="text-16px font-bold text-ink m-0 mb-6px">添加 AI 提供方</h2>
            <p class="text-13px text-ink-3 leading-[1.7] m-0 mb-18px">选择一个 AI 提供商接入 Vortex，添加后即可在网关中使用其模型。</p>
            <div class="grid grid-cols-1 gap-10px">
              <button
                class="flex items-start gap-14px p-14px border border-solid border-line rounded-sm bg-surface-2 hover:border-accent transition-all text-left cursor-pointer w-full"
                @click="goTo('/free-tokens')"
              >
                <div class="w-38px h-38px rounded-full flex items-center justify-center shrink-0" style="background: var(--accent); opacity: 0.12">
                  <el-icon :size="19" style="color: var(--accent)"><Present /></el-icon>
                </div>
                <div class="flex-1">
                  <div class="text-14px font-semibold text-ink mb-3px">去薅Token</div>
                  <div class="text-12px text-ink-3 leading-[1.5]">浏览免费 Token 站点，找到合适的平台后点击「配置使用」一键接入</div>
                </div>
                <el-icon :size="15" class="text-ink-4 shrink-0 mt-4px"><Right /></el-icon>
              </button>
              <button
                class="flex items-start gap-14px p-14px border border-solid border-line rounded-sm bg-surface-2 hover:border-accent transition-all text-left cursor-pointer w-full"
                @click="goTo('/subscriptions/new')"
              >
                <div class="w-38px h-38px rounded-full flex items-center justify-center shrink-0" style="background: var(--accent); opacity: 0.12">
                  <el-icon :size="19" style="color: var(--accent)"><Key /></el-icon>
                </div>
                <div class="flex-1">
                  <div class="text-14px font-semibold text-ink mb-3px">手动添加</div>
                  <div class="text-12px text-ink-3 leading-[1.5]">从内置提供商中选择，或填写自定义 OpenAI 兼容端点</div>
                </div>
                <el-icon :size="15" class="text-ink-4 shrink-0 mt-4px"><Right /></el-icon>
              </button>
            </div>
            <div class="mt-14px px-14px py-10px bg-surface-2 rounded-sm text-12px text-ink-4 leading-[1.6] flex items-center gap-6px">
              <el-icon :size="13" class="shrink-0"><InfoFilled /></el-icon>
              <span>添加后可在「订阅管理」中查看和管理所有连接</span>
            </div>
          </template>

          <!-- 步骤 3：接入客户端 -->
          <template v-else-if="step === 3">
            <h2 class="text-16px font-bold text-ink m-0 mb-6px">接入客户端</h2>
            <p class="text-13px text-ink-3 leading-[1.7] m-0 mb-18px">将你的客户端（Cursor、Claude Code、SDK 等）指向 Vortex 网关即可开始使用。</p>
            <div class="flex flex-col gap-10px">
              <div class="flex items-center gap-12px">
                <span class="w-72px shrink-0 text-12px font-medium text-ink-3">OpenAI</span>
                <CopyableBlock :text="openaiBaseUrl" variant="inline">{{ openaiBaseUrl }}</CopyableBlock>
              </div>
              <div class="flex items-center gap-12px">
                <span class="w-72px shrink-0 text-12px font-medium text-ink-3">Anthropic</span>
                <CopyableBlock :text="anthropicBaseUrl" variant="inline">{{ anthropicBaseUrl }}</CopyableBlock>
              </div>
              <div class="flex items-center gap-12px">
                <span class="w-72px shrink-0 text-12px font-medium text-ink-3">访问令牌</span>
                <CopyableBlock :text="token || '未设置'" variant="inline">
                  <span v-if="token" class="font-mono">{{ token }}</span>
                  <span v-else class="text-ink-4">未设置，可在「设置」中生成</span>
                </CopyableBlock>
              </div>
            </div>
            <div class="mt-14px px-14px py-10px bg-surface-2 rounded-sm text-12px text-ink-4 leading-[1.6] flex items-center gap-6px">
              <el-icon :size="13" class="shrink-0"><InfoFilled /></el-icon>
              <span>在客户端的 API Key 字段填入访问令牌，Base URL 填入上方地址即可</span>
            </div>
          </template>

          <!-- 步骤 4：完成 -->
          <template v-else>
            <div class="flex flex-col items-center text-center gap-14px">
              <div class="w-52px h-52px rounded-full flex items-center justify-center" style="background: var(--ok); opacity: 0.12">
                <el-icon :size="26" style="color: var(--ok)"><Check /></el-icon>
              </div>
              <h2 class="text-19px font-bold text-ink m-0">一切就绪！</h2>
              <p class="text-13px text-ink-3 leading-[1.7] m-0 max-w-400px">
                你已完成基本配置，现在可以开始使用 Vortex 了。
              </p>
              <div class="grid grid-cols-3 gap-8px w-full mt-4px">
                <button
                  class="flex flex-col items-center gap-5px p-12px border border-solid border-line rounded-sm bg-surface-2 hover:border-accent transition-all cursor-pointer"
                  @click="goTo('/live-routing')"
                >
                  <el-icon :size="20" style="color: var(--accent)"><DataLine /></el-icon>
                  <span class="text-11.5px text-ink-2">实时路由</span>
                </button>
                <button
                  class="flex flex-col items-center gap-5px p-12px border border-solid border-line rounded-sm bg-surface-2 hover:border-accent transition-all cursor-pointer"
                  @click="goTo('/chat')"
                >
                  <el-icon :size="20" style="color: var(--accent)"><ChatDotRound /></el-icon>
                  <span class="text-11.5px text-ink-2">对话测试</span>
                </button>
                <button
                  class="flex flex-col items-center gap-5px p-12px border border-solid border-line rounded-sm bg-surface-2 hover:border-accent transition-all cursor-pointer"
                  @click="goTo('/guide')"
                >
                  <el-icon :size="20" style="color: var(--accent)"><Reading /></el-icon>
                  <span class="text-11.5px text-ink-2">接入指南</span>
                </button>
              </div>
            </div>
          </template>
        </div>

        <!-- 底部导航 -->
        <div class="flex items-center justify-between px-24px py-14px border-t border-solid border-0 border-line bg-surface-2">
          <button v-if="step > 1" class="btn" @click="step--">
            <el-icon :size="14" class="mr-4px"><ArrowLeft /></el-icon>上一步
          </button>
          <div v-else />
          <button v-if="step < totalSteps" class="btn primary" @click="step++">
            下一步<el-icon :size="14" class="ml-4px"><Right /></el-icon>
          </button>
          <button v-else class="btn primary" @click="finish">开始使用 Vortex</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 新用户引导向导。
 * 职责：首次启动且无订阅时自动展示，分 4 步引导用户完成基本配置——
 * 欢迎介绍、添加提供方、接入客户端、完成。完成后写入 localStorage 标记不再展示。
 */
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  Check, Right, ArrowLeft, InfoFilled,
  Promotion, Present, Key, Connection, Link,
  DataLine, ChatDotRound, Reading,
} from '@element-plus/icons-vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { getSettings } from '@/api/settings'

const emit = defineEmits<{ done: [] }>()
const router = useRouter()

const totalSteps = 4
const step = ref(1)

const openaiBaseUrl = 'http://localhost:10168/v1'
const anthropicBaseUrl = 'http://localhost:10168/anthropic/v1'
const token = ref('')

onMounted(async () => {
  try {
    const data = await getSettings()
    const s = (data.security ?? {}) as Record<string, unknown>
    if (s.token != null) token.value = String(s.token)
  } catch { /* 后端未就绪 */ }
})

function markDone() {
  localStorage.setItem('vortex-onboarding-completed', '1')
}

function skip() {
  markDone()
  emit('done')
}

function finish() {
  markDone()
  emit('done')
}

function goTo(path: string) {
  markDone()
  emit('done')
  router.push(path)
}
</script>
