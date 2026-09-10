<template>
  <div>
    <PageHeader title="检查更新" sub="查看并安装新版本" />

    <div class="grid-2">
      <div class="card section">
        <div class="card-head">
          <div>
            <div class="card-title">版本信息</div>
            <div class="card-sub">当前安装的版本</div>
          </div>
        </div>
        <div class="card-body">
          <div class="setting-row">
            <div>
              <div class="setting-label">当前版本</div>
            </div>
            <span class="mono num">v0.1.0</span>
          </div>
          <div class="setting-row">
            <div>
              <div class="setting-label">更新源</div>
              <div class="setting-desc">Vortex 暂未内置自动更新</div>
            </div>
            <StatusBadge tone="neutral" label="手动更新" />
          </div>
        </div>
      </div>

      <div class="card section">
        <div class="card-head">
          <div>
            <div class="card-title">更新状态</div>
          </div>
          <StatusBadge tone="ok" label="已是最新" />
        </div>
        <div class="card-body">
          <p class="para">当前未检测到新版本。如需手动更新，请从项目发布页获取最新安装包。</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
</script>

<style scoped>
.grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: var(--gap-md); }
@media (max-width: 900px) { .grid-2 { grid-template-columns: 1fr; } }
.section { height: fit-content; }
.para { font-size: 13px; color: var(--ink-2); line-height: 1.7; margin: 0; }
</style>