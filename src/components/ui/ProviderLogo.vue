<template>
  <span class="logo inline-flex items-center justify-center rounded-5px shrink-0 text-ink-2 font-bold overflow-hidden" :style="logoStyle">
    <img v-if="logoUrl" :src="logoUrl" class="logo-img w-full h-full object-contain block" alt="" />
    <el-icon v-else :size="iconSize"><component :is="iconComp" /></el-icon>
  </span>
</template>

<script setup lang="ts">
/**
 * ProviderLogo.vue — 提供商 Logo
 * 职责：优先展示已识别厂商的 SVG 图标；未命中时回退为方形色块 + 通用图标。
 */
import { computed } from 'vue'
import { Cpu } from '@element-plus/icons-vue'
import { resolveVendorLogo } from './vendorLogos'

// Props 定义：name 为提供商标识，hint 为辅助识别线索（连接名/baseUrl 等），color 为回退色块背景色，size 控制整体尺寸
const props = withDefaults(
  defineProps<{ name?: string; hint?: string; color?: string; size?: number }>(),
  { name: '', hint: '', color: '', size: 22 },
)

const iconSize = computed(() => Math.max(12, Math.round(props.size * 0.6))) // 图标尺寸（按比例缩放）
const iconComp = Cpu // 回退使用的通用图标

// 厂商 Logo 解析结果（命中时为 SVG URL，未命中为 null）
const logoUrl = computed(() => resolveVendorLogo(props.name, props.hint))

// Logo 容器的内联样式（尺寸、背景色；命中厂商图标时透明背景）
const logoStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  fontSize: `${props.size * 0.5}px`,
  background: logoUrl.value ? 'transparent' : (props.color || 'var(--surface-3)'),
}))
</script>


