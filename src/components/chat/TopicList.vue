<template>
  <aside class="topics flex flex-col h-full w-full border-r border-line bg-surface-2">
    <div class="topics-head flex items-center justify-between shrink-0 pt-14px pr-10px pb-12px pl-16px border-b border-line">
      <span class="topics-title text-13px font-semibold tracking-0.03em text-ink-2">会话</span>
      <button type="button" class="btn bare icon topics-new w-24px h-24px rounded-6px text-ink-3 transition" title="新建对话" aria-label="新建对话" @click="emit('new')">
        <el-icon :size="15"><Plus /></el-icon>
      </button>
    </div>

    <el-scrollbar class="topics-body flex-1 min-h-0 pt-6px px-6px pb-8px">
      <p v-if="topics.length === 0" class="topics-empty m-0 pt-28px px-8px text-center text-12.5px text-ink-4">暂无会话</p>
      <ul v-else class="topics-list list-none m-0 p-0 flex flex-col gap-1px">
        <li v-for="t in topics" :key="t.id">
          <div class="topic-row relative flex items-center gap-2px pt-7px pr-8px pb-7px pl-10px rounded-8px text-13.5px text-ink-2 cursor-pointer transition" :class="{ active: activeId === t.id }">
            <span class="topic-indicator" />
            <input
              v-if="editingId === t.id"
              ref="editInput"
              v-model="editingTitle"
              class="topic-input flex-1 min-w-0 h-26px px-8px text-13.5px text-ink bg-surface border border-accent rounded-6px outline-none"
              @click.stop
              @blur="onBlur(t)"
              @keydown.enter.prevent="commitRename(t)"
              @keydown.esc.prevent="cancelRename(true)"
            />
            <button v-else type="button" class="topic-name flex-1 min-w-0 text-left overflow-hidden text-ellipsis whitespace-nowrap" @click="emit('select', t.id)">
              {{ t.title || '新对话' }}
            </button>
            <button
              type="button"
              class="topic-op shrink-0 inline-flex items-center justify-center w-22px h-22px p-0 border-none rounded-5px bg-transparent text-ink-4 opacity-0 cursor-pointer transition"
              title="重命名"
              aria-label="重命名"
              @click.stop="beginRename(t)"
            >
              <el-icon :size="13"><EditPen /></el-icon>
            </button>
            <button
              type="button"
              class="topic-op shrink-0 inline-flex items-center justify-center w-22px h-22px p-0 border-none rounded-5px bg-transparent text-ink-4 opacity-0 cursor-pointer transition"
              title="删除"
              aria-label="删除"
              @click.stop="emit('delete', t)"
            >
              <el-icon :size="13"><Delete /></el-icon>
            </button>
          </div>
        </li>
      </ul>
    </el-scrollbar>
  </aside>
</template>

<script setup lang="ts">
/**
 * TopicList.vue — 话题列表
 * 职责：展示对话会话列表，支持新建、选择、重命名（内联编辑）与删除操作。
 */
import { nextTick, ref } from 'vue'
import { Plus, EditPen, Delete } from '@element-plus/icons-vue'
import type { ChatTopic } from '@/api/chat'

// Props 定义：topics 为会话列表，activeId 为当前激活会话 ID
defineProps<{ topics: ChatTopic[]; activeId: string | null }>()

// Emits 定义：select 选中会话，new 新建会话，rename 重命名会话，delete 删除会话
const emit = defineEmits<{
  select: [id: string]
  new: []
  rename: [topic: ChatTopic, title: string]
  delete: [topic: ChatTopic]
}>()

const editingId = ref<string | null>(null) // 正在编辑的会话 ID
const editingTitle = ref('') // 编辑中的标题文本
const editInput = ref<HTMLInputElement | HTMLInputElement[] | null>(null) // 编辑输入框引用
/** Esc 取消时会先触发 blur，用这个标记吞掉那次提交。 */
const ignoreBlur = ref(false)

/** 开始重命名：进入编辑态并聚焦选中输入框文本。 */
async function beginRename(topic: ChatTopic): Promise<void> {
  editingId.value = topic.id
  editingTitle.value = topic.title || '新对话'
  await nextTick()
  const el = Array.isArray(editInput.value) ? editInput.value[0] : editInput.value
  el?.focus()
  el?.select()
}

/** 取消重命名，退出编辑态。 */
function cancelRename(fromEsc = false): void {
  if (fromEsc) ignoreBlur.value = true
  editingId.value = null
  editingTitle.value = ''
}

/** 输入框失焦处理：若非 Esc 取消则提交重命名。 */
function onBlur(topic: ChatTopic): void {
  if (ignoreBlur.value) {
    ignoreBlur.value = false
    return
  }
  commitRename(topic)
}

/** 提交重命名：标题非空且变化时触发 rename 事件。 */
function commitRename(topic: ChatTopic): void {
  const next = editingTitle.value.trim()
  const current = topic.title || '新对话'
  if (!next || next === current) {
    cancelRename()
    return
  }
  cancelRename()
  emit('rename', topic, next)
}
</script>

<style scoped>
.topics-head { gap: var(--gap-sm); }
.topics-new:hover {
  background: var(--surface-3);
  color: var(--ink);
}

.topic-row:hover { background: var(--surface-3); color: var(--ink); }
.topic-row.active {
  background: var(--accent-bg);
  color: var(--accent-ink);
  font-weight: 500;
}
html.dark .topic-row.active { background: var(--surface-3); color: var(--ink); }

.topic-indicator {
  position: absolute;
  left: 2px;
  top: 50%;
  transform: translateY(-50%) scaleY(0);
  width: 3px;
  height: 16px;
  border-radius: 2px;
  background: var(--accent);
  transition: transform 0.15s ease;
}
.topic-row.active .topic-indicator { transform: translateY(-50%) scaleY(1); }

.topic-name {
  border: none;
  background: transparent;
  padding: 0;
  font: inherit;
  color: inherit;
  cursor: pointer;
}

.topic-row:hover .topic-op,
.topic-row.active .topic-op { opacity: 1; }
.topic-op:hover { color: var(--ink); background: var(--surface-3); }
html.dark .topic-op:hover { background: var(--surface-2); }
</style>
