<template>
  <div>
    <PageHeader title="关于" sub="Vortex AI Gateway" />

    <div class="card section">
      <div class="card-body about-hero">
        <div class="app-mark">V</div>
        <div class="about-info">
          <div class="about-name">Vortex</div>
          <div class="about-meta mono">v0.1.0 · MIT License · Tauri 2 + Rust + Vue 3</div>
          <p class="about-desc">统一的 AI 网关桌面应用，将 21+ 个 AI 提供商聚合为 OpenAI 兼容 API。支持 17 种路由策略、组合模型、API 密钥管理与用量统计。</p>
          <div class="about-links">
            <a class="btn" href="https://v2.tauri.app" target="_blank" rel="noopener">Tauri 文档</a>
            <a class="btn" href="https://element-plus.org" target="_blank" rel="noopener">Element Plus</a>
          </div>
        </div>
      </div>
    </div>

    <div class="card section">
      <div class="card-head">
        <div>
          <div class="card-title">免责声明</div>
        </div>
      </div>
      <div class="card-body">
        <p class="para">本软件仅作为本地 API 网关代理工具，不存储、不转发你的对话内容到任何第三方服务。所有请求直接从你的设备发往你配置的 AI 提供商。</p>
        <p class="para">请确保你拥有所使用 AI 提供商 API 的合法访问权限，并遵守各提供商的服务条款。使用本软件产生的任何费用由用户自行承担。</p>
        <p class="para">本软件按「现状」提供，不提供任何明示或暗示的担保。</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import PageHeader from '@/components/ui/PageHeader.vue'
</script>

<style scoped>
.section { margin-bottom: var(--gap-lg); }
.about-hero { display: flex; gap: 20px; align-items: flex-start; }
.app-mark {
  width: 56px; height: 56px;
  border-radius: 14px;
  background: var(--ink);
  color: var(--accent);
  display: grid;
  place-items: center;
  font-weight: 800;
  font-size: 26px;
  flex-shrink: 0;
}
.about-name { font-size: 20px; font-weight: 700; }
.about-meta { font-size: 11.5px; color: var(--ink-3); margin-top: 4px; }
.about-desc { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 12px 0; }
.about-links { display: flex; gap: var(--gap-sm); }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.75; margin: 0 0 10px; }
.para:last-child { margin-bottom: 0; }
</style>