/**
 * 对话适配层。
 *
 * 背景：
 * - vortex 后端目前没有 chat 相关接口，但它本身就是 OpenAI 兼容网关
 *   （`POST /v1/chat/completions`，支持 `stream: true`）。
 *
 * 因此这里的实现是：
 * - 会话/消息的**分支树**在前端完整实现并持久化到 localStorage
 *   （parentId 链 + activeNodeId 指向当前分支叶子 + 兄弟节点切换 + 重生成开新分支）；
 * - 发送走 vortex 网关自己的 `/v1/chat/completions` SSE 流。
 *
 * 后端补齐 chat 表后，替换本文件的 CRUD 实现即可，组件层无需改动。
 */
import { listKeys } from './keys'
import { listProviders, type ProviderConnection } from './providers'
import { runtime } from '@/lib/runtime'

const STORAGE_KEY = 'vortex-chat-v1'
const GATEWAY_BASE = 'http://localhost:10168'

/** 消息流式状态 */
export type ChatMessageStatus = 'pending' | 'streaming' | 'success' | 'error'

/** 对话主题（会话） */
export interface ChatTopic {
  id: string // 会话唯一标识
  title: string // 会话标题
  /** 模型所属分组（vortex 下为 `owned_by`，即提供商 id）。 */
  endpointId: string // 端点 ID
  model: string // 使用的模型名
  activeNodeId?: string | null // 当前活动分支叶子节点 ID
  createdAt: string // 创建时间
  updatedAt: string // 更新时间
}

/** 存储态消息节点（树结构，parentId 为 null 表示挂在根上）。 */
interface StoredMessage {
  id: string
  topicId: string
  parentId: string | null
  role: 'user' | 'assistant'
  content: string
  status: ChatMessageStatus
  createdAt: string
  updatedAt: string
}

/** 展示态消息（在活动分支路径上，附带兄弟分支信息）。 */
export interface BranchMessage {
  id: string
  topicId: string
  parentId: string | null
  role: string
  content: string
  status: ChatMessageStatus | string
  createdAt: string
  updatedAt: string
  siblingIndex: number
  siblingCount: number
  siblingIds: string[]
}

/** 创建会话请求参数 */
export interface CreateChatTopicRequest {
  title?: string // 会话标题（可选）
  endpointId: string // 端点 ID
  model: string // 模型名
}

/** 更新会话请求参数 */
export interface UpdateChatTopicRequest {
  title?: string // 新标题（可选）
  endpointId?: string // 新端点 ID（可选）
  model?: string // 新模型名（可选）
}

/** 发送消息响应 */
export interface ChatSendResponse {
  userMessageId: string // 用户消息节点 ID
  assistantMessageId: string // 助手消息节点 ID
}

/** 流式增量块载荷 */
export interface ChatChunkPayload {
  topicId: string // 会话 ID
  messageId: string // 消息节点 ID
  delta: string // 增量文本
}

/** 流式完成载荷 */
export interface ChatDonePayload {
  topicId: string // 会话 ID
  messageId: string // 消息节点 ID
  content: string // 完整文本内容
}

/** 流式错误载荷 */
export interface ChatErrorPayload {
  topicId: string // 会话 ID
  messageId: string // 消息节点 ID
  error: string // 错误信息
}

/* ============ 持久化 ============ */

/** localStorage 持久化结构 */
interface Store {
  topics: ChatTopic[] // 全部会话
  messages: StoredMessage[] // 全部消息节点
}

/** 从 localStorage 读取存储结构，解析失败返回空结构。 */
function load(): Store {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { topics: [], messages: [] }
    const parsed = JSON.parse(raw) as Partial<Store>
    return {
      topics: Array.isArray(parsed.topics) ? parsed.topics : [],
      messages: Array.isArray(parsed.messages) ? parsed.messages : [],
    }
  } catch {
    return { topics: [], messages: [] }
  }
}

/** 将存储结构写入 localStorage，配额溢出时静默忽略。 */
function save(s: Store): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(s))
  } catch {
    /* 配额溢出时忽略，避免打断对话 */
  }
}

/** 生成唯一 ID（时间戳 36 进制 + 随机串）。 */
function uid(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

/** 生成当前时间的 ISO 字符串。 */
function nowIso(): string {
  return new Date().toISOString()
}

/* ============ 事件总线 ============ */

/** 类型别名：事件监听回调 */
type Listener<T> = (p: T) => void
/** 类型别名：取消监听函数 */
type Unlisten = () => void

/**
 * 创建一个简易事件总线（发布/订阅）。
 * @returns 包含 on（订阅）和 emit（发布）方法的对象
 */
function bus<T>() {
  const set = new Set<Listener<T>>()
  return {
    on(cb: Listener<T>): Unlisten {
      set.add(cb)
      return () => set.delete(cb)
    },
    emit(p: T): void {
      for (const cb of [...set]) cb(p)
    },
  }
}

const chunkBus = bus<ChatChunkPayload>()
const doneBus = bus<ChatDonePayload>()
const errorBus = bus<ChatErrorPayload>()

/** 每个 topic 一个 AbortController，实现「按会话并行 + 单独取消」。 */
/** 进行中请求的取消句柄（Web 端为 AbortController，桌面端为 IPC 取消函数）。 */
interface AbortHandle {
  abort: () => void
}

const aborters = new Map<string, AbortHandle>()

/* ============ 树操作 ============ */

/** 获取指定父节点下的子消息（按创建时间排序）。 */
function childrenOf(msgs: StoredMessage[], topicId: string, parentId: string | null): StoredMessage[] {
  return msgs
    .filter((m) => m.topicId === topicId && m.parentId === parentId)
    .sort((a, b) => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.id < b.id ? -1 : 1))
}

/** 从 nodeId 沿「最后一个子节点」链走到叶子，用于分支切换后定位活动叶子。 */
function deepestLeaf(msgs: StoredMessage[], topicId: string, nodeId: string): string {
  let cur = nodeId
  for (;;) {
    const kids = childrenOf(msgs, topicId, cur)
    if (kids.length === 0) return cur
    cur = kids[kids.length - 1].id
  }
}

/** 活动分支路径：从 activeNodeId 回溯到根，再反转成时间顺序。 */
function activePath(msgs: StoredMessage[], topic: ChatTopic): StoredMessage[] {
  const byId = new Map(msgs.map((m) => [m.id, m]))
  const out: StoredMessage[] = []
  let cur = topic.activeNodeId ?? null
  const guard = new Set<string>()
  while (cur) {
    if (guard.has(cur)) break
    guard.add(cur)
    const m = byId.get(cur)
    if (!m || m.topicId !== topic.id) break
    out.push(m)
    cur = m.parentId
  }
  return out.reverse()
}

/** 把存储态消息转换为展示态消息（附带兄弟分支信息）。 */
function toBranchMessages(msgs: StoredMessage[], topic: ChatTopic): BranchMessage[] {
  const path = activePath(msgs, topic)
  return path.map((m) => {
    const siblings = childrenOf(msgs, topic.id, m.parentId)
    const siblingIds = siblings.map((s) => s.id)
    return {
      id: m.id,
      topicId: m.topicId,
      parentId: m.parentId,
      role: m.role,
      content: m.content,
      status: m.status,
      createdAt: m.createdAt,
      updatedAt: m.updatedAt,
      siblingIndex: Math.max(0, siblingIds.indexOf(m.id)),
      siblingCount: siblingIds.length,
      siblingIds,
    }
  })
}

/** 收集以 nodeId 为根的整棵子树 id（删除分支时用）。 */
function subtreeIds(msgs: StoredMessage[], topicId: string, nodeId: string): Set<string> {
  const out = new Set<string>([nodeId])
  const stack = [nodeId]
  while (stack.length) {
    const cur = stack.pop()!
    for (const k of childrenOf(msgs, topicId, cur)) {
      out.add(k.id)
      stack.push(k.id)
    }
  }
  return out
}

/* ============ 流式发送 ============ */

/** 网关鉴权：取第一个可用的 vortex API Key；取不到就不带 Authorization。 */
let cachedKey: string | null | undefined
async function gatewayKey(): Promise<string | null> {
  if (cachedKey !== undefined) return cachedKey
  try {
    const res = await listKeys()
    const k = (res.keys ?? []).find((x) => x.isActive && !x.isBanned) ?? res.keys?.[0]
    cachedKey = k?.key ?? null
  } catch {
    cachedKey = null
  }
  return cachedKey
}

/** SSE 增量解析：按空行切分事件，取 `choices[0].delta.content`；同时检测 error 事件。 */
function extractDelta(json: string): { delta: string; error: string } {
  try {
    const obj = JSON.parse(json) as {
      choices?: { delta?: { content?: string }; message?: { content?: string } }[]
      error?: { message?: string } | string
    }
    const c = obj.choices?.[0]
    const delta = c?.delta?.content ?? c?.message?.content ?? ''
    const error = typeof obj.error === 'string' ? obj.error : (obj.error?.message ?? '')
    return { delta, error }
  } catch {
    return { delta: '', error: '' }
  }
}

/** 喂入一段 SSE 原始文本，解析出全部 data 行增量并回调；返回未消费完的缓冲尾部。 */
function feedSseText(buf: string, text: string, onDelta: (delta: string) => void, onError: (error: string) => void): string {
  buf += text
  // SSE 以空行分隔事件；保留最后一段不完整数据等下一批
  const parts = buf.split(/\r?\n\r?\n/)
  buf = parts.pop() ?? ''
  for (const part of parts) {
    for (const line of part.split(/\r?\n/)) {
      if (!line.startsWith('data:')) continue
      const payload = line.slice(5).trim()
      if (!payload || payload === '[DONE]') continue
      const { delta, error } = extractDelta(payload)
      if (error) onError(error)
      if (delta) onDelta(delta)
    }
  }
  return buf
}

/**
 * 桌面端：走 Tauri IPC Channel 流式对话。
 *
 * webview 的 fetch 流式在 WKWebView 下不可靠，桌面端改为 invoke
 * `chat_completions_stream`，Rust 侧直接驱动代理引擎，把归一化 SSE
 * 增量经 IPC Channel 逐块推回；取消通过 `cancel_chat_stream` 协作完成。
 */
async function streamCompletionViaIpc(
  requestId: string,
  model: string,
  messages: { role: string; content: string }[],
  isCancelled: () => boolean,
  onDelta: (delta: string) => void,
): Promise<void> {
  const { Channel, invoke } = await import('@tauri-apps/api/core')

  let buf = ''
  let upstreamError = ''

  const onEvent = new Channel<{
    type: 'delta' | 'complete' | 'done' | 'error'
    data?: { text?: string; body?: string; message?: string; ok?: boolean }
  }>()
  onEvent.onmessage = (ev) => {
    try {
      if (ev?.type === 'delta' && ev.data?.text) {
        buf = feedSseText(buf, ev.data.text, onDelta, (e) => { upstreamError = upstreamError || e })
      } else if (ev?.type === 'complete' && ev.data?.body) {
        const { delta, error } = extractDelta(ev.data.body)
        if (error) upstreamError = upstreamError || error
        if (delta) onDelta(delta)
      } else if (ev?.type === 'error') {
        upstreamError = ev.data?.message || '上游请求失败'
      }
    } catch (e) {
      upstreamError = e instanceof Error ? e.message : String(e)
    }
  }

  await invoke('chat_completions_stream', {
    onEvent,
    requestId,
    model,
    messages,
    stream: true,
  })

  if (isCancelled()) throw new CancelledError()
  if (upstreamError) throw new Error(upstreamError)
}

/** 用户主动取消的标记错误（区别于超时/上游错误）。 */
class CancelledError extends Error {
  constructor() {
    super('cancelled')
    this.name = 'CancelledError'
  }
}

/**
 * 执行流式对话（桌面端走 Tauri IPC，Web 端走网关 SSE fetch），
 * 把增量派发到事件总线。
 * 完成/失败都会落库（更新消息 content 与 status），保证刷新后不丢内容。
 */
async function streamCompletion(topicId: string, assistantId: string): Promise<void> {
  const s = load()
  const topic = s.topics.find((t) => t.id === topicId)
  if (!topic) return

  // 上下文取活动分支上、assistant 节点之前的全部消息
  const path = activePath(s.messages, topic)
  const idx = path.findIndex((m) => m.id === assistantId)
  const history = (idx >= 0 ? path.slice(0, idx) : path).filter((m) => m.content.trim() !== '')
  const payloadMessages = history.map((m) => ({ role: m.role, content: m.content }))

  let cancelled = false
  const isCancelled = () => cancelled

  // 请求标识：桌面端用于协作取消（cancel_chat_stream）
  const requestId = `${topicId}:${Date.now()}:${Math.random().toString(36).slice(2, 8)}`

  // 取消句柄：桌面端触发 IPC 取消命令，Web 端触发 fetch abort
  let cancelFn: () => void = () => {}
  aborters.set(topicId, {
    abort: () => {
      cancelled = true
      cancelFn()
    },
  })

  // Web 端超时兜底：180s ≈ 后端 60s 首字节超时 × 3 次尝试
  //（桌面端无此限时，由 Rust 侧首字节/逐块超时保护）
  let timeoutId = 0

  let acc = ''
  const onDelta = (delta: string) => {
    acc += delta
    chunkBus.emit({ topicId, messageId: assistantId, delta })
  }

  try {
    if (runtime.kind === 'desktop') {
      // 预绑定 IPC 取消；invoke 完成即流结束
      cancelFn = () => {
        import('@tauri-apps/api/core')
          .then(({ invoke }) => invoke('cancel_chat_stream', { requestId }))
          .catch(() => {})
      }
      await streamCompletionViaIpc(requestId, topic.model, payloadMessages, isCancelled, onDelta)
    } else {
      const controller = new AbortController()
      cancelFn = () => controller.abort()
      timeoutId = window.setTimeout(() => controller.abort(), 180000)

      const key = await gatewayKey()
      const headers: Record<string, string> = { 'Content-Type': 'application/json' }
      if (key) headers.Authorization = `Bearer ${key}`

      const url = `${GATEWAY_BASE}/v1/chat/completions`
      const res = await fetch(url, {
        method: 'POST',
        headers,
        signal: controller.signal,
        body: JSON.stringify({ model: topic.model, stream: true, messages: payloadMessages }),
      })

      if (!res.ok) {
        const text = await res.text().catch(() => '')
        const hint = parseGatewayError(text) || `网关返回 ${res.status}`
        throw new Error(hint)
      }
      if (!res.body) throw new Error('网关未返回流式响应体')

      let httpError = ''
      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buf = ''
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        buf = feedSseText(buf, decoder.decode(value, { stream: true }), onDelta, (e) => { httpError = httpError || e })
      }

      if (httpError) throw new Error(httpError)
      if (!acc) {
        throw new Error('网关返回了空响应（无内容），可能未配置上游提供商或模型不可用')
      }
    }

    if (!acc) {
      throw new Error('网关返回了空响应（无内容），可能未配置上游提供商或模型不可用')
    }

    patchMessage(assistantId, { content: acc, status: 'success' })
    touchTopicTitle(topicId)
    doneBus.emit({ topicId, messageId: assistantId, content: acc })
  } catch (e) {
    const isCancel = cancelled || e instanceof CancelledError
    const isAbort = e instanceof DOMException && e.name === 'AbortError'
    if (isCancel && acc) {
      // 用户手动取消：保留已生成内容
      patchMessage(assistantId, { content: acc, status: 'success' })
      errorBus.emit({ topicId, messageId: assistantId, error: '已取消' })
    } else if (isCancel || (isAbort && !acc)) {
      // 超时取消（Web 端）
      const msg = '请求超时（180 秒无响应），请检查网关是否已启动且上游提供商可用'
      patchMessage(assistantId, { content: msg, status: 'error' })
      errorBus.emit({ topicId, messageId: assistantId, error: msg })
    } else {
      const msg = e instanceof Error ? e.message : String(e)
      patchMessage(assistantId, { content: msg, status: 'error' })
      errorBus.emit({ topicId, messageId: assistantId, error: msg })
    }
  } finally {
    if (timeoutId) window.clearTimeout(timeoutId)
    aborters.delete(topicId)
  }
}

/** 网关错误体 → 可读文案。 */
function parseGatewayError(text: string): string {
  try {
    const obj = JSON.parse(text) as { error?: { message?: string } | string }
    if (typeof obj.error === 'string') return obj.error
    return obj.error?.message ?? ''
  } catch {
    return text.slice(0, 300)
  }
}

/** 局部更新单条消息并持久化。 */
function patchMessage(id: string, patch: Partial<StoredMessage>): void {
  const s = load()
  const m = s.messages.find((x) => x.id === id)
  if (!m) return
  Object.assign(m, patch, { updatedAt: nowIso() })
  save(s)
}

/** 首条用户消息作为会话标题（未手动改名时）。 */
function touchTopicTitle(topicId: string): void {
  const s = load()
  const t = s.topics.find((x) => x.id === topicId)
  if (!t || t.title) return
  const first = s.messages
    .filter((m) => m.topicId === topicId && m.role === 'user')
    .sort((a, b) => (a.createdAt < b.createdAt ? -1 : 1))[0]
  if (!first) return
  t.title = first.content.trim().slice(0, 24)
  t.updatedAt = nowIso()
  save(s)
}

/* ============ 对外 API ============ */

/** chatApi：对话会话与消息的对外接口 */
export const chatApi = {
  async listTopics(): Promise<ChatTopic[]> {
    const s = load()
    return [...s.topics].sort((a, b) => (a.updatedAt < b.updatedAt ? 1 : -1))
  },

  async createTopic(req: CreateChatTopicRequest): Promise<ChatTopic> {
    const s = load()
    const t: ChatTopic = {
      id: uid(),
      title: req.title ?? '',
      endpointId: req.endpointId,
      model: req.model,
      activeNodeId: null,
      createdAt: nowIso(),
      updatedAt: nowIso(),
    }
    s.topics.unshift(t)
    save(s)
    return t
  },

  async updateTopic(id: string, req: UpdateChatTopicRequest): Promise<ChatTopic> {
    const s = load()
    const t = s.topics.find((x) => x.id === id)
    if (!t) throw new Error('会话不存在')
    if (req.title !== undefined) t.title = req.title
    if (req.endpointId !== undefined) t.endpointId = req.endpointId
    if (req.model !== undefined) t.model = req.model
    t.updatedAt = nowIso()
    save(s)
    return t
  },

  async deleteTopic(id: string): Promise<void> {
    aborters.get(id)?.abort()
    const s = load()
    s.topics = s.topics.filter((t) => t.id !== id)
    s.messages = s.messages.filter((m) => m.topicId !== id)
    save(s)
  },

  async listMessages(topicId: string): Promise<BranchMessage[]> {
    const s = load()
    const t = s.topics.find((x) => x.id === topicId)
    if (!t) return []
    return toBranchMessages(s.messages, t)
  },

  /** 切换到指定兄弟节点所在分支：活动叶子定位到该子树最深处。 */
  async setActiveNode(topicId: string, nodeId: string): Promise<ChatTopic> {
    const s = load()
    const t = s.topics.find((x) => x.id === topicId)
    if (!t) throw new Error('会话不存在')
    t.activeNodeId = deepestLeaf(s.messages, topicId, nodeId)
    t.updatedAt = nowIso()
    save(s)
    return t
  },

  /** 追加一轮问答：用户消息挂在当前活动叶子下，助手消息为其子节点。 */
  async send(topicId: string, content: string): Promise<ChatSendResponse> {
    const s = load()
    const t = s.topics.find((x) => x.id === topicId)
    if (!t) throw new Error('会话不存在')

    const userId = uid()
    const assistantId = uid()
    const ts = nowIso()
    s.messages.push({
      id: userId,
      topicId,
      parentId: t.activeNodeId ?? null,
      role: 'user',
      content,
      status: 'success',
      createdAt: ts,
      updatedAt: ts,
    })
    s.messages.push({
      id: assistantId,
      topicId,
      parentId: userId,
      role: 'assistant',
      content: '',
      status: 'pending',
      createdAt: ts,
      updatedAt: ts,
    })
    t.activeNodeId = assistantId
    t.updatedAt = ts
    save(s)
    touchTopicTitle(topicId)

    void streamCompletion(topicId, assistantId)
    return { userMessageId: userId, assistantMessageId: assistantId }
  },

  /** 重生成：在同一用户消息下新开一个助手分支（保留旧分支可切回）。 */
  async regenerate(topicId: string, assistantMessageId: string): Promise<ChatSendResponse> {
    const s = load()
    const t = s.topics.find((x) => x.id === topicId)
    if (!t) throw new Error('会话不存在')
    const old = s.messages.find((m) => m.id === assistantMessageId)
    if (!old) throw new Error('消息不存在')

    const newId = uid()
    const ts = nowIso()
    s.messages.push({
      id: newId,
      topicId,
      parentId: old.parentId,
      role: 'assistant',
      content: '',
      status: 'pending',
      createdAt: ts,
      updatedAt: ts,
    })
    t.activeNodeId = newId
    t.updatedAt = ts
    save(s)

    void streamCompletion(topicId, newId)
    return { userMessageId: old.parentId ?? '', assistantMessageId: newId }
  },

  /** 删除某条消息及其整棵子树（活动叶子回退到其父节点）。 */
  async deleteMessage(topicId: string, messageId: string): Promise<void> {
    const s = load()
    const t = s.topics.find((x) => x.id === topicId)
    if (!t) return
    const target = s.messages.find((m) => m.id === messageId)
    if (!target) return
    const drop = subtreeIds(s.messages, topicId, messageId)
    s.messages = s.messages.filter((m) => !drop.has(m.id))
    if (t.activeNodeId && drop.has(t.activeNodeId)) {
      t.activeNodeId = target.parentId
    }
    t.updatedAt = nowIso()
    save(s)
  },

  async abort(topicId: string): Promise<void> {
    aborters.get(topicId)?.abort()
  },

  onChunk(cb: (p: ChatChunkPayload) => void): Unlisten {
    return chunkBus.on(cb)
  },
  onDone(cb: (p: ChatDonePayload) => void): Unlisten {
    return doneBus.on(cb)
  },
  onError(cb: (p: ChatErrorPayload) => void): Unlisten {
    return errorBus.on(cb)
  },
}

/* ============ 模型选项 ============ */

export interface ModelOptionGroup {
  /** 分组 id（`owned_by`）。 */
  id: string
  name: string
  models: string[]
}

/** `endpointId::model` 复合键。 */
export function modelKey(endpointId: string, model: string): string {
  return `${endpointId}::${model}`
}

/**
 * 解析 `endpointId::model` 复合键。
 * @param key - 复合键字符串
 * @returns 解析结果，格式不合法时返回 null
 */
export function parseModelKey(key: string): { endpointId: string; model: string } | null {
  const i = key.indexOf('::')
  if (i <= 0) return null
  const endpointId = key.slice(0, i)
  const model = key.slice(i + 2)
  if (!endpointId || !model) return null
  return { endpointId, model }
}

/**
 * 内置常见提供商的默认模型兜底列表。
 * 当用户已配置连接但未手动挂载模型、也未设置 default_model 时使用，
 * 保证对话页至少有模型可选。模型名为各提供商官方常用 ID，发送时网关按前缀路由。
 */
const PROVIDER_DEFAULT_MODELS: Record<string, string[]> = {
  openai: ['gpt-4o', 'gpt-4o-mini', 'gpt-3.5-turbo'],
  anthropic: ['claude-sonnet-4-20250514', 'claude-haiku-4-20250414', 'claude-opus-4-20250514'],
  gemini: ['gemini-2.0-flash', 'gemini-1.5-pro', 'gemini-1.5-flash'],
  deepseek: ['deepseek-chat', 'deepseek-reasoner'],
  groq: ['llama-3.3-70b-versatile', 'mixtral-8x7b-32768', 'llama-3.1-8b-instant'],
  xai: ['grok-beta', 'grok-2'],
  mistral: ['mistral-large-latest', 'mistral-small-latest', 'codestral-latest'],
  openrouter: ['openai/gpt-4o', 'anthropic/claude-sonnet-4', 'meta-llama/llama-3.3-70b-instruct'],
  cohere: ['command-r-plus', 'command-r', 'command-light'],
  together: ['meta-llama/Llama-3.3-70B-Instruct-Turbo', 'mistralai/Mixtral-8x7B-Instruct-v0.1'],
  fireworks: ['accounts/fireworks/models/llama-v3p1-70b-instruct', 'accounts/fireworks/models/mixtral-8x7b-instruct'],
  cerebras: ['llama3.3-70b', 'llama3.1-8b'],
  nvidia: ['meta/llama-3.3-70b-instruct', 'mistralai/mixtral-8x7b-instruct-v0.1'],
  ollama: ['llama3.2', 'qwen2.5', 'deepseek-r1'],
  siliconflow: ['deepseek-ai/DeepSeek-V3', 'Qwen/Qwen2.5-72B-Instruct', 'deepseek-ai/DeepSeek-R1'],
  huggingface: ['meta-llama/Llama-3.2-3B-Instruct', 'mistralai/Mistral-7B-Instruct-v0.3'],
  pollinations: ['openai/gpt-4o-mini', 'meta-llama/Llama-3.3-70B-Instruct'],
  perplexity: ['sonar-pro', 'sonar', 'sonar-reasoning'],
  qwen: ['qwen-plus', 'qwen-turbo', 'qwen-max'],
  minimax: ['abab6.5s-chat', 'abab6.5-chat'],
}

/** 从单个连接提取模型 ID 列表：优先 models 数组，回退 defaultModel。 */
function modelIdsFromConnection(c: ProviderConnection): string[] {
  const ids: string[] = []
  if (c.models && c.models.length > 0) {
    for (const m of c.models) {
      if (m.id && !ids.includes(m.id)) ids.push(m.id)
    }
  }
  if (c.defaultModel && !ids.includes(c.defaultModel)) {
    ids.push(c.defaultModel)
  }
  return ids
}

/**
 * 从已配置连接构造模型分组（fallback 路径）。
 * 遍历所有活跃连接，提取 models / defaultModel；
 * 若连接两者都为空，使用内置默认模型列表兜底。
 */
async function modelGroupsFromConnections(): Promise<ModelOptionGroup[]> {
  try {
    const data = await listProviders()
    const connections = data.connections ?? []
    const groups = new Map<string, ModelOptionGroup>()

    for (const c of connections) {
      if (!c.isActive) continue
      const providerId = c.provider
      if (!providerId) continue

      let ids = modelIdsFromConnection(c)
      // 连接未挂载任何模型时，用内置默认模型兜底
      if (ids.length === 0) {
        ids = PROVIDER_DEFAULT_MODELS[providerId] ?? []
      }
      if (ids.length === 0) continue

      let g = groups.get(providerId)
      if (!g) {
        g = { id: providerId, name: providerId, models: [] }
        groups.set(providerId, g)
      }
      for (const id of ids) {
        // 与后端 /v1/models 保持一致的完整 ID 格式：{provider_id}/{model_id}
        const full = `${providerId}/${id}`
        if (!g.models.includes(full)) g.models.push(full)
      }
    }

    return [...groups.values()].sort((a, b) => a.name.localeCompare(b.name))
  } catch {
    return []
  }
}

/**
 * 获取可用模型分组，三级降级：
 * 1. 网关 `GET /v1/models`（本地已挂载模型，瞬时返回）
 * 2. 从已配置连接的 models / defaultModel 提取
 * 3. 内置常见提供商默认模型兜底
 *
 * 保证用户只要配置了活跃连接，对话页就有模型可选。
 */
export async function listModelGroups(): Promise<ModelOptionGroup[]> {
  // 第一级：网关 /v1/models
  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), 5000)
  try {
    const key = await gatewayKey()
    const headers: Record<string, string> = {}
    if (key) headers.Authorization = `Bearer ${key}`
    const res = await fetch(`${GATEWAY_BASE}/v1/models`, { headers, signal: controller.signal })
    if (res.ok) {
      const body = (await res.json()) as { data?: { id?: string; owned_by?: string }[] }
      const groups = new Map<string, ModelOptionGroup>()
      for (const m of body.data ?? []) {
        const id = m.id
        if (!id) continue
        const owner = m.owned_by || 'unknown'
        let g = groups.get(owner)
        if (!g) {
          g = { id: owner, name: owner, models: [] }
          groups.set(owner, g)
        }
        if (!g.models.includes(id)) g.models.push(id)
      }
      const result = [...groups.values()].sort((a, b) => a.name.localeCompare(b.name))
      if (result.length > 0) return result
    }
  } catch {
    /* 降级到 fallback */
  } finally {
    window.clearTimeout(timer)
  }

  // 第二级 + 第三级：从已配置连接提取，内置默认模型兜底
  return modelGroupsFromConnections()
}
