<template>
  <div class="chat-root flex flex-1 h-full min-h-0 bg-bg">
    <!-- 会话侧栏：会话列表与新建/重命名/删除入口 -->
    <div
      class="chat-side shrink-0 w-208px overflow-hidden"
      :class="{ 'collapsed w-0': layout.topicListCollapsed }"
    >
      <TopicList
        :topics="topics"
        :active-id="activeId"
        @select="onSelectTopic"
        @new="handleNew"
        @rename="handleRename"
        @delete="deletingTopic = $event"
      />
    </div>

    <!-- 主列：头部、消息列表与输入框 -->
    <div ref="columnEl" class="chat-col flex flex-col flex-1 min-w-0 min-h-0">
      <!-- 头部：侧边栏切换、标题与模型选择器 -->
      <header
        class="chat-head flex items-center justify-between gap-12px shrink-0 px-16px pb-9px border-b border-line bg-surface"
      >
        <div class="head-left flex items-center gap-8px">
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
          <h1 class="chat-title m-0 text-15px font-semibold tracking-[-0.01em]">对话</h1>
        </div>

        <div class="head-right flex items-center gap-6px">
          <!-- 无可用模型时引导去配置 -->
          <template v-if="modelGroups.length === 0">
            <button type="button" class="btn sm" @click="goConfigure">去配置端点</button>
          </template>
          <!-- 模型选择器 -->
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

      <!-- 消息滚动区 -->
      <el-scrollbar class="chat-scroll flex-1 min-h-0">
        <!-- 空态提示 -->
        <div
          v-if="messages.length === 0"
          class="chat-empty flex flex-col items-center justify-center gap-4px h-full text-ink-4"
        >
          <p class="empty-main m-0 text-14px">选择模型后开始对话</p>
          <p class="empty-sub m-0 text-12px text-ink-5">非核心功能，对话无工具支持，可用于测试连通性</p>
        </div>
        <!-- 消息气泡列表 -->
        <div v-else class="chat-list flex flex-col gap-16px w-full max-w-780px my-0 mx-auto">
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

      <!-- 输入框 -->
      <ComposerBar
        v-model:value="draft"
        :busy="busy"
        :disabled="modelGroups.length === 0"
        :column-el="columnEl"
        @send="handleSend"
        @abort="handleAbort"
      />
    </div>

    <!-- 删除会话确认弹窗 -->
    <el-dialog
      v-model="deleteOpen"
      title="删除会话"
      width="400"
      :close-on-click-modal="!deleting"
    >
      <p class="del-text m-0 text-14px text-ink-2 leading-1.6">
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
/**
 * 聊天页面。
 * 职责：提供与 AI 模型的对话界面，包括会话侧栏、消息列表、输入框与模型选择器；
 * 支持新建/重命名/删除会话、流式接收回复、重新生成、切换历史分支与中止生成。
 */
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
// 聊天布局状态（侧边栏折叠等）
const layout = useChatLayoutStore()

// 会话列表
const topics = ref<ChatTopic[]>([])
// 当前会话的消息列表
const messages = ref<BranchMessage[]>([])
// 当前选中的会话 ID
const activeId = ref<string | null>(null)
// 输入框草稿
const draft = ref('')
// 可用模型分组
const modelGroups = ref<ModelOptionGroup[]>([])
// 当前选中的模型 key（endpointId/model）
const selectedKey = ref('')

/** 正在生成的会话 id 集合（后端按 topic 并行，前端须隔离）。 */
const streamingIds = ref<Set<string>>(new Set())
// 当前会话是否正在生成
const busy = computed(() => !!activeId.value && streamingIds.value.has(activeId.value))

// 待删除的会话
const deletingTopic = ref<ChatTopic | null>(null)
// 是否正在删除
const deleting = ref(false)
// 删除弹窗可见性（与 deletingTopic 联动）
const deleteOpen = computed({
  get: () => deletingTopic.value !== null,
  set: (v: boolean) => {
    if (!v && !deleting.value) deletingTopic.value = null
  },
})

// 主列容器元素（供 ComposerBar 计算高度）
const columnEl = ref<HTMLElement | null>(null)

// 消息列表底部锚点（用于滚动到底）
const bottomEl = ref<HTMLElement | null>(null)

// 当前选中的会话对象
const activeTopic = computed(() => topics.value.find((t) => t.id === activeId.value) ?? null)
// 侧边栏切换按钮的 aria 标签
const sidebarToggleLabel = computed(() => (layout.topicListCollapsed ? '显示侧边栏' : '隐藏侧边栏'))

/* ============ 载入 ============ */

/**
 * 加载会话列表。
 */
async function loadTopics(): Promise<void> {
  topics.value = await chatApi.listTopics()
}

/**
 * 加载当前会话的消息列表。
 */
async function loadMessages(): Promise<void> {
  if (!activeId.value) {
    messages.value = []
    return
  }
  messages.value = await chatApi.listMessages(activeId.value)
}

/**
 * 加载可用模型分组，并在未选中时默认选第一个。
 * 首次加载失败时自动重试（后端可能尚未就绪）。
 */
async function loadModels(): Promise<void> {
  modelGroups.value = await listModelGroups()
  if (modelGroups.value.length === 0) {
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 1500))
      modelGroups.value = await listModelGroups()
      if (modelGroups.value.length > 0) break
    }
  }
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
      // 当前会话不存在时回退到第一个
      activeId.value = topics.value[0]?.id ?? null
      await loadMessages()
    }
  },
)

// 切换会话时重新加载消息
watch(activeId, () => void loadMessages())

// 会话的端点或模型变化时同步 selectedKey
watch(
  () => [activeTopic.value?.id, activeTopic.value?.endpointId, activeTopic.value?.model] as const,
  ([id, ep, model]) => {
    if (!id || !ep || !model) return
    selectedKey.value = modelKey(ep, model)
  },
)

// 消息变化时滚动到底部
watch(messages, () => void scrollToBottom())

/**
 * 滚动消息列表到底部。
 */
async function scrollToBottom(): Promise<void> {
  await nextTick()
  const el = bottomEl.value
  if (!el) return
  // 流式过程中用瞬时滚动，避免每来一个 token 都触发平滑动画而卡顿
  el.scrollIntoView({ behavior: busy.value ? 'auto' : 'smooth', block: 'end' })
}

/* ============ 流式事件 ============ */

/**
 * 标记某会话是否正在流式生成。
 * @param topicId 会话 ID
 * @param on 是否正在生成
 */
function markStreaming(topicId: string, on: boolean): void {
  const next = new Set(streamingIds.value)
  if (on) next.add(topicId)
  else next.delete(topicId)
  streamingIds.value = next
}

/**
 * 局部更新某条消息的字段。
 * @param topicId 会话 ID
 * @param messageId 消息 ID
 * @param patch 待合并的字段
 */
function patchMessage(topicId: string, messageId: string, patch: Partial<BranchMessage>): void {
  if (activeId.value !== topicId) return
  messages.value = messages.value.map((m) => (m.id === messageId ? { ...m, ...patch } : m))
}

// 事件监听器卸载函数集合
let unlistens: Array<() => void> = []

/**
 * 全局键盘事件处理：Ctrl+[ 切换侧边栏。
 * @param e 键盘事件
 */
function onKeyDown(e: KeyboardEvent): void {
  if (e.isComposing) return // 忽略输入法组合
  if (!(e.ctrlKey || e.metaKey) || e.key !== '[') return
  e.preventDefault()
  layout.toggleTopicList()
}

/**
 * 组件挂载时加载模型与会话，并注册流式事件监听。
 */
onMounted(async () => {
  await loadModels()
  await loadTopics()
  if (topics.value.length === 0) activeId.value = null
  else {
    activeId.value = topics.value[0]?.id ?? null
    await loadMessages()
  }

  // 注册流式事件监听
  unlistens.push(
    // 接收到增量 chunk：追加到对应消息内容
    chatApi.onChunk((p) => {
      if (activeId.value !== p.topicId) return
      messages.value = messages.value.map((m) =>
        m.id === p.messageId ? { ...m, content: m.content + p.delta, status: 'streaming' } : m,
      )
    }),
    // 生成完成：更新最终内容与状态
    chatApi.onDone((p) => {
      patchMessage(p.topicId, p.messageId, { content: p.content, status: 'success' })
      markStreaming(p.topicId, false)
      void loadTopics()
    }),
    // 生成出错：标记错误状态（已取消视为成功）
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

/**
 * 组件卸载时移除事件监听。
 */
onBeforeUnmount(() => {
  unlistens.forEach((u) => u())
  unlistens = []
  window.removeEventListener('keydown', onKeyDown)
})

/* ============ 交互 ============ */

/**
 * 选择会话。
 * @param id 会话 ID
 */
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
  // 校验当前模型是否仍然可用
  const stillAvailable = modelGroups.value.some(
    (g) => g.id === parsed.endpointId && g.models.includes(parsed.model),
  )
  if (!stillAvailable) {
    ElMessage.error('当前模型已不可用，请重新选择')
    return null
  }
  // 已有会话：若模型变更则同步更新
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
  // 无会话：创建新会话
  const created = await chatApi.createTopic({
    endpointId: parsed.endpointId,
    model: parsed.model,
  })
  await loadTopics()
  activeId.value = created.id
  return created.id
}

/**
 * 新建会话。
 */
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

/**
 * 重命名会话。
 * @param topic 会话对象
 * @param title 新标题
 */
async function handleRename(topic: ChatTopic, title: string): Promise<void> {
  try {
    await chatApi.updateTopic(topic.id, { title })
    await loadTopics()
  } catch (e) {
    ElMessage.error(e instanceof Error ? e.message : '重命名失败')
  }
}

/**
 * 确认删除会话。
 */
async function handleConfirmDelete(): Promise<void> {
  const target = deletingTopic.value
  if (!target) return
  // 预先选定删除后的激活会话
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

/**
 * 发送消息。
 */
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

/**
 * 重新生成指定消息。
 * @param m 待重新生成的消息
 */
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

/**
 * 切换到同级历史分支。
 * @param m 当前消息
 * @param dir 切换方向：-1 上一个 / 1 下一个
 */
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

/**
 * 中止当前会话的生成。
 */
async function handleAbort(): Promise<void> {
  if (!activeId.value || !streamingIds.value.has(activeId.value)) return
  try {
    await chatApi.abort(activeId.value)
  } catch {
    /* ignore */
  }
}

/**
 * 切换模型并同步到当前会话。
 * @param key 模型 key
 */
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
    // 更新失败时回滚选择
    selectedKey.value = prev
    ElMessage.error(e instanceof Error ? e.message : '更新模型失败')
  }
}

/**
 * 跳转到订阅管理页配置端点。
 */
function goConfigure(): void {
  void router.push('/subscriptions')
}
</script>

<style scoped>
/* 侧边栏折叠过渡动画（transition 难以用原子类表达，保留） */
.chat-side {
  transition: width 0.2s ease-in-out;
}

/* 头部：拖动由 AppLayout 顶部的 WindowChrome 统一处理 */
.chat-head {
  padding-top: 9px;
}

/* el-scrollbar 内部 padding（:deep() 穿透选择器，保留） */
.chat-scroll :deep(.el-scrollbar__view) {
  padding: 20px 20px 12px;
}
</style>
