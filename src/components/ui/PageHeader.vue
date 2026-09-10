<template>
  <div class="page-header">
    <div>
      <div class="page-title">{{ title }}</div>
      <div v-if="sub" class="page-sub">{{ sub }}</div>
    </div>
    <div v-if="$slots.actions" class="page-actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PageHeader.vue — 页面标题头
 * 职责：展示页面主标题、副标题及右侧操作插槽。
 */
// Props 定义：title 为必填标题，sub 为可选副标题
defineProps<{ title: string; sub?: string }>()
</script>