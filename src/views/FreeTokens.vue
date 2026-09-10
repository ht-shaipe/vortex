<template>
  <div>
    <!-- 页面头部：标题、视图切换与提交推荐按钮 -->
    <PageHeader title="免费 Token" sub="收录可申请免费额度的 AI 平台，标注是否提供 API 与申请门槛">
      <template #actions>
        <div class="radio-group">
          <button class="radio-option" :class="{ active: view === 'card' }" @click="view = 'card'">卡片</button>
          <button class="radio-option" :class="{ active: view === 'table' }" @click="view = 'table'">表格</button>
        </div>
        <button class="btn primary" @click="openSubmit">
          <el-icon :size="14"><Plus /></el-icon>提交推荐
        </button>
      </template>
    </PageHeader>

    <!-- 筛选工具条：搜索、区域切换、仅 API、我的推荐 -->
    <div class="card toolbar">
      <div class="tb-search">
        <el-input v-model="keyword" placeholder="搜索站点名称、标签或说明" clearable />
      </div>

      <div class="radio-group">
        <button
          v-for="r in regionTabs"
          :key="r.value"
          class="radio-option"
          :class="{ active: region === r.value }"
          @click="region = r.value"
        >{{ r.label }}</button>
      </div>

      <button class="btn sm" :class="{ accent: onlyApi }" @click="onlyApi = !onlyApi">
        <el-icon :size="13"><Select /></el-icon>仅支持 API
      </button>
      <button class="btn sm" :class="{ accent: onlyUser }" @click="onlyUser = !onlyUser">
        <el-icon :size="13"><Star /></el-icon>我的推荐
      </button>

      <div class="tb-count">共 <b>{{ filtered.length }}</b> / {{ sites.length }} 个站点</div>
    </div>

    <!-- 说明提示 -->
    <div class="notice">
      免费额度与限速随平台政策变动，此处信息仅作参考，请以各平台官网为准。
      「我的推荐」会提交到远程服务器统一管理，本地同时保留副本便于离线查看。
    </div>

    <!-- 加载中 -->
    <div v-if="loading" class="spin-wrap">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <!-- 加载失败 -->
    <div v-else-if="loadError" class="card">
      <EmptyState title="无法读取站点列表" :desc="loadError">
        <button class="btn primary" @click="load">重试</button>
      </EmptyState>
    </div>

    <!-- 空态 -->
    <div v-else-if="filtered.length === 0" class="card">
      <EmptyState title="没有匹配的站点" desc="换个关键词，或提交一个新的免费 Token 站点推荐">
        <button class="btn primary" @click="openSubmit">提交推荐</button>
      </EmptyState>
    </div>

    <!-- 卡片视图 -->
    <div v-else-if="view === 'card'" class="grid">
      <div v-for="s in filtered" :key="s.id" class="card site-card">
        <!-- 卡片头部：站点名与 API 支持标识 -->
        <div class="sc-head">
          <div class="sc-name">
            <span class="sc-title">{{ s.name }}</span>
            <span v-if="s.source === 'user'" class="pill warn">我的推荐</span>
          </div>
          <span class="pill" :class="s.apiSupported ? 'ok' : 'neutral'">
            <span class="dot" />{{ s.apiSupported ? '支持 API' : '无 API' }}
          </span>
        </div>

        <!-- 免费额度说明 -->
        <div class="sc-quota">{{ s.freeQuota || '免费额度信息待补充' }}</div>

        <!-- 区域与标签 -->
        <div class="sc-tags">
          <span class="pill tag">{{ regionLabel(s.region) }}</span>
          <span v-for="t in s.tags" :key="t" class="pill tag">{{ t }}</span>
        </div>

        <!-- 门槛与格式等元信息 -->
        <div class="sc-meta">
          <span :class="['meta', s.requiresCard ? 'warn' : 'ok']">{{ s.requiresCard ? '需绑定信用卡' : '免绑卡' }}</span>
          <span v-if="s.requiresVerify" class="meta">需实名 / 手机验证</span>
          <span v-if="s.apiFormat" class="meta mono">{{ s.apiFormat }}</span>
          <span v-if="s.providerId" class="meta">已内置 · {{ s.providerId }}</span>
        </div>

        <!-- API Base URL -->
        <CopyableBlock v-if="s.apiBase" variant="inline" :text="s.apiBase">{{ s.apiBase }}</CopyableBlock>

        <!-- 备注 -->
        <div v-if="s.note" class="sc-note">{{ s.note }}</div>

        <!-- 卡片底部操作：申请入口、官网、删除 -->
        <div class="sc-foot">
          <button v-if="linkFor(s)" class="btn sm accent" @click="openLink(linkFor(s))">
            <el-icon :size="13"><Link /></el-icon>申请入口
          </button>
          <button v-if="s.homeUrl" class="btn sm" @click="openLink(s.homeUrl)">官网</button>
          <button v-if="s.source === 'user'" class="btn sm danger" @click="removeSite(s)">删除</button>
          <span v-if="s.submitter" class="by">由 {{ s.submitter }} 推荐</span>
        </div>
      </div>
    </div>

    <!-- 表格视图 -->
    <div v-else class="card tbl-wrap">
      <table class="table">
        <thead>
          <tr>
            <th style="width: 190px">站点</th>
            <th style="width: 62px">区域</th>
            <th style="width: 70px">API</th>
            <th>免费额度</th>
            <th style="width: 132px">门槛</th>
            <th style="width: 148px">标签</th>
            <th style="width: 112px"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in filtered" :key="s.id">
            <!-- 站点名与 API 地址 -->
            <td>
              <div class="tbl-name">{{ s.name }}</div>
              <div v-if="s.apiBase" class="tbl-base mono">{{ s.apiBase }}</div>
            </td>
            <!-- 区域 -->
            <td><span class="pill tag">{{ regionLabel(s.region) }}</span></td>
            <!-- API 支持 -->
            <td>
              <span class="pill" :class="s.apiSupported ? 'ok' : 'neutral'">
                <span class="dot" />{{ s.apiSupported ? '是' : '否' }}
              </span>
            </td>
            <!-- 免费额度 -->
            <td><div class="tbl-quota">{{ s.freeQuota || '—' }}</div></td>
            <!-- 门槛 -->
            <td>
              <div class="tbl-gates">
                <span :class="['meta', s.requiresCard ? 'warn' : 'ok']">{{ s.requiresCard ? '需绑卡' : '免绑卡' }}</span>
                <span v-if="s.requiresVerify" class="meta">需实名</span>
              </div>
            </td>
            <!-- 标签（最多展示 2 个） -->
            <td>
              <div class="tbl-tags">
                <span v-for="t in s.tags.slice(0, 2)" :key="t" class="pill tag">{{ t }}</span>
              </div>
            </td>
            <!-- 操作按钮 -->
            <td>
              <div class="tbl-actions">
                <button v-if="linkFor(s)" class="btn sm" @click="openLink(linkFor(s))">申请</button>
                <button v-if="s.source === 'user'" class="btn sm danger" @click="removeSite(s)">删除</button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- 提交推荐弹窗 -->
    <el-dialog
      v-model="dialogVisible"
      title="提交免费 Token 站点推荐"
      width="600px"
      top="6vh"
      append-to-body
      class="ft-dialog"
    >
      <div class="form">
        <div class="f-group-title">站点信息</div>

        <!-- 站点名称 -->
        <div class="form-row">
          <div class="f-label">站点名称 <em>*</em></div>
          <el-input v-model="form.name" placeholder="例如：某某 AI 开放平台" />
        </div>

        <!-- 官网与申请地址 -->
        <div class="form-2col">
          <div class="form-row">
            <div class="f-label">官网地址</div>
            <el-input v-model="form.homeUrl" placeholder="https://" />
          </div>
          <div class="form-row">
            <div class="f-label">申请 / 控制台地址</div>
            <el-input v-model="form.applyUrl" placeholder="https://" />
          </div>
        </div>

        <div class="f-group-title">额度与接口</div>

        <!-- 区域与 API Base URL -->
        <div class="form-2col">
          <div class="form-row">
            <div class="f-label">所在区域</div>
            <el-select v-model="form.region" style="width: 100%">
              <el-option label="国内平台" value="cn" />
              <el-option label="海外平台" value="global" />
              <el-option label="本地部署" value="local" />
            </el-select>
          </div>
          <div class="form-row">
            <div class="f-label">API Base URL</div>
            <el-input v-model="form.apiBase" placeholder="https://api.example.com/v1" />
          </div>
        </div>

        <!-- 额度说明 -->
        <div class="form-row">
          <div class="f-label">额度说明</div>
          <el-input
            v-model="form.freeQuota"
            type="textarea"
            :rows="2"
            resize="none"
            placeholder="例如：新用户注册赠 2000 万 tokens；小参数模型长期免费"
          />
        </div>

        <!-- 三个布尔属性开关卡片 -->
        <div class="switch-grid">
          <div class="switch-card" :class="{ on: form.apiSupported }" @click="form.apiSupported = !form.apiSupported">
            <div class="sc-text">
              <span class="sc-t">支持 API</span>
              <span class="sc-d">可编程调用</span>
            </div>
            <el-switch v-model="form.apiSupported" @click.stop />
          </div>
          <div class="switch-card" :class="{ on: form.requiresCard }" @click="form.requiresCard = !form.requiresCard">
            <div class="sc-text">
              <span class="sc-t">需绑卡</span>
              <span class="sc-d">要信用卡</span>
            </div>
            <el-switch v-model="form.requiresCard" @click.stop />
          </div>
          <div class="switch-card" :class="{ on: form.requiresVerify }" @click="form.requiresVerify = !form.requiresVerify">
            <div class="sc-text">
              <span class="sc-t">需实名</span>
              <span class="sc-d">手机 / 实名</span>
            </div>
            <el-switch v-model="form.requiresVerify" @click.stop />
          </div>
        </div>

        <div class="f-group-title">补充信息</div>

        <!-- 标签与推荐人 -->
        <div class="form-2col">
          <div class="form-row">
            <div class="f-label">标签</div>
            <el-input v-model="form.tagsText" placeholder="逗号分隔，如：聚合平台,免绑卡" />
          </div>
          <div class="form-row">
            <div class="f-label">推荐人</div>
            <el-input v-model="form.submitter" placeholder="选填，留空则匿名" />
          </div>
        </div>

        <!-- 备注 -->
        <div class="form-row">
          <div class="f-label">备注</div>
          <el-input
            v-model="form.note"
            type="textarea"
            :rows="2"
            resize="none"
            placeholder="限速情况、使用体验、注意事项等（选填）"
          />
        </div>
      </div>

      <!-- 弹窗底部操作 -->
      <template #footer>
        <div class="dlg-foot">
          <span class="foot-hint">推荐将提交到远程服务器（本地保留副本）</span>
          <div class="foot-btns">
            <button class="btn" @click="dialogVisible = false">取消</button>
            <button class="btn primary" :disabled="submitting" @click="submitSite">
              {{ submitting ? '提交中…' : '提交推荐' }}
            </button>
          </div>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 免费 Token 页面。
 * 职责：收录可申请免费额度的 AI 平台，支持按关键词、区域、是否支持 API、
 * 是否为我的推荐进行筛选，提供卡片/表格两种视图，并支持提交与删除推荐。
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Select, Star, Link, Loading } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import CopyableBlock from '@/components/ui/CopyableBlock.vue'
import { runtime } from '@/lib/runtime'
import {
  listFreeTokenSites,
  submitFreeTokenSite,
  deleteFreeTokenSite,
  type FreeTokenSite,
} from '@/api/freeTokens'

/** 视图模式：卡片或表格 */
type ViewMode = 'card' | 'table'

// 站点列表
const sites = ref<FreeTokenSite[]>([])
// 是否正在加载
const loading = ref(true)
// 加载错误信息
const loadError = ref('')

// 当前视图模式（从 localStorage 恢复）
const view = ref<ViewMode>((localStorage.getItem('vortex-free-token-view') as ViewMode) || 'card')
// 搜索关键词
const keyword = ref('')
// 区域筛选值
const region = ref('all')
// 是否仅展示支持 API 的站点
const onlyApi = ref(false)
// 是否仅展示我的推荐
const onlyUser = ref(false)

// 区域切换标签
const regionTabs = [
  { label: '全部', value: 'all' },
  { label: '国内', value: 'cn' },
  { label: '海外', value: 'global' },
  { label: '本地', value: 'local' },
]

// 区域值到中文标签的映射
const REGION_LABELS: Record<string, string> = { cn: '国内', global: '海外', local: '本地' }
/**
 * 将区域值转为中文标签。
 * @param r 区域值
 */
function regionLabel(r: string): string {
  return REGION_LABELS[r] ?? r
}

// 筛选后的站点列表
const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase()
  return sites.value.filter((s) => {
    // 区域筛选
    if (region.value !== 'all' && s.region !== region.value) return false
    // 仅支持 API 筛选
    if (onlyApi.value && !s.apiSupported) return false
    // 仅我的推荐筛选
    if (onlyUser.value && s.source !== 'user') return false
    // 关键词搜索：匹配名称、额度、备注、API 地址与标签
    if (!kw) return true
    const haystack = [s.name, s.freeQuota, s.note ?? '', s.apiBase ?? '', ...(s.tags ?? [])]
      .join(' ')
      .toLowerCase()
    return haystack.includes(kw)
  })
})

/**
 * 取站点的申请链接，优先用申请地址，回退到官网。
 * @param s 站点
 */
function linkFor(s: FreeTokenSite): string {
  return s.applyUrl || s.homeUrl || ''
}

/** 只允许 http/https，避免把任意协议交给系统打开 */
function isSafeUrl(url: string): boolean {
  return /^https?:\/\//i.test(url)
}

/**
 * 打开外部链接：桌面端用 Tauri shell 打开，Web 端用 window.open。
 * @param url 待打开的链接
 */
async function openLink(url: string) {
  if (!url) return
  if (!isSafeUrl(url)) {
    ElMessage.warning('链接协议不受支持')
    return
  }
  if (runtime.kind === 'desktop') {
    // 桌面端：通过 Tauri shell 插件打开系统默认浏览器
    try {
      const { open } = await import('@tauri-apps/plugin-shell')
      await open(url)
    } catch (e) {
      ElMessage.error(`打开链接失败：${errMsg(e)}`)
    }
  } else {
    // Web 端：新标签页打开
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

/**
 * 将未知类型的错误转为字符串消息。
 * @param e 错误对象
 */
function errMsg(e: unknown): string {
  if (typeof e === 'string') return e
  if (e && typeof e === 'object' && 'message' in e) return String((e as { message: unknown }).message)
  return String(e)
}

/**
 * 加载站点列表。
 */
async function load() {
  loading.value = true
  loadError.value = ''
  try {
    const data = await listFreeTokenSites()
    sites.value = data.sites ?? []
  } catch (e) {
    loadError.value = `网关未响应，请确认 Vortex 服务已启动（${errMsg(e)}）`
  } finally {
    loading.value = false
  }
}

/* ---------------- 提交推荐 ---------------- */

// 提交弹窗是否可见
const dialogVisible = ref(false)
// 是否正在提交
const submitting = ref(false)

// 提交表单状态
const form = reactive({
  name: '',
  homeUrl: '',
  applyUrl: '',
  region: 'cn',
  apiSupported: true,
  apiBase: '',
  freeQuota: '',
  requiresCard: false,
  requiresVerify: false,
  tagsText: '',
  note: '',
  submitter: '',
})

/**
 * 重置提交表单为默认值。
 */
function resetForm() {
  Object.assign(form, {
    name: '',
    homeUrl: '',
    applyUrl: '',
    region: 'cn',
    apiSupported: true,
    apiBase: '',
    freeQuota: '',
    requiresCard: false,
    requiresVerify: false,
    tagsText: '',
    note: '',
    submitter: '',
  })
}

/**
 * 打开提交推荐弹窗。
 */
function openSubmit() {
  resetForm()
  dialogVisible.value = true
}

/**
 * 将逗号分隔的标签文本解析为数组，最多保留 6 个。
 * @param text 标签文本
 */
function parseTags(text: string): string[] {
  return text
    .split(/[,，]/)
    .map((t) => t.trim())
    .filter(Boolean)
    .slice(0, 6)
}

/**
 * 提交推荐站点到远程服务器并加入本地清单。
 */
async function submitSite() {
  if (!form.name.trim()) {
    ElMessage.warning('请填写站点名称')
    return
  }
  submitting.value = true
  try {
    const created = await submitFreeTokenSite({
      name: form.name.trim(),
      homeUrl: form.homeUrl.trim(),
      applyUrl: form.applyUrl.trim(),
      region: form.region,
      apiSupported: form.apiSupported,
      apiBase: form.apiBase.trim(),
      freeQuota: form.freeQuota.trim(),
      requiresCard: form.requiresCard,
      requiresVerify: form.requiresVerify,
      tags: parseTags(form.tagsText),
      note: form.note.trim(),
      submitter: form.submitter.trim(),
    })
    sites.value = [...sites.value, created]
    dialogVisible.value = false
    ElMessage.success('已加入本地清单')
  } catch (e) {
    ElMessage.error(`提交失败：${errMsg(e)}`)
  } finally {
    submitting.value = false
  }
}

/**
 * 删除推荐站点（需二次确认）。
 * @param s 待删除的站点
 */
async function removeSite(s: FreeTokenSite) {
  try {
    await ElMessageBox.confirm(`确定从本地清单中删除「${s.name}」吗？`, '删除推荐', {
      confirmButtonText: '删除',
      cancelButtonText: '取消',
      type: 'warning',
    })
  } catch {
    return
  }
  try {
    await deleteFreeTokenSite(s.id)
    sites.value = sites.value.filter((x) => x.id !== s.id)
    ElMessage.success('已删除')
  } catch (e) {
    ElMessage.error(`删除失败：${errMsg(e)}`)
  }
}

onMounted(load)

// 视图选择持久化，避免每次进页面都要重选
watch(view, (v) => localStorage.setItem('vortex-free-token-view', v))
</script>

<style scoped>
/* ---------- 工具条 ---------- */
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 10px 14px;
  margin-bottom: 12px;
}
.tb-search { width: 260px; }
.tb-count {
  margin-left: auto;
  font-size: 12px;
  color: var(--ink-3);
}
.tb-count b { color: var(--ink); font-family: var(--font-mono); }

.notice {
  font-size: 12px;
  line-height: 1.6;
  color: var(--ink-3);
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 8px 12px;
  margin-bottom: 14px;
}

.spin-wrap { padding: 48px; text-align: center; color: var(--ink-4); }

/* ---------- 卡片视图 ---------- */
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
  gap: 14px;
}
.site-card {
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding: 14px 16px;
}
.sc-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}
.sc-name { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; min-width: 0; }
.sc-title { font-size: 14px; font-weight: 600; color: var(--ink); line-height: 1.35; }
.sc-quota {
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--ink-2);
  background: var(--surface-3);
  border-radius: var(--r-sm);
  padding: 7px 10px;
}
.sc-tags { display: flex; flex-wrap: wrap; gap: 5px; }
.sc-meta { display: flex; flex-wrap: wrap; gap: 8px 12px; }

.meta { font-size: 11px; color: var(--ink-3); }
.meta.ok { color: var(--ok); }
.meta.warn { color: var(--warn); }
.meta.mono { font-family: var(--font-mono); }

.sc-note { font-size: 12px; line-height: 1.6; color: var(--ink-3); }

.sc-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: auto;
  padding-top: 4px;
}
.sc-foot .by { font-size: 11px; color: var(--ink-4); margin-left: auto; }

/* ---------- 表格视图 ---------- */
/* 固定布局锁定列宽（auto 布局下中文会被逐字压成竖排），窄窗口横向滚动 */
.tbl-wrap { overflow-x: auto; }
.tbl-wrap .table { min-width: 960px; table-layout: fixed; }
.tbl-name { font-size: 13px; font-weight: 500; color: var(--ink); }
.tbl-base {
  font-size: 11px;
  color: var(--ink-4);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.tbl-quota { font-size: 12.5px; line-height: 1.55; color: var(--ink-2); }
.tbl-gates { display: flex; flex-wrap: wrap; gap: 4px 8px; }
.tbl-gates .meta { white-space: nowrap; }
.tbl-tags { display: flex; flex-wrap: wrap; gap: 4px; }
.tbl-actions { display: flex; gap: 6px; }
.table tbody tr { cursor: default; }

/* ---------- 弹窗表单 ---------- */
.form { display: flex; flex-direction: column; gap: 11px; }
.form-row { display: flex; flex-direction: column; gap: 6px; }
.form-2col { display: grid; grid-template-columns: 1fr 1fr; gap: 11px; align-items: start; }
.f-label { font-size: 12px; font-weight: 500; color: var(--ink-2); }
.f-label em { color: var(--err); font-style: normal; }

/* 分区小标题：左侧文案 + 右侧延伸细线 */
.f-group-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  font-weight: 600;
  color: var(--ink-4);
  letter-spacing: 0.05em;
  margin-top: 3px;
}
.f-group-title::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--line);
}
.f-group-title:first-child { margin-top: 0; }

/* 三个布尔属性：整块可点击的开关卡片，替代原先裸露的开关 */
.switch-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
.switch-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface-2);
  cursor: pointer;
  transition: border-color 0.12s, background 0.12s;
}
.switch-card:hover { border-color: var(--line-2); }
.switch-card.on { border-color: var(--accent); background: var(--surface); }
.sc-text { display: flex; flex-direction: column; line-height: 1.3; min-width: 0; }
.sc-t { font-size: 12px; font-weight: 500; color: var(--ink-2); }
.sc-d { font-size: 10.5px; color: var(--ink-4); }
.switch-card :deep(.el-switch) { flex-shrink: 0; }

/* 底部操作条 */
.dlg-foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.foot-hint { font-size: 11.5px; color: var(--ink-4); }
.foot-btns { display: flex; gap: 8px; }
</style>

<!-- 弹窗通过 append-to-body 挂到 body 下，Element Plus 自身的容器元素
     没有本组件的 scoped 标记，因此容器级样式统一用 .ft-dialog 前缀限定 -->
<style>
.ft-dialog.el-dialog {
  border-radius: var(--r-lg);
  overflow: hidden;
}
.ft-dialog .el-dialog__header {
  padding: 15px 20px 12px;
  margin-right: 0;
  border-bottom: 1px solid var(--line);
}
.ft-dialog .el-dialog__title { font-size: 15px; font-weight: 600; color: var(--ink); }
.ft-dialog .el-dialog__headerbtn { top: 13px; right: 12px; }
/* 内容超长时在弹窗内部滚动，保证底部按钮始终可见 */
.ft-dialog .el-dialog__body {
  padding: 14px 20px 16px;
  max-height: 62vh;
  overflow-y: auto;
}
.ft-dialog .el-dialog__footer {
  padding: 11px 20px;
  border-top: 1px solid var(--line);
  background: var(--surface-2);
  text-align: left;
}
</style>
