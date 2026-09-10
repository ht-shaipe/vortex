<template>
  <div class="card stat-card">
    <span class="stat-label">{{ label }}</span>
    <div class="stat-line" :class="{ below: hintBelow }">
      <span class="stat-val tnum">{{ display }}</span>
      <slot name="hint" />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * StatCard.vue — 统计卡片
 * 职责：展示单个统计指标的标签与数值，支持辅助提示插槽（水平或垂直布局）。
 */
import { computed } from 'vue'

// Props 定义：label 为标签，value 为数值或文本，hintBelow 控制辅助提示布局方向
const props = withDefaults(
  defineProps<{
    label: string
    value: number | string
    /** true 时辅助提示在数值下方（垂直堆叠）；默认在右侧（水平）。 */
    hintBelow?: boolean
  }>(),
  { hintBelow: false },
)

// 显示值：数值类型时添加千分位分隔符
const display = computed(() =>
  typeof props.value === 'number' ? props.value.toLocaleString() : props.value,
)
</script>

<style scoped>
.stat-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px 18px;
}
.stat-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  min-height: 30px;
}
.stat-line.below {
  flex-direction: column;
  align-items: flex-start;
  justify-content: flex-start;
  gap: 1px;
}
</style>
