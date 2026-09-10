<template>
  <div class="page-header">
    <div>
      <div class="page-title">{{ title }}</div>
      <div v-if="sub" class="page-sub">{{ sub }}</div>
    </div>
    <div v-if="$slots.actions" class="page-actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ title: string; sub?: string }>()
</script>