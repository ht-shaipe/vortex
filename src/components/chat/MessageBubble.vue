<template>
  <div class="msg flex w-full gap-10px [&.mine]:flex-row-reverse" :class="{ mine: isUser }">
    <div class="avatar mt-2px shrink-0 w-32px h-32px rounded-full grid place-items-center bg-surface-3 text-ink-3 [&.mine]:bg-accent-bg [&.mine]:text-accent-ink" :class="{ mine: isUser }" aria-hidden="true">
      <el-icon :size="15"><component :is="isUser ? User : Cpu" /></el-icon>
    </div>

    <div class="msg-col flex flex-col gap-4px min-w-0 max-w-76% items-start [&.mine]:items-end" :class="{ mine: isUser }">
      <div class="msg-meta flex items-center gap-8px px-2px text-11px text-ink-4 [&.mine]:flex-row-reverse" :class="{ mine: isUser }">
        <span class="msg-who font-medium text-ink-3">{{ isUser ? '我' : '助手' }}</span>
        <span v-if="time" :title="new Date(msg.createdAt).toLocaleString()">{{ time }}</span>
      </div>

      <div v-if="isError" class="flex flex-col gap-4px max-w-480px px-12px py-8px border border-solid rounded-12px rounded-bl-4px text-13px leading-[1.55] text-ink-2 break-words border-[color:var(--err-line,rgba(220,80,80,0.35))] border-l-3px border-l-err bg-err-bg">
        <div class="err-head flex items-center gap-6px text-err">
          <el-icon :size="14" class="err-icon shrink-0"><CircleClose /></el-icon>
          <span class="err-title font-medium text-12.5px">生成失败</span>
          <button
            v-if="msg.content.length > 60"
            type="button"
            class="err-toggle ml-auto px-8px py-1px border border-solid rounded-full bg-transparent text-11px text-err cursor-pointer transition-colors hover:bg-[rgba(220,80,80,0.1)] border-[color:var(--err-line,rgba(220,80,80,0.35))]"
            @click="expanded = !expanded"
          >
            {{ expanded ? '收起' : '查看详情' }}
          </button>
        </div>
        <div class="err-body whitespace-pre-wrap break-words text-ink-2 font-mono text-12px [&.collapsed]:line-clamp-2" :class="{ collapsed: !expanded && msg.content.length > 60 }">{{ msg.content }}</div>
      </div>

      <div v-else-if="isUser" class="bubble px-14px py-9px rounded-12px rounded-bl-4px text-14px leading-[1.625] text-ink bg-surface border border-solid border-line whitespace-pre-wrap break-words min-w-64px min-h-22px [&.mine]:bg-accent-bg [&.mine]:border-transparent [&.mine]:text-accent-ink [&.mine]:rounded-bl-12px [&.mine]:rounded-br-4px dark:[&.mine]:text-ink" :class="{ mine: isUser }">{{ body }}</div>
      <div v-else class="bubble markdown-body px-14px py-9px rounded-12px rounded-bl-4px text-14px leading-[1.625] text-ink bg-surface border border-solid border-line break-words min-w-64px min-h-22px [&.mine]:bg-accent-bg [&.mine]:border-transparent [&.mine]:text-accent-ink [&.mine]:rounded-bl-12px [&.mine]:rounded-br-4px dark:[&.mine]:text-ink" :class="{ mine: isUser }" v-html="renderedBody"></div>

      <div v-if="showBranch || showRegen" class="msg-ops flex items-center gap-3px pt-2px px-2px text-ink-4">
        <template v-if="showBranch">
          <button
            type="button"
            class="op inline-flex items-center gap-4px px-7px py-3px border-none bg-transparent text-inherit rounded-6px cursor-pointer transition-colors [&:hover:not(:disabled)]:bg-surface-3 [&:hover:not(:disabled)]:text-ink disabled:opacity-30 disabled:cursor-not-allowed"
            :disabled="msg.siblingIndex <= 0 || busy"
            title="上一分支"
            aria-label="上一分支"
            @click="emit('switchSibling', msg, -1)"
          >
            <el-icon :size="13"><ArrowLeft /></el-icon>
          </button>
          <span class="branch-idx tnum min-w-34px text-center text-11px">{{ msg.siblingIndex + 1 }}/{{ msg.siblingCount }}</span>
          <button
            type="button"
            class="op inline-flex items-center gap-4px px-7px py-3px border-none bg-transparent text-inherit rounded-6px cursor-pointer transition-colors [&:hover:not(:disabled)]:bg-surface-3 [&:hover:not(:disabled)]:text-ink disabled:opacity-30 disabled:cursor-not-allowed"
            :disabled="msg.siblingIndex >= msg.siblingCount - 1 || busy"
            title="下一分支"
            aria-label="下一分支"
            @click="emit('switchSibling', msg, 1)"
          >
            <el-icon :size="13"><ArrowRight /></el-icon>
          </button>
        </template>
        <button v-if="showRegen" type="button" class="op text inline-flex items-center gap-4px px-7px py-3px border-none bg-transparent text-inherit rounded-6px cursor-pointer transition-colors [&:hover:not(:disabled)]:bg-surface-3 [&:hover:not(:disabled)]:text-ink disabled:opacity-30 disabled:cursor-not-allowed [&.text]:text-11.5px" title="重新生成" @click="emit('regenerate', msg)">
          <el-icon :size="12"><Refresh /></el-icon>重生成
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * MessageBubble.vue — 消息气泡
 * 职责：渲染单条对话消息（用户/助手），支持错误态展示、分支切换与重新生成操作。
 */
import { computed, ref } from 'vue'
import { User, Cpu, ArrowLeft, ArrowRight, Refresh, CircleClose } from '@element-plus/icons-vue'
import type { BranchMessage } from '@/api/chat'
import { renderMarkdown } from '@/lib/markdown'

// Props 定义：msg 为消息数据，busy 标识是否正在生成中
const props = defineProps<{ msg: BranchMessage; busy: boolean }>()
// Emits 定义：regenerate 触发重新生成，switchSibling 切换兄弟分支
const emit = defineEmits<{
  regenerate: [m: BranchMessage]
  switchSibling: [m: BranchMessage, dir: -1 | 1]
}>()

const expanded = ref(false) // 错误详情是否展开

const isUser = computed(() => props.msg.role === 'user') // 是否为用户消息
const isError = computed(() => props.msg.status === 'error') // 是否为错误消息
const showBranch = computed(() => !isUser.value && props.msg.siblingCount > 1) // 是否显示分支切换按钮
const showRegen = computed(
  () => !isUser.value && (props.msg.status === 'success' || props.msg.status === 'error') && !props.busy,
) // 是否显示重新生成按钮

// 消息正文内容（生成中显示省略号）
const body = computed(
  () =>
    props.msg.content ||
    (props.msg.status === 'pending' || props.msg.status === 'streaming' ? '…' : ''),
)

// 助手消息的 Markdown 渲染结果
const renderedBody = computed(() => renderMarkdown(body.value))

// 格式化时间显示（月/日 时:分）
const time = computed(() => {
  const d = new Date(props.msg.createdAt)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleString('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
})
</script>

