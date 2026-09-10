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

      <div class="bubble" :class="{ mine: isUser, bad: isError }">{{ body }}</div>

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
import { computed } from 'vue'
import { User, Cpu, ArrowLeft, ArrowRight, Refresh } from '@element-plus/icons-vue'
import type { BranchMessage } from '@/api/chat'

const props = defineProps<{ msg: BranchMessage; busy: boolean }>()
const emit = defineEmits<{
  regenerate: [m: BranchMessage]
  switchSibling: [m: BranchMessage, dir: -1 | 1]
}>()

const isUser = computed(() => props.msg.role === 'user')
const isError = computed(() => props.msg.status === 'error')
const showBranch = computed(() => !isUser.value && props.msg.siblingCount > 1)
const showRegen = computed(
  () => !isUser.value && (props.msg.status === 'success' || props.msg.status === 'error') && !props.busy,
)

const body = computed(
  () =>
    props.msg.content ||
    (props.msg.status === 'pending' || props.msg.status === 'streaming' ? '…' : ''),
)

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
  padding: 10px 14px;
  border-radius: var(--r-md);
  font-size: 14px;
  line-height: 1.625;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--line);
  white-space: pre-wrap;
  word-break: break-word;
}
.bubble.mine {
  background: var(--accent-bg);
  border-color: var(--accent-line);
  color: var(--accent-ink);
}
html.dark .bubble.mine { color: var(--ink); }
.bubble.bad { border-color: var(--err); color: var(--err); background: var(--err-bg); }

.msg-ops { display: flex; align-items: center; gap: 3px; padding: 0 2px; color: var(--ink-4); }
.op {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 4px;
  border: none;
  background: transparent;
  color: inherit;
  border-radius: 4px;
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.op:hover:not(:disabled) { background: var(--surface-3); color: var(--ink); }
.op:disabled { opacity: 0.3; cursor: not-allowed; }
.op.text { font-size: 11px; padding: 2px 6px; }
.branch-idx { min-width: 34px; text-align: center; font-size: 11px; }
</style>
