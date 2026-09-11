<template>
  <span class="pill" :class="tone">
    <el-icon :size="11"><component :is="icon" /></el-icon>
    {{ Math.abs(pct).toFixed(0) }}%
  </span>
</template>

<script setup lang="ts">
/**
 * TrendBadge.vue — 趋势徽章
 * 职责：以胶囊徽章展示百分比变化趋势，增长为正向（绿）、下降为负向（红）。
 */
import { computed } from 'vue'
import { Top, Bottom, Minus } from '@element-plus/icons-vue'

// Props 定义：pct 为百分比变化值（正/负/零）
const props = defineProps<{ pct: number }>()

/**
 * 涨=红 / 跌=绿 不适用于「请求量趋势」这类中性指标，
 * 增长为正向（绿），下降为负向（红）。
 */
const tone = computed(() => (props.pct > 0 ? 'ok' : props.pct < 0 ? 'err' : 'neutral')) // 色调
const icon = computed(() => (props.pct > 0 ? Top : props.pct < 0 ? Bottom : Minus)) // 方向图标
</script>
