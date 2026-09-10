<template>
  <span class="pill" :class="tone">
    <span v-if="dot" class="dot" />
    <slot>{{ label }}</slot>
  </span>
</template>

<script setup lang="ts">
/**
 * StatusBadge.vue — 状态徽章
 * 职责：以胶囊样式展示状态标签，支持不同色调与可选圆点指示。
 */
// Props 定义：tone 控制色调，label 为文案，dot 控制是否显示圆点
withDefaults(
  defineProps<{
    tone?: 'ok' | 'warn' | 'err' | 'neutral'
    label?: string
    dot?: boolean
  }>(),
  { tone: 'neutral', dot: true, label: '' },
)
</script>