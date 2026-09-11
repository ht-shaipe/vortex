<template>
  <div class="share-list flex flex-col gap-8px">
    <div v-if="items.length === 0" class="share-empty text-12.5px text-ink-4 py-8px">暂无数据</div>
    <div v-for="it in sorted" :key="it.name" class="share-row grid grid-cols-[140px_1fr_80px] gap-10px items-center">
      <span class="share-name mono text-12px text-ink-2 overflow-hidden text-ellipsis whitespace-nowrap">{{ it.name }}</span>
      <div class="share-track h-6px bg-surface-3 rounded-3px overflow-hidden">
        <div class="share-bar h-full bg-accent rounded-3px" :style="{ width: pct(it.value) + '%' }" />
      </div>
      <span class="share-val num text-12px text-ink-3 text-right">{{ it.value.toLocaleString() }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ShareList.vue — 分享列表
 * 职责：以横向条形图展示各项目的占比分布，按数值降序排列。
 */
import { computed } from 'vue'

// Props 定义：items 为名称-数值对列表
const props = defineProps<{ items: { name: string; value: number }[] }>()

const sorted = computed(() => [...props.items].sort((a, b) => b.value - a.value)) // 按数值降序排列
const max = computed(() => Math.max(1, ...sorted.value.map((i) => i.value))) // 最大值（用于条形宽度计算）

/** 计算某值占最大值的百分比。 */
function pct(v: number): number {
  return Math.round((v / max.value) * 100)
}
</script>

<style scoped>
.share-bar { transition: width 0.3s; }
</style>