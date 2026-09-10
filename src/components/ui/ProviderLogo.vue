<template>
  <span class="logo" :style="logoStyle">
    <el-icon :size="iconSize"><component :is="iconComp" /></el-icon>
  </span>
</template>

<script setup lang="ts">
/**
 * ProviderLogo.vue — 提供商 Logo
 * 职责：以方形色块 + 图标的形式展示提供商标识，支持自定义颜色与尺寸。
 */
import { computed } from 'vue'
import { Cpu } from '@element-plus/icons-vue'

// Props 定义：name 为提供商名称，color 为背景色，size 控制整体尺寸
const props = withDefaults(
  defineProps<{ name?: string; color?: string; size?: number }>(),
  { name: '', color: '', size: 22 },
)

const letter = computed(() => (props.name || '?').charAt(0).toUpperCase()) // 名称首字母（大写）
const iconSize = computed(() => Math.max(12, Math.round(props.size * 0.6))) // 图标尺寸（按比例缩放）
const iconComp = Cpu // 固定使用 Cpu 图标

// Logo 容器的内联样式（尺寸、字号、背景色）
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