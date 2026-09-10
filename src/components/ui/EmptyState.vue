<template>
  <div class="empty-state">
    <div class="empty-icon"><component :is="icon" /></div>
    <div class="empty-title">{{ title }}</div>
    <div v-if="desc" class="empty-desc">{{ desc }}</div>
    <slot />
  </div>
</template>

<script setup lang="ts">
/**
 * EmptyState.vue — 空状态
 * 职责：展示空数据占位图标、标题、描述及额外插槽内容。
 */
import { DataBoard } from '@element-plus/icons-vue'

// Props 定义：icon 为图标组件，title 为标题，desc 为描述文案
withDefaults(
  defineProps<{ icon?: unknown; title?: string; desc?: string }>(),
  { icon: DataBoard, title: '', desc: '' },
)
</script>

<style scoped>
.empty-state {
  padding: 48px 24px;
  text-align: center;
  color: var(--ink-3);
}
.empty-icon { width: 32px; height: 32px; margin: 0 auto 12px; color: var(--ink-4); }
.empty-icon :deep(svg) { width: 32px; height: 32px; }
.empty-title { font-size: 14px; font-weight: 600; color: var(--ink-2); margin-bottom: 6px; }
.empty-desc { font-size: 12.5px; line-height: 1.6; margin-bottom: 16px; }
</style>