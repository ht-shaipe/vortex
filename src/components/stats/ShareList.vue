<template>
  <div class="share-list">
    <div v-if="items.length === 0" class="share-empty">暂无数据</div>
    <div v-for="it in sorted" :key="it.name" class="share-row">
      <span class="share-name mono">{{ it.name }}</span>
      <div class="share-track">
        <div class="share-bar" :style="{ width: pct(it.value) + '%' }" />
      </div>
      <span class="share-val num">{{ it.value.toLocaleString() }}</span>
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
.share-list { display: flex; flex-direction: column; gap: 8px; }
.share-empty { font-size: 12.5px; color: var(--ink-4); padding: 8px 0; }
.share-row { display: grid; grid-template-columns: 140px 1fr 80px; gap: 10px; align-items: center; }
.share-name { font-size: 12px; color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.share-track { height: 6px; background: var(--surface-3); border-radius: 3px; overflow: hidden; }
.share-bar { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.3s; }
.share-val { font-size: 12px; color: var(--ink-3); text-align: right; }
</style>