<template>
  <aside class="topics">
    <div class="topics-head">
      <span class="topics-title">会话</span>
      <button type="button" class="btn bare icon" title="新建对话" aria-label="新建对话" @click="emit('new')">
        <el-icon :size="15"><Plus /></el-icon>
      </button>
    </div>

    <el-scrollbar class="topics-body">
      <p v-if="topics.length === 0" class="topics-empty">暂无会话</p>
      <ul v-else class="topics-list">
        <li v-for="t in topics" :key="t.id">
          <div class="topic-row" :class="{ active: activeId === t.id }">
            <input
              v-if="editingId === t.id"
              ref="editInput"
              v-model="editingTitle"
              class="topic-input"
              @click.stop
              @blur="onBlur(t)"
              @keydown.enter.prevent="commitRename(t)"
              @keydown.esc.prevent="cancelRename(true)"
            />
            <button v-else type="button" class="topic-name" @click="emit('select', t.id)">
              {{ t.title || '新对话' }}
            </button>
            <button
              type="button"
              class="topic-op"
              title="重命名"
              aria-label="重命名"
              @click.stop="beginRename(t)"
            >
              <el-icon :size="13"><EditPen /></el-icon>
            </button>
            <button
              type="button"
              class="topic-op"
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
import { nextTick, ref } from 'vue'
import { Plus, EditPen, Delete } from '@element-plus/icons-vue'
import type { ChatTopic } from '@/api/chat'

defineProps<{ topics: ChatTopic[]; activeId: string | null }>()

const emit = defineEmits<{
  select: [id: string]
  new: []
  rename: [topic: ChatTopic, title: string]
  delete: [topic: ChatTopic]
}>()

const editingId = ref<string | null>(null)
const editingTitle = ref('')
const editInput = ref<HTMLInputElement | HTMLInputElement[] | null>(null)
/** Esc 取消时会先触发 blur，用这个标记吞掉那次提交。 */
const ignoreBlur = ref(false)

async function beginRename(topic: ChatTopic): Promise<void> {
  editingId.value = topic.id
  editingTitle.value = topic.title || '新对话'
  await nextTick()
  const el = Array.isArray(editInput.value) ? editInput.value[0] : editInput.value
  el?.focus()
  el?.select()
}

function cancelRename(fromEsc = false): void {
  if (fromEsc) ignoreBlur.value = true
  editingId.value = null
  editingTitle.value = ''
}

function onBlur(topic: ChatTopic): void {
  if (ignoreBlur.value) {
    ignoreBlur.value = false
    return
  }
  commitRename(topic)
}

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
.topics {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  border-right: 1px solid var(--line);
  background: var(--surface-2);
}
.topics-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  padding: 10px 8px 10px 12px;
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}
.topics-title {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--ink-3);
}
.topics-body { flex: 1; min-height: 0; padding: 0 6px; }
.topics-empty {
  margin: 0;
  padding: 22px 8px;
  text-align: center;
  font-size: 12px;
  color: var(--ink-4);
}
.topics-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }

.topic-row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 6px 6px 6px 9px;
  border-radius: var(--r-sm);
  font-size: 14px;
  color: var(--ink-2);
  transition: background 0.12s, color 0.12s;
}
.topic-row:hover { background: var(--surface-3); color: var(--ink); }
.topic-row.active { background: var(--accent-bg); color: var(--accent-ink); }
html.dark .topic-row.active { background: var(--surface-3); color: var(--ink); }

.topic-name {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  text-align: left;
  padding: 0;
  font: inherit;
  color: inherit;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.topic-input {
  flex: 1;
  min-width: 0;
  height: 24px;
  padding: 0 6px;
  font-size: 14px;
  color: var(--ink);
  background: var(--surface);
  border: 1px solid var(--accent);
  border-radius: 4px;
  outline: none;
}

.topic-op {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  padding: 2px;
  border: none;
  background: transparent;
  color: var(--ink-4);
  opacity: 0;
  cursor: pointer;
  transition: opacity 0.12s, color 0.12s;
}
.topic-row:hover .topic-op { opacity: 1; }
.topic-op:hover { color: var(--ink); }
</style>
