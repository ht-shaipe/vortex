/**
 * 通知管理组合式函数。
 * 定期轮询服务端通知，对未读通知弹出桌面通知提示，
 * 已读状态持久化到 localStorage，支持标记已读和测试通知。
 */
import { ref, readonly } from 'vue'
import { fetchNotifications, type NotificationItem } from '@/api/notifications'
import router from '@/router'

/** localStorage 中存储已读通知 ID 的键名。 */
const READ_KEY = 'vortex-read-notifications'
/** 轮询间隔：5 分钟。 */
const POLL_INTERVAL = 5 * 60 * 1000

// 未读通知数量
const unreadCount = ref(0)
// 最新通知列表
const latestNotifications = ref<NotificationItem[]>([])

// 轮询定时器
let _timer: ReturnType<typeof setInterval> | null = null
// 是否已启动轮询
let _started = false

/**
 * 从 localStorage 读取已读通知 ID 集合。
 * @returns 已读通知 ID 的 Set
 */
function getReadIds(): Set<string> {
  try {
    const raw = localStorage.getItem(READ_KEY)
    if (!raw) return new Set()
    const arr = JSON.parse(raw) as string[]
    return new Set(arr)
  } catch {
    return new Set()
  }
}

/**
 * 将指定通知 ID 标记为已读并持久化。
 * 已读列表超过 500 条时自动裁剪最早的记录。
 * @param ids - 需标记已读的通知 ID 数组
 */
function markRead(ids: string[]): void {
  if (ids.length === 0) return
  const read = getReadIds()
  for (const id of ids) read.add(id)
  const arr = Array.from(read)
  // 限制已读列表最多 500 条，超出时移除最早的
  if (arr.length > 500) arr.splice(0, arr.length - 500)
  localStorage.setItem(READ_KEY, JSON.stringify(arr))
}

/**
 * 弹出一条桌面通知。
 * 点击通知时跳转到通知 URL 或默认的免费 Token 页面。
 * @param n - 通知项
 */
function showNotification(n: NotificationItem): void {
  import('element-plus').then(({ ElNotification }) => {
    ElNotification({
      title: n.title,
      message: n.body,
      type: n.type === 'warning' ? 'warning' : n.type === 'success' ? 'success' : 'info',
      duration: 10000,
      position: 'bottom-right',
      onClick: () => {
        // 有外链则打开，否则跳转到免费 Token 页
        if (n.url) {
          window.open(n.url, '_blank', 'noopener')
        } else {
          router.push('/free-tokens')
        }
      },
    })
  })
}

/**
 * 检查并处理新通知。
 * 拉取通知列表后，对未读通知逐条弹出提示并标记已读。
 * @param silent - 是否静默模式（静默时获取失败不提示用户）
 */
async function checkNotifications(silent = false): Promise<void> {
  try {
    const items = await fetchNotifications()
    if (items.length === 0) return

    // 过滤出未读通知
    const readIds = getReadIds()
    const fresh = items.filter((n) => !readIds.has(n.id))

    latestNotifications.value = items
    unreadCount.value = fresh.length

    if (fresh.length === 0) return

    // 逐条弹出未读通知
    for (const n of fresh) {
      showNotification(n)
    }

    // 标记为已读
    markRead(fresh.map((n) => n.id))
    unreadCount.value = 0
  } catch {
    // 非静默模式下提示获取失败
    if (!silent) {
      const { ElMessage } = await import('element-plus')
      ElMessage.warning('通知获取失败，稍后重试')
    }
  }
}

/**
 * 发送一条测试通知（用于调试）。
 */
function testNotification(): void {
  showNotification({
    id: 'test-' + Date.now(),
    title: '发现新的免费 Token',
    body: 'OpenRouter 提供 $5 免费额度，点击查看详情并领取。',
    type: 'success',
    createdAt: new Date().toISOString(),
  })
}

/**
 * 启动通知轮询。
 * 立即检查一次，之后按 POLL_INTERVAL 定时检查。
 * 重复调用不会创建多个定时器。
 */
function startPolling(): void {
  if (_started) return
  _started = true
  // 立即检查一次
  checkNotifications(true)
  // 设置定时轮询
  _timer = setInterval(() => checkNotifications(true), POLL_INTERVAL)
}

/**
 * 停止通知轮询，清除定时器。
 */
function stopPolling(): void {
  if (_timer) {
    clearInterval(_timer)
    _timer = null
  }
  _started = false
}

/**
 * 将当前所有通知标记为已读。
 */
function markAllRead(): void {
  const ids = latestNotifications.value.map((n) => n.id)
  markRead(ids)
  unreadCount.value = 0
}

/**
 * 通知管理组合式函数。
 * @returns 包含未读数、通知列表等只读 ref 及操作方法的对象
 */
export function useNotifications() {
  return {
    unreadCount: readonly(unreadCount),
    latestNotifications: readonly(latestNotifications),
    checkNotifications,
    testNotification,
    startPolling,
    stopPolling,
    markAllRead,
  }
}
