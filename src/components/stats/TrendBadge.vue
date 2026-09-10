<template>
  <span class="pill" :class="tone">
    <el-icon :size="11"><component :is="icon" /></el-icon>
    {{ Math.abs(pct).toFixed(0) }}%
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Top, Bottom, Minus } from '@element-plus/icons-vue'

const props = defineProps<{ pct: number }>()

/**
 * 涨=红 / 跌=绿 不适用于「请求量趋势」这类中性指标，
 * 这里沿用 ccMesh 语义：增长为正向（绿），下降为负向（红）。
 */
const tone = computed(() => (props.pct > 0 ? 'ok' : props.pct < 0 ? 'err' : 'neutral'))
const icon = computed(() => (props.pct > 0 ? Top : props.pct < 0 ? Bottom : Minus))
</script>
