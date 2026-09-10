<template>
  <span class="logo" :style="logoStyle">
    <el-icon :size="iconSize"><component :is="iconComp" /></el-icon>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Cpu } from '@element-plus/icons-vue'

const props = withDefaults(
  defineProps<{ name?: string; color?: string; size?: number }>(),
  { name: '', color: '', size: 22 },
)

const letter = computed(() => (props.name || '?').charAt(0).toUpperCase())
const iconSize = computed(() => Math.max(12, Math.round(props.size * 0.6)))
const iconComp = Cpu

const logoStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  fontSize: `${props.size * 0.5}px`,
  background: props.color || 'var(--surface-3)',
}))
</script>

<style scoped>
.logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 5px;
  flex-shrink: 0;
  color: var(--ink-2);
  font-weight: 700;
  overflow: hidden;
}
</style>