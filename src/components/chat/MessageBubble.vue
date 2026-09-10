<template>
  <div class="msg" :class="{ mine: isUser }">
    <div class="avatar" :class="{ mine: isUser }" aria-hidden="true">
      <el-icon :size="15"><component :is="isUser ? User : Cpu" /></el-icon>
    </div>

    <div class="msg-col" :class="{ mine: isUser }">
      <div class="msg-meta" :class="{ mine: isUser }">
        <span class="msg-who">{{ isUser ? '我' : '助手' }}</span>
        <span v-if="time" :title="new Date(msg.createdAt).toLocaleString()">{{ time }}</span>
      </div>

      <div v-if="isError" class="err-callout">
        <div class="err-head">
          <el-icon :size="14" class="err-icon"><CircleClose /></el-icon>
          <span class="err-title">生成失败</span>
          <button
            v-if="msg.content.length > 60"
            type="button"
            class="err-toggle"
            @click="expanded = !expanded"
          >
            {{ expanded ? '收起' : '查看详情' }}
          </button>
        </div>
        <div class="err-body" :class="{ collapsed: !expanded && msg.content.length > 60 }">{{ msg.content }}</div>
      </div>

      <div v-else class="bubble" :class="{ mine: isUser }">{{ body }}</div>

      <div v-if="showBranch || showRegen" class="msg-ops">
        <template v-if="showBranch">
          <button
            type="button"
            class="op"
            :disabled="msg.siblingIndex <= 0 || busy"
            title="上一分支"
            aria-label="上一分支"
            @click="emit('switchSibling', msg, -1)"
          >
            <el-icon :size="13"><ArrowLeft /></el-icon>
          </button>
          <span class="branch-idx tnum">{{ msg.siblingIndex + 1 }}/{{ msg.siblingCount }}</span>
          <button
            type="button"
            class="op"
            :disabled="msg.siblingIndex >= msg.siblingCount - 1 || busy"
            title="下一分支"
            aria-label="下一分支"
            @click="emit('switchSibling', msg, 1)"
          >
            <el-icon :size="13"><ArrowRight /></el-icon>
          </button>
        </template>
        <button v-if="showRegen" type="button" class="op text" title="重新生成" @click="emit('regenerate', msg)">
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

<style scoped>
.msg { display: flex; width: 100%; gap: 10px; }
.msg.mine { flex-direction: row-reverse; }

.avatar {
  margin-top: 2px;
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--surface-3);
  color: var(--ink-3);
}
.avatar.mine { background: var(--accent-bg); color: var(--accent-ink); }

.msg-col {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  max-width: 76%;
  align-items: flex-start;
}
.msg-col.mine { align-items: flex-end; }

.msg-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 2px;
  font-size: 11px;
  color: var(--ink-4);
}
.msg-meta.mine { flex-direction: row-reverse; }
.msg-who { font-weight: 500; color: var(--ink-3); }

.bubble {
  padding: 9px 14px;
  border-radius: 12px;
  border-bottom-left-radius: 4px;
  font-size: 14px;
  line-height: 1.625;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--line);
  white-space: pre-wrap;
  word-break: break-word;
  /* 用户极短消息（如 "试试"）避免塌成小方块 */
  min-width: 64px;
  min-height: 22px;
}
.bubble.mine {
  background: var(--accent-bg);
  border-color: transparent;
  color: var(--accent-ink);
  border-bottom-left-radius: 12px;
  border-bottom-right-radius: 4px;
}
html.dark .bubble.mine { color: var(--ink); }

/* 错误态：紧凑 callout，而非整段红底 */
.err-callout {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-width: 480px;
  padding: 8px 12px;
  border: 1px solid var(--err-line, rgba(220, 80, 80, 0.35));
  border-left: 3px solid var(--err);
  border-radius: 12px;
  border-bottom-left-radius: 4px;
  background: var(--err-bg, rgba(220, 80, 80, 0.06));
  font-size: 13px;
  line-height: 1.55;
  color: var(--ink-2);
  word-break: break-word;
}
.err-head {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--err);
}
.err-icon { flex-shrink: 0; }
.err-title { font-weight: 500; font-size: 12.5px; }
.err-toggle {
  margin-left: auto;
  padding: 1px 8px;
  border: 1px solid var(--err-line, rgba(220, 80, 80, 0.35));
  border-radius: 999px;
  background: transparent;
  font-size: 11px;
  color: var(--err);
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.err-toggle:hover { background: rgba(220, 80, 80, 0.1); }
.err-body {
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--ink-2);
  font-family: var(--font-mono);
  font-size: 12px;
}
.err-body.collapsed {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.msg-ops { display: flex; align-items: center; gap: 3px; padding: 2px 2px 0; color: var(--ink-4); }
.op {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 7px;
  border: none;
  background: transparent;
  color: inherit;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.op:hover:not(:disabled) { background: var(--surface-3); color: var(--ink); }
.op:disabled { opacity: 0.3; cursor: not-allowed; }
.op.text { font-size: 11.5px; }
.branch-idx { min-width: 34px; text-align: center; font-size: 11px; }
</style>
