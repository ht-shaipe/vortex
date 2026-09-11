<template>
  <span v-if="value >= 1e4" class="token-hint text-sm text-ink-3">{{ formatTokenCompact(value) }}</span>
</template>

<script setup lang="ts">
/**
 * TokenHint.vue — Token 提示
 * 职责：当 Token 数值达到万级时展示折算单位小字，用作 StatCard 的辅助提示。
 */
import { formatTokenCompact } from '@/lib/format'

/**
 * Token 辅助单位小字：仅当数值达到「万」量级（≥ 1e4）才展示折算单位，
 * 否则精确值本身已足够清晰。用作 StatCard 的 hint。
 */
// Props 定义：value 为 Token 数值
defineProps<{ value: number }>()
</script>


