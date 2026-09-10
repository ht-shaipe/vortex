<template>
  <div class="copyable" :class="variant">
    <div class="copy-body">
      <template v-if="variant === 'block' && lang">
        <!-- eslint-disable-next-line vue/no-v-html -->
        <span v-html="highlighted" />
      </template>
      <slot v-else>{{ text }}</slot>
    </div>
    <button type="button" class="btn bare sm copy-btn" @click="copy">
      <el-icon v-if="!copied" :size="13"><DocumentCopy /></el-icon>
      <el-icon v-else :size="13"><Check /></el-icon>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * CopyableBlock.vue — 可复制文本块
 * 职责：展示文本内容并附带复制按钮，支持代码高亮（block 模式）与内联（inline）两种变体。
 */
import { computed, ref } from 'vue'
import { DocumentCopy, Check } from '@element-plus/icons-vue'
import hljs from 'highlight.js/lib/core'
import python from 'highlight.js/lib/languages/python'
import bash from 'highlight.js/lib/languages/bash'
import json from 'highlight.js/lib/languages/json'
import plaintext from 'highlight.js/lib/languages/plaintext'

// 注册 highlight.js 支持的语言
hljs.registerLanguage('python', python)
hljs.registerLanguage('bash', bash)
hljs.registerLanguage('json', json)
hljs.registerLanguage('plaintext', plaintext)

// Props 定义：text 为文本内容，variant 控制展示变体，lang 指定高亮语言
const props = withDefaults(
  defineProps<{ text?: string; variant?: 'block' | 'inline'; lang?: string }>(),
  { text: '', variant: 'block', lang: '' },
)

const copied = ref(false) // 是否已复制（用于切换图标）

// 高亮后的 HTML 字符串（无语言或无文本时回退为原文）
const highlighted = computed(() => {
  if (!props.lang || !props.text) return props.text
  try {
    return hljs.highlight(props.text, { language: props.lang }).value
  } catch {
    return props.text // 高亮失败时回退为纯文本
  }
})

/** 复制文本到剪贴板，并短暂显示已复制状态。 */
async function copy() {
  try {
    await navigator.clipboard.writeText(props.text)
  } catch {
    /* 剪贴板不可用时忽略 */
  }
  copied.value = true
  setTimeout(() => (copied.value = false), 1500) // 1.5 秒后恢复图标
}
</script>


<style scoped>
.copyable { position: relative; }
.copyable.block .copy-body {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 10px 36px 10px 12px;
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--ink-2);
  overflow-x: auto;
  white-space: pre;
}
.copyable.block .copy-btn {
  position: absolute;
  top: 6px;
  right: 6px;
  z-index: 1;
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
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.copy-btn { flex-shrink: 0; }
</style>

<style>
/* highlight.js 主题 — 用 CSS 变量适配明暗 */
.hljs { color: var(--ink-2); background: transparent; }
.hljs-keyword, .hljs-selector-tag, .hljs-built_in { color: oklch(0.60 0.18 280); }
.hljs-string, .hljs-attr, .hljs-template-string { color: oklch(0.60 0.13 150); }
.hljs-comment, .hljs-quote { color: var(--ink-4); font-style: italic; }
.hljs-number, .hljs-literal { color: oklch(0.60 0.15 60); }
.hljs-title, .hljs-section, .hljs-name { color: oklch(0.55 0.15 250); }
.hljs-variable, .hljs-template-variable { color: var(--ink); }
.hljs-type, .hljs-class .hljs-title { color: oklch(0.60 0.15 200); }
.hljs-meta, .hljs-tag { color: var(--ink-3); }
.hljs-symbol, .hljs-bullet, .hljs-link { color: oklch(0.60 0.15 0); }
.hljs-emphasis { font-style: italic; }
.hljs-strong { font-weight: 700; }
</style>
