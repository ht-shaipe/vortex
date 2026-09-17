<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="接入指南" sub="如何将客户端接入 Vortex 网关" />

    <!-- 切换标签栏：智能体接入 / OpenAI 协议 / Anthropic 协议 -->
    <div class="tabs">
      <button
        v-for="t in tabs"
        :key="t.id"
        class="tab"
        :class="{ active: active === t.id }"
        @click="active = t.id"
      >
        {{ t.label }}
      </button>
    </div>

    <!-- 智能体接入 -->
    <template v-if="active === 'agent'">
      <AgentIntegration />
    </template>

    <!-- OpenAI 协议 -->
    <template v-else-if="active === 'openai'">
      <!-- 第 1 步：启动网关 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 1 步 · 启动网关</div>
            <div class="card-sub">确认网关已在本地运行</div>
          </div>
        </div>
        <div class="card-body">
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">Vortex 在本地启动 API 服务器，对外提供 OpenAI 兼容协议：</p>
          <div class="url-row flex items-center gap-12px mb-8px">
            <span class="url-label w-90px shrink-0 text-12px font-medium text-ink-3">Base URL</span>
            <CopyableBlock :text="openaiBaseUrl" variant="inline">{{ openaiBaseUrl }}</CopyableBlock>
          </div>
          <div class="url-row flex items-center gap-12px mb-8px">
            <span class="url-label w-90px shrink-0 text-12px font-medium text-ink-3">访问令牌</span>
            <CopyableBlock :text="token" variant="inline">
              <span v-if="token" class="font-mono">{{ token }}</span>
              <span v-else class="text-ink-4">未生成，可在「设置 · 安全与访问」中开启并生成</span>
            </CopyableBlock>
          </div>
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">多数客户端要求 API Key 字段非空才会发起请求，接入时将上方访问令牌填入客户端的 API Key 字段即可；令牌可在「设置 · 安全与访问」中重新生成。</p>
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">在管理界面「订阅」页添加提供商连接与 API 密钥后即可使用。</p>
        </div>
      </div>

      <!-- 第 2 步：配置客户端 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 2 步 · 配置客户端</div>
            <div class="card-sub">将 base_url 指向 Vortex</div>
          </div>
        </div>
        <div class="card-body">
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">使用 OpenAI Python SDK：</p>
          <CopyableBlock :text="openaiSdkSnippet" lang="python" />
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px" style="margin-top: 16px">使用 cURL：</p>
          <CopyableBlock :text="openaiCurlSnippet" lang="bash" />
        </div>
      </div>

      <!-- 第 3 步：模型命名格式说明表 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 3 步 · 指定模型</div>
            <div class="card-sub">使用 provider/model 格式</div>
          </div>
        </div>
        <div class="card-body">
          <table class="table">
            <thead>
              <tr><th style="width: 180px">格式</th><th>示例</th><th style="width: 200px">说明</th></tr>
            </thead>
            <tbody>
              <tr><td class="mono">provider/model</td><td class="mono">openai/gpt-4o</td><td>显式指定提供商与模型</td></tr>
              <tr><td class="mono">别名前缀</td><td class="mono">ds-deepseek-chat</td><td>使用提供商别名</td></tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>

    <!-- Anthropic 协议 -->
    <template v-else-if="active === 'anthropic'">
      <!-- 第 1 步：启动网关 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 1 步 · 启动网关</div>
            <div class="card-sub">确认网关已在本地运行</div>
          </div>
        </div>
        <div class="card-body">
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">Vortex 在本地启动 API 服务器，对外提供 Anthropic 兼容协议：</p>
          <div class="url-row flex items-center gap-12px mb-8px">
            <span class="url-label w-90px shrink-0 text-12px font-medium text-ink-3">Base URL</span>
            <CopyableBlock :text="anthropicBaseUrl" variant="inline">{{ anthropicBaseUrl }}</CopyableBlock>
          </div>
          <div class="url-row flex items-center gap-12px mb-8px">
            <span class="url-label w-90px shrink-0 text-12px font-medium text-ink-3">访问令牌</span>
            <CopyableBlock :text="token" variant="inline">
              <span v-if="token" class="font-mono">{{ token }}</span>
              <span v-else class="text-ink-4">未生成，可在「设置 · 安全与访问」中开启并生成</span>
            </CopyableBlock>
          </div>
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">多数客户端要求 API Key 字段非空才会发起请求，接入时将上方访问令牌填入客户端的 API Key 字段即可；令牌可在「设置 · 安全与访问」中重新生成。</p>
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">在管理界面「订阅」页添加提供商连接与 API 密钥后即可使用。</p>
        </div>
      </div>

      <!-- 第 2 步：配置客户端 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 2 步 · 配置客户端</div>
            <div class="card-sub">将 base_url 指向 Vortex</div>
          </div>
        </div>
        <div class="card-body">
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px">使用 Anthropic Python SDK：</p>
          <CopyableBlock :text="anthropicSdkSnippet" lang="python" />
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px" style="margin-top: 16px">使用 Claude Code（设置环境变量）：</p>
          <CopyableBlock :text="claudeSnippet" lang="bash" />
          <p class="para text-13px text-ink-2 leading-[1.7] m-0 mb-12px" style="margin-top: 16px">使用 cURL：</p>
          <CopyableBlock :text="anthropicCurlSnippet" lang="bash" />
        </div>
      </div>

      <!-- 第 3 步：模型命名格式说明表 -->
      <div class="card mb-[var(--gap-lg)]">
        <div class="card-head">
          <div>
            <div class="card-title">第 3 步 · 指定模型</div>
            <div class="card-sub">使用 provider/model 格式</div>
          </div>
        </div>
        <div class="card-body">
          <table class="table">
            <thead>
              <tr><th style="width: 180px">格式</th><th>示例</th><th style="width: 200px">说明</th></tr>
            </thead>
            <tbody>
              <tr><td class="mono">provider/model</td><td class="mono">anthropic/claude-sonnet-4-20250514</td><td>显式指定提供商与模型</td></tr>
              <tr><td class="mono">别名前缀</td><td class="mono">claude-3-opus</td><td>使用提供商别名</td></tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 使用指南页面。
 * 职责：向用户展示如何将客户端接入 Vortex 网关，包括启动网关（base_url 与访问令牌）、
 * 配置客户端、指定模型格式三个步骤，并按 OpenAI / Anthropic 协议切换示例代码。
 */
import { computed, onMounted, ref } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import AgentIntegration from '@/components/sync/AgentIntegration.vue'
import { getSettings } from '@/api/settings'

// OpenAI 兼容协议的本地接入地址
const openaiBaseUrl = 'http://localhost:10168/v1'
// Anthropic 兼容协议的本地接入地址
const anthropicBaseUrl = 'http://localhost:10168/anthropic/v1'

// 访问令牌（从安全设置读取，客户端接入时填入 API Key 字段）
const token = ref('')

// 代码示例中使用的 API Key：优先真实令牌，未读取到前用占位符
const apiKey = computed(() => token.value || 'your-vortex-api-key')

// 切换标签定义：智能体接入 / OpenAI 协议 / Anthropic 协议
const tabs = [
  { id: 'agent', label: '智能体接入' },
  { id: 'openai', label: 'OpenAI 协议' },
  { id: 'anthropic', label: 'Anthropic 协议' },
]
// 当前选中的标签
const active = ref('agent')

// 加载安全设置中的访问令牌
onMounted(async () => {
  try {
    const data = await getSettings()
    const s = (data.security ?? {}) as Record<string, unknown>
    if (s.token != null) token.value = String(s.token)
  } catch {
    /* 后端未就绪时保持占位符 */
  }
})

// OpenAI Python SDK 用法示例
const openaiSdkSnippet = computed(() => `from openai import OpenAI

client = OpenAI(
    base_url="${openaiBaseUrl}",
    api_key="${apiKey.value}",
)

resp = client.chat.completions.create(
    model="openai/gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}],
)`)

// OpenAI cURL 用法示例
const openaiCurlSnippet = computed(() => `curl ${openaiBaseUrl}/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer ${apiKey.value}" \\
  -d '{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"Hi"}]}'`)

// Anthropic Python SDK 用法示例
const anthropicSdkSnippet = computed(() => `from anthropic import Anthropic

client = Anthropic(
    base_url="${anthropicBaseUrl}",
    api_key="${apiKey.value}",
)

resp = client.messages.create(
    model="anthropic/claude-sonnet-4-20250514",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello!"}],
)`)

// Anthropic cURL 用法示例
const anthropicCurlSnippet = computed(() => `curl ${anthropicBaseUrl}/messages \\
  -H "Content-Type: application/json" \\
  -H "x-api-key: ${apiKey.value}" \\
  -H "anthropic-version: 2023-06-01" \\
  -d '{"model":"anthropic/claude-sonnet-4-20250514","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}'`)

// Claude Code 环境变量配置示例
const claudeSnippet = computed(() => `# 设置环境变量
export ANTHROPIC_BASE_URL=${anthropicBaseUrl}
export ANTHROPIC_API_KEY=${apiKey.value}`)
</script>
