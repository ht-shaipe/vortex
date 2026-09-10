<template>
  <span class="pill" :class="tone">
    <span v-if="dot" class="dot" />
    <slot>{{ label }}</slot>
  </span>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    tone?: 'ok' | 'warn' | 'err' | 'neutral'
    label?: string
    dot?: boolean
  }>(),
  { tone: 'neutral', dot: true, label: '' },
)
</script>