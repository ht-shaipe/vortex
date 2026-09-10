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
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    label: string
    value: number | string
    /** true 时辅助提示在数值下方（垂直堆叠）；默认在右侧（水平）。 */
    hintBelow?: boolean
  }>(),
  { hintBelow: false },
)

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
