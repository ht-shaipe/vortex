<template>
  <div>
    <PageHeader title="接入指南" sub="如何将客户端接入 Vortex 网关" />

    <div class="tabs">
      <button v-for="t in tabs" :key="t.id" class="tab" :class="{ active: active === t.id }" @click="active = t.id">
        {{ t.label }}
      </button>
    </div>

    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">第 1 步 · 启动网关</div>
          <div class="card-sub">确认网关已在本地运行</div>
        </div>
      </div>
      <div class="card-body">
        <p class="para">Vortex 在本地启动一个 OpenAI 兼容的 API 服务器，默认监听：</p>
        <CopyableBlock :text="baseUrl" variant="inline">{{ baseUrl }}</CopyableBlock>
        <p class="para">在管理界面「订阅」页添加提供商连接与 API 密钥后即可使用。</p>
      </div>
    </div>

    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">第 2 步 · 配置客户端</div>
          <div class="card-sub">将 base_url 指向 Vortex</div>
        </div>
      </div>
      <div class="card-body">
        <template v-if="active === 'openai'">
          <p class="para">使用 OpenAI Python SDK：</p>
          <CopyableBlock :text="openaiSnippet">{{ openaiSnippet }}</CopyableBlock>
        </template>
        <template v-else-if="active === 'curl'">
          <p class="para">使用 cURL：</p>
          <CopyableBlock :text="curlSnippet">{{ curlSnippet }}</CopyableBlock>
        </template>
        <template v-else>
          <p class="para">在 Claude Code 设置中将 API 端点指向 Vortex：</p>
          <CopyableBlock :text="claudeSnippet">{{ claudeSnippet }}</CopyableBlock>
        </template>
      </div>
    </div>

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
            <tr><td class="mono">组合名</td><td class="mono">my-combo</td><td>使用预定义组合模型</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'

const baseUrl = 'http://localhost:20128/v1'
const tabs = [
  { id: 'openai', label: 'OpenAI SDK' },
  { id: 'curl', label: 'cURL' },
  { id: 'claude', label: 'Claude Code' },
]
const active = ref('openai')

const openaiSnippet = `from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:20128/v1",
    api_key="your-vortex-api-key",
)

resp = client.chat.completions.create(
    model="openai/gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}],
)`

const curlSnippet = `curl http://localhost:20128/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer your-vortex-api-key" \\
  -d '{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"Hi"}]}'`

const claudeSnippet = `# 设置环境变量
export ANTHROPIC_BASE_URL=http://localhost:20128
export ANTHROPIC_API_KEY=your-vortex-api-key`
</script>

<style scoped>
.section { margin-bottom: var(--gap-lg); }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 0 0 12px; }
</style>