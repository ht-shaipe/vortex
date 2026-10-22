<template>
  <div class="web-providers-root max-w-920px mx-auto">
    <PageHeader title="网页版账号" sub="通过本地浏览器登录官方网站，把网页版对话包装成 API 供网关调用">
      <template #actions>
        <button type="button" class="btn sm" :disabled="loading" @click="refresh">
          {{ loading ? '检查中…' : '检查登录状态' }}
        </button>
      </template>
    </PageHeader>

    <!-- 说明条 -->
    <div class="note flex items-start gap-8px px-14px py-12px mb-[var(--gap-lg)] rounded-sm border border-solid border-line bg-surface-2">
      <el-icon :size="14" class="shrink-0 mt-2px" style="color: var(--accent)"><InfoFilled /></el-icon>
      <span class="text-12px text-ink-3 leading-[1.7]">
        授权会拉起本地 Chrome 打开平台登录页（扫码 / 验证码均可），登录凭证自动导入网关并持久化，
        重启无需重新登录。授权成功后自动添加对应连接到「订阅管理」，请求时模型名即平台名（如
        <span class="mono">doubao</span>）。多个网页账号会共用同一浏览器会话。
        <br />
        <span style="color: var(--warn)">
          免责提示：以程序化方式使用网页版会话可能违反平台服务条款，存在账号被限制或封禁的风险；
          本功能仅供技术研究与个人学习，请合理控制调用频率，勿用于批量注册、额度转售或对外提供服务。
          详见 README「免责申明」。
        </span>
      </span>
    </div>

    <!-- 平台卡片网格 -->
    <div class="grid grid-cols-1 gap-[var(--gap-md)] max-720px:grid-cols-1">
      <div
        v-for="p in platforms"
        :key="p.platform"
        class="platform-card card flex items-center gap-16px"
      >
        <ProviderLogo :name="p.platform" :hint="labelOf(p.platform)" :size="40" />

        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-8px">
            <span class="text-14.5px font-semibold text-ink">{{ labelOf(p.platform) }}</span>
            <span class="mono text-11px text-ink-4">{{ siteOf(p.platform) }}</span>
          </div>
          <div class="flex items-center gap-8px mt-4px">
            <StatusBadge :tone="loginTone(p)" :label="loginLabel(p)" />
            <span v-if="connCount(p.platform) > 0" class="conn-tag text-11px text-ink-3 bg-surface-2 py-2px px-8px rounded-full border border-solid border-line">
              已接入 ×{{ connCount(p.platform) }}
            </span>
          </div>
        </div>

        <button
          type="button"
          class="btn sm shrink-0"
          :class="{ primary: !p.loggedIn }"
          @click="authorize(p.platform)"
        >
          {{ p.loggedIn ? '重新授权' : '授权登录' }}
        </button>
      </div>
    </div>

    <!-- CDP 登录向导：选择平台后直接授权 -->
    <CdpLoginWizard
      v-model:visible="wizardVisible"
      :platform="wizardPlatform"
      :platform-name="labelOf(wizardPlatform)"
      @success="onAuthorized"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * WebProviders.vue — 网页版账号管理页。
 *
 * 职责：集中展示各网页版平台（豆包 / ChatGPT / DeepSeek / 千问 / Kimi）的
 * 登录态与接入状态；选择平台即拉起 CDP 登录向导（本地 Chrome 授权），
 * 授权成功后自动创建对应连接（无需先填表单）。登录态定时巡检 + 手动刷新。
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { InfoFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import StatusBadge from '@/components/ui/StatusBadge.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import CdpLoginWizard from '@/components/subscriptions/CdpLoginWizard.vue'
import { listWebProviders, type WebPlatform } from '@/api/webProviders'
import { listProviders, createProvider } from '@/api/providers'

/** 平台展示名 */
const PLATFORM_LABELS: Record<string, string> = {
  doubao: '豆包',
  chatgpt: 'ChatGPT',
  claude: 'Claude',
  deepseek: 'DeepSeek',
  qianwen: '通义千问',
  kimi: 'Kimi',
}
/** 平台站点（展示用） */
const PLATFORM_SITES: Record<string, string> = {
  doubao: 'www.doubao.com',
  chatgpt: 'chatgpt.com',
  claude: 'claude.ai',
  deepseek: 'chat.deepseek.com',
  qianwen: 'www.qianwen.com',
  kimi: 'www.kimi.com',
}

// 平台状态列表（GET /web-providers）
const platforms = ref<WebPlatform[]>([])
// 已接入连接数（provider id → 数量）
const connCounts = ref<Record<string, number>>({})
// 是否正在刷新
const loading = ref(false)
// CDP 向导状态
const wizardVisible = ref(false)
const wizardPlatform = ref('')
// 登录态巡检定时器（30s）
let pollTimer: ReturnType<typeof setInterval> | null = null

/** 平台展示名（未知平台回退 id） */
function labelOf(platform: string): string {
  return PLATFORM_LABELS[platform] ?? platform
}
/** 平台站点（未知平台回退 id） */
function siteOf(platform: string): string {
  return PLATFORM_SITES[platform] ?? platform
}
/** 指定平台已接入的连接数 */
function connCount(platform: string): number {
  return connCounts.value[`web-${platform}`] ?? 0
}

/** 登录态徽标文案 */
function loginLabel(p: WebPlatform): string {
  if (!p.enabled) return '未启用'
  if (p.loggedIn === null) return '未知'
  return p.loggedIn ? '已登录' : '未登录'
}
/** 登录态徽标色调 */
function loginTone(p: WebPlatform): 'ok' | 'warn' | 'neutral' {
  if (!p.enabled) return 'neutral'
  if (p.loggedIn === null) return 'neutral'
  return p.loggedIn ? 'ok' : 'warn'
}

/**
 * 刷新平台登录态与接入计数。
 */
async function refresh() {
  loading.value = true
  try {
    const [web, subs] = await Promise.all([listWebProviders(), listProviders().catch(() => null)])
    platforms.value = web.platforms ?? []
    const counts: Record<string, number> = {}
    for (const c of subs?.connections ?? []) {
      counts[c.provider] = (counts[c.provider] ?? 0) + 1
    }
    connCounts.value = counts
  } catch (e: unknown) {
    const resp = e as { response?: { data?: { error?: string } }; message?: string }
    ElMessage.error(resp.response?.data?.error || resp.message || '获取网页版平台状态失败')
  } finally {
    loading.value = false
  }
}

/**
 * 发起平台授权：拉起 CDP 登录向导（worker 未启动时后端按需启动）。
 */
function authorize(platform: string) {
  wizardPlatform.value = platform
  wizardVisible.value = true
}

/**
 * 授权成功回调：确保对应连接存在（无则自动创建），随后刷新状态。
 */
async function onAuthorized() {
  const platform = wizardPlatform.value
  wizardVisible.value = false
  if (platform) {
    await ensureConnection(platform)
  }
  await refresh()
}

/**
 * 确保平台的网关连接存在；不存在时以默认配置创建（模型名 = 平台名）。
 */
async function ensureConnection(platform: string) {
  const providerId = `web-${platform}`
  try {
    const data = await listProviders()
    const existing = (data.connections ?? []).filter((c) => c.provider === providerId)
    if (existing.length > 0) {
      return
    }
    const label = labelOf(platform)
    await createProvider({
      provider: providerId,
      name: `${label} 连接`,
      authType: 'web',
      defaultModel: platform,
      models: [{ id: platform }],
    })
    ElMessage.success(`${label} 已添加到「订阅管理」`)
  } catch (e: unknown) {
    const resp = e as { response?: { data?: { error?: string } }; message?: string }
    ElMessage.error(resp.response?.data?.error || resp.message || '连接创建失败，可稍后在订阅管理中手动添加')
  }
}

onMounted(() => {
  void refresh()
  // 登录态巡检：cookie 过期 / 风控掉登录时及时发现
  pollTimer = setInterval(() => {
    void refresh()
  }, 30_000)
})

onBeforeUnmount(() => {
  if (pollTimer !== null) {
    clearInterval(pollTimer)
    pollTimer = null
  }
})
</script>

<style scoped>
/* 平台卡片：与全局 card 一致的底色，稍紧凑的内边距 */
.platform-card {
  padding: 16px 18px;
}
</style>
