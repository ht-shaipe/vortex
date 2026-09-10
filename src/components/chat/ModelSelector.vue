<template>
  <el-popover
    v-model:visible="open"
    placement="bottom-end"
    :width="340"
    trigger="click"
    popper-class="ms-popper"
  >
    <template #reference>
      <button
        type="button"
        class="ms-trigger"
        :class="{ placeholder: isPlaceholder }"
        :title="triggerTitle"
      >
        <img v-if="triggerLogo" :src="triggerLogo" class="ms-trigger-logo" alt="" />
        <el-icon v-else :size="14" class="ms-trigger-icon"><Cpu /></el-icon>
        <span v-if="selected && !isPlaceholder && groupPrefix" class="ms-group-chip">{{ groupPrefix }}</span>
        <span class="ms-trigger-text">{{ triggerText }}</span>
        <el-icon :size="12" class="ms-caret" :class="{ flipped: open }"><ArrowDown /></el-icon>
      </button>
    </template>

    <div class="ms-panel">
      <div class="ms-search">
        <el-icon :size="13"><Search /></el-icon>
        <input
          ref="searchInput"
          v-model="query"
          class="ms-search-input"
          placeholder="搜索模型..."
        />
      </div>

      <el-scrollbar class="ms-list" height="320px">
        <p v-if="filtered.length === 0" class="ms-empty">无匹配模型</p>
        <div v-for="g in filtered" :key="g.id" class="ms-group">
          <p class="ms-group-name">
            <img v-if="groupLogo(g.id)" :src="groupLogo(g.id) || ''" class="ms-group-logo" alt="" />
            {{ g.name }}
          </p>
          <ul>
            <li v-for="m in g.models" :key="m">
              <button
                type="button"
                class="ms-item"
                :class="{ active: modelKey(g.id, m) === value }"
                @click="pick(g.id, m)"
              >
                <span v-if="modelKey(g.id, m) === value" class="ms-bar" aria-hidden="true" />
                <span v-if="groupHasItemLogos(g.id)" class="ms-item-logo">
                  <img v-if="itemLogo(g.id, m)" :src="itemLogo(g.id, m) || ''" alt="" />
                </span>
                <span class="ms-item-text" :title="m">{{ stripPrefix(g.id, m) }}</span>
                <el-icon v-if="modelKey(g.id, m) === value" :size="13" class="ms-check"><Check /></el-icon>
              </button>
            </li>
          </ul>
        </div>
      </el-scrollbar>

      <button type="button" class="ms-config" @click="onConfigure">
        <el-icon :size="13"><Setting /></el-icon>
        {{ configureText }}
      </button>
    </div>
  </el-popover>
</template>

<script setup lang="ts">
/**
 * ModelSelector.vue — 模型选择器
 * 职责：以弹出面板展示按端点分组的模型列表，支持搜索过滤、选择模型与跳转配置。
 */
import { computed, nextTick, ref, watch } from 'vue'
import { ArrowDown, Check, Cpu, Search, Setting } from '@element-plus/icons-vue'
import { modelKey, parseModelKey, type ModelOptionGroup } from '@/api/chat'
import { resolveVendorLogo } from '@/components/ui/vendorLogos'

// Props 定义：value 为当前选中的模型 key，groups 为分组模型列表，configureText 为配置按钮文案
const props = withDefaults(
  defineProps<{
    value: string
    groups: ModelOptionGroup[]
    configureText?: string
  }>(),
  { configureText: '去配置端点' },
)

// Emits 定义：update:value 同步选中值，configure 触发跳转配置
const emit = defineEmits<{
  'update:value': [key: string]
  configure: []
}>()

const open = ref(false) // 弹出面板是否展开
const query = ref('') // 搜索关键词
const searchInput = ref<HTMLInputElement | null>(null) // 搜索输入框引用

// 面板展开/收起时重置搜索或延迟聚焦输入框
watch(open, async (v) => {
  if (!v) {
    query.value = ''
    return
  }
  await nextTick()
  // 弹层动画结束后再聚焦，避免焦点被 popper 抢回
  window.setTimeout(() => searchInput.value?.focus(), 60)
})

// 当前选中的模型解析信息（端点 ID、模型名、分组名）
const selected = computed(() => {
  const parsed = parseModelKey(props.value)
  if (!parsed) return null
  const g = props.groups.find((x) => x.id === parsed.endpointId)
  return { ...parsed, groupName: g?.name }
})

/** 网关拉取模型失败时的占位条目形如 `{provider}/models`，需要特殊展示。 */
const isPlaceholder = computed(() => !!selected.value && /\/models$/.test(selected.value.model))

/** 模型 id 带 `{provider}/` 前缀时，展示时剥掉（发送仍用完整 id）。 */
const displayModel = computed(() => {
  const s = selected.value
  if (!s) return ''
  const prefix = `${s.endpointId}/`
  return s.model.startsWith(prefix) ? s.model.slice(prefix.length) : s.model
})

/** 展示用的分组短名（endpointId 剥掉 custom- 前缀）。 */
const groupPrefix = computed(() => {
  const s = selected.value
  if (!s) return ''
  return (s.groupName || s.endpointId).replace(/^custom-/, '')
})

// 触发按钮显示文案
const triggerText = computed(() => {
  if (!selected.value) return '选择模型'
  if (isPlaceholder.value) return '未获取到模型'
  return displayModel.value
})

// 触发按钮 title 提示
const triggerTitle = computed(() => {
  if (!selected.value) return '选择模型'
  if (isPlaceholder.value) return '该提供方模型目录获取失败，请检查订阅的 API 地址后重试'
  return selected.value.model
})

// 按搜索关键词过滤后的分组列表
const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return props.groups
  return props.groups
    .map((g) => ({
      ...g,
      models: g.models.filter((m) => m.toLowerCase().includes(q) || g.name.toLowerCase().includes(q)),
    }))
    .filter((g) => g.models.length > 0)
})

/** 选中某个模型，关闭面板并触发更新事件。 */
function pick(groupId: string, model: string): void {
  open.value = false
  emit('update:value', modelKey(groupId, model))
}

/** 列表里剥掉 `{provider}/` 前缀与 `/models` 占位，仅用于展示。 */
function stripPrefix(groupId: string, model: string): string {
  const prefix = `${groupId}/`
  const bare = model.startsWith(prefix) ? model.slice(prefix.length) : model
  if (bare === 'models') return 'models（获取失败占位）'
  return bare
}

/** 点击配置按钮，关闭面板并触发 configure 事件。 */
function onConfigure(): void {
  open.value = false
  emit('configure')
}

// ===== 厂商 Logo 解析 =====

/** 分组/条目级厂商 Logo 缓存（避免模板中重复解析） */
const logoCache = computed(() => {
  const groupLogos = new Map<string, string | null>()
  const itemLogos = new Map<string, string | null>()
  const hasItemLogos = new Set<string>()
  for (const g of props.groups) {
    const gl = resolveVendorLogo(g.id, g.name)
    groupLogos.set(g.id, gl)
    for (const m of g.models) {
      // 条目 Logo 仅在与分组 Logo 不同时展示（如 OpenRouter 内聚合的多家模型）
      const l = resolveVendorLogo(g.id, m)
      const il = l && l !== gl ? l : null
      itemLogos.set(modelKey(g.id, m), il)
      if (il) hasItemLogos.add(g.id)
    }
  }
  return { groupLogos, itemLogos, hasItemLogos }
})

/** 触发按钮上的厂商 Logo（按当前选中模型解析，未命中回退 Cpu 图标） */
const triggerLogo = computed(() => {
  const s = selected.value
  if (!s) return null
  return resolveVendorLogo(s.endpointId, s.model, s.groupName)
})

/** 分组标题 Logo */
function groupLogo(groupId: string): string | null {
  return logoCache.value.groupLogos.get(groupId) ?? null
}

/** 模型条目 Logo（与分组 Logo 相同时返回 null） */
function itemLogo(groupId: string, model: string): string | null {
  return logoCache.value.itemLogos.get(modelKey(groupId, model)) ?? null
}

/** 分组内是否存在带独立厂商 Logo 的条目（用于对齐占位） */
function groupHasItemLogos(groupId: string): boolean {
  return logoCache.value.hasItemLogos.has(groupId)
}
</script>

<style scoped>
.ms-trigger {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 200px;
  max-width: 340px;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface);
  color: var(--ink-2);
  font-size: var(--fs-sm);
  transition: border-color 0.12s, background 0.12s, color 0.12s, box-shadow 0.12s;
}
.ms-trigger:hover { border-color: var(--line-2); color: var(--ink); background: var(--surface-2); }
.ms-trigger.placeholder { border-style: dashed; color: var(--warn, #b8860b); }
.ms-trigger.placeholder .ms-trigger-text { color: var(--warn, #b8860b); }
.ms-trigger-icon { flex-shrink: 0; color: var(--ink-4); }
.ms-trigger-logo { width: 16px; height: 16px; flex-shrink: 0; object-fit: contain; }
.ms-group-logo {
  width: 14px;
  height: 14px;
  vertical-align: -2px;
  margin-right: 5px;
  object-fit: contain;
}
.ms-item-logo {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.ms-item-logo img { width: 100%; height: 100%; object-fit: contain; }
.ms-group-chip {
  flex-shrink: 0;
  max-width: 96px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10.5px;
  line-height: 1;
  padding: 3px 7px;
  border-radius: 999px;
  background: var(--surface-3);
  color: var(--ink-3);
}
.ms-trigger-text {
  flex: 1;
  min-width: 0;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 12.5px;
}
.ms-caret { flex-shrink: 0; color: var(--ink-4); transition: transform 0.15s; }
.ms-caret.flipped { transform: rotate(180deg); }

.ms-panel { display: flex; flex-direction: column; }
.ms-search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  border-bottom: 1px solid var(--line);
  color: var(--ink-4);
}
.ms-search-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: var(--fs-sm);
  color: var(--ink);
}
.ms-search-input::placeholder { color: var(--ink-4); }

.ms-list { padding: 4px 0; }
.ms-empty { margin: 0; padding: 22px 8px; text-align: center; font-size: var(--fs-sm); color: var(--ink-4); }

.ms-group-name {
  margin: 0;
  padding: 5px 10px 3px;
  font-size: var(--fs-xs);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--ink-4);
}
.ms-group ul { list-style: none; margin: 0; padding: 0; }

.ms-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  background: transparent;
  text-align: left;
  font-size: var(--fs-body);
  color: var(--ink-2);
  transition: background 0.1s, color 0.1s;
}
.ms-item:hover { background: var(--surface-3); color: var(--ink); }
.ms-item.active { background: var(--accent-bg); color: var(--accent-ink); }
html.dark .ms-item.active { color: var(--ink); }
.ms-bar {
  position: absolute;
  left: 0;
  top: 4px;
  bottom: 4px;
  width: 2px;
  border-radius: 0 2px 2px 0;
  background: var(--accent);
}
.ms-item-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ms-check { flex-shrink: 0; }

.ms-config {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 10px;
  border: none;
  border-top: 1px solid var(--line);
  background: transparent;
  font-size: var(--fs-sm);
  color: var(--ink-3);
  transition: background 0.1s, color 0.1s;
}
.ms-config:hover { background: var(--surface-3); color: var(--ink); }
</style>

<!-- 弹层被 teleport 到 body，需非 scoped 覆盖 Element Plus 内边距 -->
<style>
.ms-popper.el-popover.el-popper { padding: 0; }
</style>
