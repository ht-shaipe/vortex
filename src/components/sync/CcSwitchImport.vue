<template>
  <section class="sync-card">
    <div class="sync-head">
      <div class="head-text">
        <h2 class="sync-title">从 cc-switch 迁移配置</h2>
        <p class="sync-desc">
          选择本机 cc-switch 导出的 JSON，识别可迁移的供应商并勾选导入为 vortex 连接
        </p>
      </div>
      <button type="button" class="btn sm" @click="openDialog">
        <el-icon :size="13"><Download /></el-icon>同步配置
      </button>
    </div>

    <el-dialog v-model="open" title="cc-switch 配置迁移" width="720" top="8vh">
      <div class="dlg">
        <p v-if="loading" class="hint">请选择 cc-switch 导出的 JSON 文件…</p>
        <p v-else-if="loadError" class="hint err">读取失败：{{ loadError }}</p>
        <p v-else-if="items.length === 0" class="hint">
          {{ picked ? '未在文件中找到可识别的供应商。' : '已取消选择文件。' }}
        </p>

        <template v-else>
          <div class="dlg-bar">
            <div class="bar-left">
              <button
                type="button"
                class="sel-all"
                :disabled="visibleImportable.length === 0"
                @click="selectAll"
              >
                <span class="box" :class="{ on: allSelected, dim: visibleImportable.length === 0 }">
                  <el-icon v-if="allSelected" :size="11"><Check /></el-icon>
                </span>
                全选
              </button>
              <button type="button" class="link-btn" :disabled="selected.size === 0" @click="deselectAll">
                取消全选
              </button>
              <div class="filters">
                <button
                  type="button"
                  class="filter-btn"
                  :class="{ on: appFilter.claude }"
                  aria-label="仅显示 Claude"
                  aria-pressed="appFilter.claude"
                  @click="appFilter.claude = !appFilter.claude"
                >
                  claude
                </button>
                <button
                  type="button"
                  class="filter-btn"
                  :class="{ on: appFilter.codex }"
                  aria-label="仅显示 Codex"
                  aria-pressed="appFilter.codex"
                  @click="appFilter.codex = !appFilter.codex"
                >
                  codex
                </button>
              </div>
            </div>
            <span class="bar-right tnum">
              已勾选 {{ selected.size }} / 可迁移 {{ importable.length }}（共 {{ items.length }}）
            </span>
          </div>

          <el-scrollbar class="dlg-list" max-height="46vh">
            <p v-if="visibleItems.length === 0" class="hint">当前筛选下没有可展示的项</p>
            <template v-else>
              <div
                v-for="item in visibleItems"
                :key="item.ccSwitchId"
                class="row"
                :class="{ dim: item.status === 'skipped' }"
                role="checkbox"
                :aria-checked="selected.has(item.ccSwitchId)"
                :aria-disabled="item.status === 'skipped'"
                :tabindex="item.status === 'skipped' ? -1 : 0"
                @click="item.status === 'ok' && toggle(item.ccSwitchId)"
                @keydown.enter.prevent="item.status === 'ok' && toggle(item.ccSwitchId)"
                @keydown.space.prevent="item.status === 'ok' && toggle(item.ccSwitchId)"
              >
                <span class="box" :class="{ on: selected.has(item.ccSwitchId), dim: item.status === 'skipped' }">
                  <el-icon v-if="selected.has(item.ccSwitchId)" :size="11"><Check /></el-icon>
                </span>
                <div class="row-main">
                  <span class="row-name">{{ item.name }}</span>
                  <span v-if="item.apiUrl" class="row-url mono">{{ item.apiUrl }}</span>
                  <span v-if="item.status === 'skipped'" class="row-sub">{{ skipReasonLabel(item.skipReason) }}</span>
                  <span v-else class="row-sub">{{ item.apiKeyMasked || '—' }}</span>
                </div>
                <div class="row-trail">
                  <span class="badge" :class="item.appType">{{ item.appType }}</span>
                  <el-icon :size="11" class="trail-arrow"><Right /></el-icon>
                  <span class="badge kind" :title="`导入为 provider: ${item.transformer}`">{{ item.transformer }}</span>
                </div>
              </div>
            </template>
          </el-scrollbar>
        </template>
      </div>

      <template #footer>
        <button type="button" class="btn" @click="open = false">取消</button>
        <button
          type="button"
          class="btn primary"
          :disabled="selected.size === 0 || importing"
          @click="onImport"
        >
          <el-icon :size="13"><Select /></el-icon>
          {{ importing ? '导入中…' : `导入 ${selected.size} 项` }}
        </button>
      </template>
    </el-dialog>
  </section>
</template>

<script setup lang="ts">
/**
 * CcSwitchImport.vue — CC Switch 导入
 * 职责：从 cc-switch 导出的 JSON 中识别可迁移的供应商配置，支持勾选、筛选与批量导入为 vortex 连接。
 */
import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Check, Download, Right, Select } from '@element-plus/icons-vue'
import { ccSwitchApi, errMsg, type PreviewItem } from '@/api/sync'

const open = ref(false) // 弹窗是否打开
const loading = ref(false) // 是否正在加载预览
const loadError = ref('') // 加载错误信息
const picked = ref(false) // 是否已选择文件
const items = ref<PreviewItem[]>([]) // 预览项列表
const selected = ref<Set<string>>(new Set()) // 已勾选项的 ID 集合
const appFilter = ref({ claude: false, codex: false }) // 应用类型筛选状态
const importing = ref(false) // 是否正在导入

const APP_ORDER: Record<string, number> = { claude: 0, codex: 1 } // 应用类型排序权重

/** 不可用在前；可迁移项 claude → codex，同组按名称。 */
const sortedItems = computed(() =>
  [...items.value].sort((a, b) => {
    const as = a.status === 'skipped' ? 0 : 1
    const bs = b.status === 'skipped' ? 0 : 1
    if (as !== bs) return as - bs
    const aa = APP_ORDER[a.appType] ?? 99
    const bb = APP_ORDER[b.appType] ?? 99
    if (aa !== bb) return aa - bb
    return a.name.localeCompare(b.name, 'zh-CN')
  }),
)

const importable = computed(() => items.value.filter((i) => i.status === 'ok')) // 可迁移项列表

// 按应用类型筛选后的可见项
const visibleItems = computed(() => {
  const active = appFilter.value.claude || appFilter.value.codex
  if (!active) return sortedItems.value
  return sortedItems.value.filter((i) =>
    i.appType === 'claude' ? appFilter.value.claude : i.appType === 'codex' ? appFilter.value.codex : false,
  )
})

const visibleImportable = computed(() => visibleItems.value.filter((i) => i.status === 'ok')) // 可见且可迁移的项
const allSelected = computed(
  () => visibleImportable.value.length > 0 && visibleImportable.value.every((i) => selected.value.has(i.ccSwitchId)),
) // 是否全选

/** 跳过原因 → 用户可读文案。 */
function skipReasonLabel(reason?: string): string {
  if (!reason) return '不可迁移'
  if (reason.startsWith('oauth')) return 'OAuth/托管账号，需手动配置'
  if (reason === 'managed_account') return '托管账号，不支持迁移'
  if (reason === 'no_url') return '缺少上游地址'
  if (reason === 'no_key') return '缺少 API Key'
  if (reason === 'invalid_api_url') return '上游地址无效'
  if (reason.startsWith('unsupported_app')) return '暂不支持的客户端类型'
  return reason
}

/** 切换某项的勾选状态。 */
function toggle(id: string): void {
  const next = new Set(selected.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  selected.value = next
}

/** 全选可见可迁移项。 */
function selectAll(): void {
  selected.value = new Set(visibleImportable.value.map((i) => i.ccSwitchId))
}

/** 取消全选。 */
function deselectAll(): void {
  selected.value = new Set()
}

/** 打开弹窗并加载预览数据。 */
async function openDialog(): Promise<void> {
  selected.value = new Set()
  appFilter.value = { claude: false, codex: false }
  items.value = []
  loadError.value = ''
  picked.value = false
  open.value = true

  loading.value = true
  try {
    const data = await ccSwitchApi.preview()
    picked.value = true
    items.value = data
  } catch (e) {
    loadError.value = errMsg(e)
  } finally {
    loading.value = false
  }
}

/** 执行导入：提交已勾选项到后端。 */
async function onImport(): Promise<void> {
  const chosen = items.value.filter((i) => selected.value.has(i.ccSwitchId))
  importing.value = true
  try {
    const s = await ccSwitchApi.import(chosen)
    ElMessage.success(
      `导入完成：成功 ${s.imported}` + (s.skipped > 0 ? `，跳过 ${s.skipped}` : ''),
    )
    open.value = false
  } catch (e) {
    ElMessage.error(`导入失败：${errMsg(e)}`)
  } finally {
    importing.value = false
  }
}
</script>

<style scoped>
.head-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }

.dlg { display: flex; flex-direction: column; gap: 10px; min-height: 260px; }
.hint { margin: 0; padding: 24px 8px; text-align: center; font-size: var(--fs-sm); color: var(--ink-4); }
.hint.err { color: var(--err); }

.dlg-bar { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-shrink: 0; }
.bar-left { display: flex; align-items: center; gap: 10px; }
.bar-right { font-size: var(--fs-xs); color: var(--ink-4); }

.sel-all {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  border: none;
  background: transparent;
  font-size: var(--fs-xs);
  color: var(--ink-3);
}
.sel-all:disabled { cursor: not-allowed; opacity: 0.5; }
.link-btn {
  border: none;
  background: transparent;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: var(--fs-xs);
  color: var(--ink-3);
  transition: background 0.12s, color 0.12s;
}
.link-btn:hover:not(:disabled) { background: var(--surface-3); color: var(--ink); }
.link-btn:disabled { opacity: 0.4; cursor: not-allowed; }

.filters { display: flex; align-items: center; gap: 4px; padding-left: 10px; border-left: 1px solid var(--line); }
.filter-btn {
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface);
  font-size: var(--fs-xs);
  color: var(--ink-3);
  opacity: 0.65;
  transition: all 0.12s;
}
.filter-btn:hover { opacity: 1; background: var(--surface-3); color: var(--ink); }
.filter-btn.on {
  opacity: 1;
  border-color: var(--accent-line);
  background: var(--accent-bg);
  color: var(--accent-ink);
}

.dlg-list {
  flex: 1;
  min-height: 0;

  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}

.row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 9px 12px;
  border-bottom: 1px solid var(--line);
  cursor: pointer;
  outline: none;
  transition: background 0.1s;
}
.row:last-child { border-bottom: none; }
.row:hover { background: var(--surface-2); }
.row.dim { opacity: 0.5; cursor: not-allowed; }

.box {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 15px;
  height: 15px;
  margin-top: 2px;
  border: 1px solid var(--line-2);
  border-radius: 4px;
  background: var(--surface);
  color: #fff;
  transition: background 0.12s, border-color 0.12s;
}
.box.on { background: var(--accent); border-color: var(--accent); }
.box.dim { opacity: 0.5; }

.row-main { display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0; }
.row-name { font-size: var(--fs-body); color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.row-url { font-size: var(--fs-xs); color: var(--ink-4); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.row-sub { font-size: var(--fs-xs); color: var(--ink-4); }

.row-trail { display: flex; align-items: center; gap: 5px; flex-shrink: 0; align-self: center; }
.trail-arrow { color: var(--ink-5); }
.badge {
  padding: 1px 5px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: 500;
  background: var(--surface-3);
  color: var(--ink-3);
}
.badge.claude { background: var(--warn-bg); color: var(--warn); }
.badge.codex { background: var(--accent-bg); color: var(--accent-ink); }
.badge.kind { font-family: var(--font-mono); }
</style>
