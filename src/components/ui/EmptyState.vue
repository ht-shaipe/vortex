<template>
  <div class="empty-state py-48px px-24px text-center text-ink-3">
    <div class="empty-icon w-32px h-32px mx-auto mb-12px text-ink-4"><component :is="icon" /></div>
    <div class="empty-title text-14px font-semibold text-ink-2 mb-6px">{{ title }}</div>
    <div v-if="desc" class="empty-desc text-12.5px leading-1.6 mb-16px">{{ desc }}</div>
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
.empty-icon :deep(svg) { width: 32px; height: 32px; }
</style>