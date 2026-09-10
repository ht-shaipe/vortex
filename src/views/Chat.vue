<template>
  <div class="chat-root">
    <!-- 会话侧栏 -->
    <div class="chat-side" :class="{ collapsed: layout.topicListCollapsed }">
      <TopicList
        :topics="topics"
        :active-id="activeId"
        @select="onSelectTopic"
        @new="handleNew"
        @rename="handleRename"
        @delete="deletingTopic = $event"
      />
    </div>

    <!-- 主列 -->
    <div ref="columnEl" class="chat-col">
      <header class="chat-head">
        <div class="head-left">
          <el-tooltip :content="`${sidebarToggleLabel} (Ctrl+[)`" placement="bottom">
            <button
              type="button"
              class="btn bare icon"
              :aria-label="sidebarToggleLabel"
              :aria-pressed="!layout.topicListCollapsed"
              @click="layout.toggleTopicList()"
            >
              <el-icon :size="15"><Fold /></el-icon>
            </button>
          </el-tooltip>
          <h1 class="chat-title">对话</h1>
        </div>

        <div class="head-right">
          <template v-if="modelGroups.length === 0">
            <button type="button" class="btn sm" @click="goConfigure">去配置端点</button>
          </template>
          <ModelSelector
            v-else
            :value="selectedKey"
            :groups="modelGroups"
            configure-text="去配置端点"
            @update:value="onModelChange"
            @configure="goConfigure"
          />
        </div>
      </header>

      <el-scrollbar class="chat-scroll">
        <div v-if="messages.length === 0" class="chat-empty">
          <p class="empty-main">选择模型后开始对话</p>
          <p class="empty-sub">非核心功能，对话无工具支持，可用于测试连通性</p>
        </div>
        <div v-else class="chat-list">
          <MessageBubble
            v-for="m in messages"
            :key="m.id"
            :msg="m"
            :busy="busy"
            @regenerate="handleRegenerate"
            @switch-sibling="handleSwitchSibling"
          />
          <div ref="bottomEl" />
        </div>
      </el-scrollbar>

      <ComposerBar
        v-model:value="draft"
        :busy="busy"
        :disabled="modelGroups.length === 0"
        :column-el="columnEl"
        @send="handleSend"
        @abort="handleAbort"
      />
    </div>

    <!-- 删除会话 -->
    <el-dialog
      v-model="deleteOpen"
      title="删除会话"
      width="400"
      :close-on-click-modal="!deleting"
    >
      <p class="del-text">
        确定删除「{{ deletingTopic?.title || '新对话' }}」吗？该会话下的消息会一并删除，且无法恢复。
      </p>
      <template #footer>
        <button type="button" class="btn" :disabled="deleting" @click="deleteOpen = false">取消</button>
        <button type="button" class="btn danger" :disabled="deleting" @click="handleConfirmDelete">
          {{ deleting ? '删除中…' : '确认删除' }}
        </button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { Fold } from '@element-plus/icons-vue'
import TopicList from '@/components/chat/TopicList.vue'
import MessageBubble from '@/components/chat/MessageBubble.vue'
import ComposerBar from '@/components/chat/ComposerBar.vue'
import ModelSelector from '@/components/chat/ModelSelector.vue'
import { useChatLayoutStore } from '@/stores/chatLayout'
import {
  chatApi,
  listModelGroups,
  modelKey,
  parseModelKey,
  type BranchMessage,
  type ChatTopic,
  type ModelOptionGroup,
} from '@/api/chat'

const router = useRouter()
const layout = useChatLayoutStore()

const topics = ref<ChatTopic[]>([])
const messages = ref<BranchMessage[]>([])
const activeId = ref<string | null>(null)
const draft = ref('')
const modelGroups = ref<ModelOptionGroup[]>([])
const selectedKey = ref('')

/** 正在生成的会话 id 集合（后端按 topic 并行，前端须隔离）。 */
const streamingIds = ref<Set<string>>(new Set())
const busy = computed(() => !!activeId.value && streamingIds.value.has(activeId.value))

const deletingTopic = ref<ChatTopic | null>(null)
const deleting = ref(false)
const deleteOpen = computed({
  get: () => deletingTopic.value !== null,
  set: (v: boolean) => {
    if (!v && !deleting.value) deletingTopic.value = null
  },
})

const columnEl = ref<HTMLElement | null>(null)

const bottomEl = ref<HTMLElement | null>(null)

const activeTopic = computed(() => topics.value.find((t) => t.id === activeId.value) ?? null)
const sidebarToggleLabel = computed(() => (layout.topicListCollapsed ? '显示侧边栏' : '隐藏侧边栏'))

/* ============ 载入 ============ */

async function loadTopics(): Promise<void> {
  topics.value = await chatApi.listTopics()
}

async function loadMessages(): Promise<void> {
  if (!activeId.value) {
    messages.value = []
    return
  }
  messages.value = await chatApi.listMessages(activeId.value)
}

async function loadModels(): Promise<void> {
  modelGroups.value = await listModelGroups()
  if (!selectedKey.value) {
    const first = modelGroups.value[0]
    if (first?.models[0]) selectedKey.value = modelKey(first.id, first.models[0])
  }
}

/** 无会话且已有选中模型时自动置空，避免沿用上一个会话的模型。 */
watch(
  () => topics.value.length,
  async (n) => {
    if (n === 0) activeId.value = null
    else if (!activeId.value || !topics.value.some((t) => t.id === activeId.value)) {
      activeId.value = topics.value[0]?.id ?? null
      await loadMessages()
    }
  },
)

watch(activeId, () => void loadMessages())

watch(
  () => [activeTopic.value?.id, activeTopic.value?.endpointId, activeTopic.value?.model] as const,
  ([id, ep, model]) => {
    if (!id || !ep || !model) return
    selectedKey.value = modelKey(ep, model)
  },
)

watch(messages, () => void scrollToBottom())

async function scrollToBottom(): Promise<void> {
  await nextTick()
  const el = bottomEl.value
  if (!el) return
  // 流式过程中用瞬时滚动，避免每来一个 token 都触发平滑动画而卡顿
  el.scrollIntoView({ behavior: busy.value ? 'auto' : 'smooth', block: 'end' })
}

/* ============ 流式事件 ============ */

function markStreaming(topicId: string, on: boolean): void {
  const next = new Set(streamingIds.value)
  if (on) next.add(topicId)
  else next.delete(topicId)
  streamingIds.value = next
}

function patchMessage(topicId: string, messageId: string, patch: Partial<BranchMessage>): void {
  if (activeId.value !== topicId) return
  messages.value = messages.value.map((m) => (m.id === messageId ? { ...m, ...patch } : m))
}

let unlistens: Array<() => void> = []

function onKeyDown(e: KeyboardEvent): void {
  if (e.isComposing) return
  if (!(e.ctrlKey || e.metaKey) || e.key !== '[') return
  e.preventDefault()
  layout.toggleTopicList()
}

onMounted(async () => {
  await loadModels()
  await loadTopics()
  if (topics.value.length === 0) activeId.value = null
  else {
    activeId.value = topics.value[0]?.id ?? null
    await loadMessages()
  }

  unlistens.push(
    chatApi.onChunk((p) => {
      if (activeId.value !== p.topicId) return
      messages.value = messages.value.map((m) =>
        m.id === p.messageId ? { ...m, content: m.content + p.delta, status: 'streaming' } : m,
      )
    }),
    chatApi.onDone((p) => {
      patchMessage(p.topicId, p.messageId, { content: p.content, status: 'success' })
      markStreaming(p.topicId, false)
      void loadTopics()
    }),
    chatApi.onError((p) => {
      if (p.error.includes('已取消')) {
        patchMessage(p.topicId, p.messageId, { status: 'success' })
      } else {
        patchMessage(p.topicId, p.messageId, { content: p.error, status: 'error' })
        ElMessage.error(p.error || '生成失败')
      }
      markStreaming(p.topicId, false)
      void loadTopics()
    }),
  )

  window.addEventListener('keydown', onKeyDown)
})

onBeforeUnmount(() => {
  unlistens.forEach((u) => u())
  unlistens = []
  window.removeEventListener('keydown', onKeyDown)
})

/* ============ 交互 ============ */

function onSelectTopic(id: string): void {
  if (id === activeId.value) return
  activeId.value = id
  void loadMessages()
}

/** 发送前确保有会话：无则按当前选中模型创建；有则同步模型变更。 */
async function ensureTopic(): Promise<string | null> {
  const parsed = parseModelKey(selectedKey.value)
  if (!parsed) {
    ElMessage.error('请先选择模型')
    return null
  }
  const stillAvailable = modelGroups.value.some(
    (g) => g.id === parsed.endpointId && g.models.includes(parsed.model),
  )
  if (!stillAvailable) {
    ElMessage.error('当前模型已不可用，请重新选择')
    return null
  }
  if (activeId.value) {
    const t = topics.value.find((x) => x.id === activeId.value)
    if (t && (t.endpointId !== parsed.endpointId || t.model !== parsed.model)) {
      await chatApi.updateTopic(activeId.value, {
        endpointId: parsed.endpointId,
        model: parsed.model,
      })
      await loadTopics()
    }
    return activeId.value
  }
  const created = await chatApi.createTopic({
    endpointId: parsed.endpointId,
    model: parsed.model,
  })
  await loadTopics()
  activeId.value = created.id
  return created.id
}

async function handleNew(): Promise<void> {
  const parsed = parseModelKey(selectedKey.value)
  if (!parsed) {
    ElMessage.error('请先选择模型（需先在订阅管理配置提供商）')
    return
  }
  try {
    const created = await chatApi.createTopic({
      endpointId: parsed.endpointId,
      model: parsed.model,
    })
    await loadTopics()
    activeId.value = created.id
    messages.value = []
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '创建失败')
  }
}

async function handleRename(topic: ChatTopic, title: string): Promise<void> {
  try {
    await chatApi.updateTopic(topic.id, { title })
    await loadTopics()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '重命名失败')
  }
}

async function handleConfirmDelete(): Promise<void> {
  const target = deletingTopic.value
  if (!target) return
  const nextActiveId = topics.value.find((t) => t.id !== target.id)?.id ?? null
  deleting.value = true
  try {
    await chatApi.deleteTopic(target.id)
    if (activeId.value === target.id) {
      activeId.value = nextActiveId
      messages.value = []
      if (nextActiveId) await loadMessages()
    }
    await loadTopics()
    deletingTopic.value = null
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '删除失败')
  } finally {
    deleting.value = false
  }
}

async function handleSend(): Promise<void> {
  const text = draft.value.trim()
  if (!text || busy.value) return
  if (modelGroups.value.length === 0) {
    ElMessage.error('没有可用模型，请先在订阅管理配置提供商')
    return
  }
  try {
    const topicId = await ensureTopic()
    if (!topicId) return
    if (streamingIds.value.has(topicId)) {
      ElMessage.error('该会话正在生成中，请稍候')
      return
    }
    markStreaming(topicId, true)
    draft.value = ''
    try {
      const res = await chatApi.send(topicId, text)
      const msgs = await chatApi.listMessages(topicId)
      if (activeId.value === topicId) {
        messages.value = msgs
        // 竞态兜底：若列表尚未含 pending 助手消息，本地补一条
        if (!msgs.some((m) => m.id === res.assistantMessageId)) {
          const ts = new Date().toISOString()
          messages.value = [
            ...msgs,
            {
              id: res.assistantMessageId,
              topicId,
              parentId: res.userMessageId,
              role: 'assistant',
              content: '',
              status: 'pending',
              createdAt: ts,
              updatedAt: ts,
              siblingIndex: 0,
              siblingCount: 1,
              siblingIds: [res.assistantMessageId],
            },
          ]
        }
      }
    } catch (e) {
      markStreaming(topicId, false)
      ElMessage.error(e instanceof Error ? e.message : '发送失败')
    }
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '发送失败')
  }
}

async function handleRegenerate(m: BranchMessage): Promise<void> {
  if (!activeId.value || busy.value) return
  const topicId = activeId.value
  markStreaming(topicId, true)
  try {
    await chatApi.regenerate(topicId, m.id)
    messages.value = await chatApi.listMessages(topicId)
  } catch (e) {
    markStreaming(topicId, false)
    ElMessage.error(e instanceof Error ? e.message : '重生成失败')
  }
}

async function handleSwitchSibling(m: BranchMessage, dir: -1 | 1): Promise<void> {
  if (!activeId.value || busy.value) return
  const topicId = activeId.value
  const next = m.siblingIds[m.siblingIndex + dir]
  if (!next) return
  try {
    await chatApi.setActiveNode(topicId, next)
    messages.value = await chatApi.listMessages(topicId)
    await loadTopics()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '切换分支失败')
  }
}

async function handleAbort(): Promise<void> {
  if (!activeId.value || !streamingIds.value.has(activeId.value)) return
  try {
    await chatApi.abort(activeId.value)
  } catch {
    /* ignore */
  }
}

async function onModelChange(key: string): Promise<void> {
  const prev = selectedKey.value
  selectedKey.value = key
  const parsed = parseModelKey(key)
  if (!parsed || !activeId.value) return
  try {
    await chatApi.updateTopic(activeId.value, {
      endpointId: parsed.endpointId,
      model: parsed.model,
    })
    await loadTopics()
  } catch (e) {
    selectedKey.value = prev
    ElMessage.error(e instanceof Error ? e.message : '更新模型失败')
  }
}

function goConfigure(): void {
  void router.push('/subscriptions')
}
</script>

<style scoped>
.chat-root {
  display: flex;
  flex: 1;
  height: 100%;
  min-height: 0;
  background: var(--bg);
}

.chat-side {
  flex-shrink: 0;
  width: 224px;
  overflow: hidden;
  transition: width 0.2s ease-in-out;
}
.chat-side.collapsed { width: 0; }

.chat-col {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.chat-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-shrink: 0;
  padding: 10px 20px;
  border-bottom: 1px solid var(--line);
  background: var(--surface);
}
.head-left { display: flex; align-items: center; gap: 6px; }
.chat-title { margin: 0; font-size: 18px; font-weight: 400; letter-spacing: -0.01em; }
.head-right { display: flex; align-items: center; gap: 8px; }

.chat-scroll { flex: 1; min-height: 0; }
.chat-scroll :deep(.el-scrollbar__view) { padding: 16px 20px; }

.chat-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 100%;
  color: var(--ink-4);
}
.empty-main { margin: 0; font-size: 14px; }
.empty-sub { margin: 0; font-size: 12px; color: var(--ink-5); }

.chat-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
  max-width: 780px;
  margin: 0 auto;
}

.del-text { margin: 0; font-size: 14px; color: var(--ink-2); line-height: 1.6; }
</style>
