<template>
  <div>
    <!-- 页面头部：标题与副标题 -->
    <PageHeader title="统计" sub="端点调用与 Token 用量的多维分析" />

    <!-- 顶部切换标签：端点统计 / 用量统计 -->
    <div class="tabs">
      <button
        v-for="t in TOP_TABS"
        :key="t.key"
        type="button"
        class="tab"
        :class="{ active: tab === t.key }"
        @click="tab = t.key"
      >
        {{ t.label }}
      </button>
    </div>

    <!-- 按选中标签渲染对应统计面板 -->
    <EndpointStatsPanel v-if="tab === 'endpoint'" />
    <UsagePanel v-else />
  </div>
</template>

<script setup lang="ts">
/**
 * 统计页面。
 * 职责：作为统计模块的容器，在「端点统计」与「用量统计」两个子面板间切换，
 * 具体统计逻辑由 EndpointStatsPanel / UsagePanel 子组件实现。
 */
import { ref } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EndpointStatsPanel from '@/components/stats/EndpointStatsPanel.vue'
import UsagePanel from '@/components/stats/UsagePanel.vue'

// 顶部标签定义：端点统计 / 用量统计
const TOP_TABS = [
  { key: 'endpoint', label: '端点统计' },
  { key: 'usage', label: '用量统计' },
] as const

// 标签 key 的联合类型
type TopKey = (typeof TOP_TABS)[number]['key']

// 当前选中的统计面板
const tab = ref<TopKey>('endpoint')
</script>
