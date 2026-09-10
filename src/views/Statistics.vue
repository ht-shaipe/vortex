<template>
  <div>
    <PageHeader title="统计" sub="端点调用与 Token 用量的多维分析" />

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

    <EndpointStatsPanel v-if="tab === 'endpoint'" />
    <UsagePanel v-else />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EndpointStatsPanel from '@/components/stats/EndpointStatsPanel.vue'
import UsagePanel from '@/components/stats/UsagePanel.vue'

const TOP_TABS = [
  { key: 'endpoint', label: '端点统计' },
  { key: 'usage', label: '用量统计' },
] as const

type TopKey = (typeof TOP_TABS)[number]['key']

const tab = ref<TopKey>('endpoint')
</script>
