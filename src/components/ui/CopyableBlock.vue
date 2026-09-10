<template>
  <div class="copyable" :class="variant">
    <div class="copy-body">
      <slot>{{ text }}</slot>
    </div>
    <button type="button" class="btn bare sm copy-btn" @click="copy">
      <el-icon v-if="!copied" :size="13"><DocumentCopy /></el-icon>
      <el-icon v-else :size="13"><Check /></el-icon>
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { DocumentCopy, Check } from '@element-plus/icons-vue'

const props = withDefaults(
  defineProps<{ text?: string; variant?: 'block' | 'inline' }>(),
  { text: '', variant: 'block' },
)

const copied = ref(false)

async function copy() {
  try {
    await navigator.clipboard.writeText(props.text)
  } catch {
    /* 剪贴板不可用时忽略 */
  }
  copied.value = true
  setTimeout(() => (copied.value = false), 1500)
}
</script>

<style scoped>
.copyable { position: relative; }
.copyable.block .copy-body {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 10px 12px;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.7;
  color: var(--ink-2);
  overflow-x: auto;
  white-space: pre;
}
.copyable.inline { display: flex; align-items: center; gap: 8px; }
.copyable.inline .copy-body {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 6px 6px 6px 12px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--ink-2);
  flex: 1;
  overflow-x: auto;
  white-space: nowrap;
}
.copy-btn { flex-shrink: 0; }
</style>