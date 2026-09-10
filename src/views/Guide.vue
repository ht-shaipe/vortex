<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="接入指南" sub="如何将客户端接入 Vortex 网关" />

    <!-- 协议切换标签栏：OpenAI / Anthropic -->
    <div class="tabs">
      <button v-for="t in tabs" :key="t.id" class="tab" :class="{ active: active === t.id }" @click="active = t.id">
        {{ t.label }}
      </button>
    </div>

    <!-- 第 1 步：启动网关，展示对外协议的 base_url -->
    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">第 1 步 · 启动网关</div>
          <div class="card-sub">确认网关已在本地运行</div>
        </div>
      </div>
      <div class="card-body">
        <p class="para">Vortex 在本地启动 API 服务器，对外提供 OpenAI 兼容与 Anthropic 兼容两种协议：</p>
        <div class="url-row">
          <span class="url-label">OpenAI</span>
          <CopyableBlock :text="openaiBaseUrl" variant="inline">{{ openaiBaseUrl }}</CopyableBlock>
        </div>
        <div class="url-row">
          <span class="url-label">Anthropic</span>
          <CopyableBlock :text="anthropicBaseUrl" variant="inline">{{ anthropicBaseUrl }}</CopyableBlock>
        </div>
        <p class="para">在管理界面「订阅」页添加提供商连接与 API 密钥后即可使用。</p>
      </div>
    </div>

    <!-- 第 2 步：配置客户端，按所选协议展示对应代码示例 -->
    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">第 2 步 · 配置客户端</div>
          <div class="card-sub">将 base_url 指向 Vortex</div>
        </div>
      </div>
      <div class="card-body">
        <!-- OpenAI 协议示例：SDK 与 cURL -->
        <template v-if="active === 'openai'">
          <p class="para">使用 OpenAI Python SDK：</p>
          <CopyableBlock :text="openaiSdkSnippet" lang="python" />
          <p class="para" style="margin-top: 16px">使用 cURL：</p>
          <CopyableBlock :text="openaiCurlSnippet" lang="bash" />
        </template>
        <!-- Anthropic 协议示例：SDK、Claude Code 环境变量与 cURL -->
        <template v-else>
          <p class="para">使用 Anthropic Python SDK：</p>
          <CopyableBlock :text="anthropicSdkSnippet" lang="python" />
          <p class="para" style="margin-top: 16px">使用 Claude Code（设置环境变量）：</p>
          <CopyableBlock :text="claudeSnippet" lang="bash" />
          <p class="para" style="margin-top: 16px">使用 cURL：</p>
          <CopyableBlock :text="anthropicCurlSnippet" lang="bash" />
        </template>
      </div>
    </div>

    <!-- 第 3 步：模型命名格式说明表 -->
    <div class="card section">
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
  </div>
</template>

<script setup lang="ts">
/**
 * 使用指南页面。
 * 职责：向用户展示如何将客户端接入 Vortex 网关，包括启动网关、配置客户端
 * base_url、指定模型格式三个步骤，并按 OpenAI / Anthropic 协议切换示例代码。
 */
import { ref } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'

// OpenAI 兼容协议的本地接入地址
const openaiBaseUrl = 'http://localhost:20128/v1'
// Anthropic 兼容协议的本地接入地址
const anthropicBaseUrl = 'http://localhost:20128/anthropic/v1'

// 协议切换标签定义
const tabs = [
  { id: 'openai', label: 'OpenAI 协议' },
  { id: 'anthropic', label: 'Anthropic 协议' },
]
// 当前选中的协议标签
const active = ref('openai')

// OpenAI Python SDK 用法示例
const openaiSdkSnippet = `from openai import OpenAI

client = OpenAI(
    base_url="${openaiBaseUrl}",
    api_key="your-vortex-api-key",
)

resp = client.chat.completions.create(
    model="openai/gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}],
)`

// OpenAI cURL 用法示例
const openaiCurlSnippet = `curl ${openaiBaseUrl}/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer your-vortex-api-key" \\
  -d '{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"Hi"}]}'`

// Anthropic Python SDK 用法示例
const anthropicSdkSnippet = `from anthropic import Anthropic

client = Anthropic(
    base_url="${anthropicBaseUrl}",
    api_key="your-vortex-api-key",
)

resp = client.messages.create(
    model="anthropic/claude-sonnet-4-20250514",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello!"}],
)`

// Anthropic cURL 用法示例
const anthropicCurlSnippet = `curl ${anthropicBaseUrl}/messages \\
  -H "Content-Type: application/json" \\
  -H "x-api-key: your-vortex-api-key" \\
  -H "anthropic-version: 2023-06-01" \\
  -d '{"model":"anthropic/claude-sonnet-4-20250514","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}'`

// Claude Code 环境变量配置示例
const claudeSnippet = `# 设置环境变量
export ANTHROPIC_BASE_URL=${anthropicBaseUrl}
export ANTHROPIC_API_KEY=your-vortex-api-key`
</script>

<style scoped>
.section { margin-bottom: var(--gap-lg); }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 0 0 12px; }
.url-row { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; }
.url-label { width: 90px; flex-shrink: 0; font-size: 12px; font-weight: 500; color: var(--ink-3); }
</style>
