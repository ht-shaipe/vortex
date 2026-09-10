<template>
  <span v-if="value >= 1e4" class="token-hint">{{ formatTokenCompact(value) }}</span>
</template>

<script setup lang="ts">
import { formatTokenCompact } from '@/lib/format'

/**
 * Token 辅助单位小字：仅当数值达到「万」量级（≥ 1e4）才展示折算单位，
 * 否则精确值本身已足够清晰。用作 StatCard 的 hint。
 */
defineProps<{ value: number }>()
</script>

<style scoped>
.token-hint { font-size: var(--fs-sm); color: var(--ink-3); }
</style>
