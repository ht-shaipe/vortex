<template>
  <el-dialog
    :model-value="visible"
    @update:model-value="$emit('update:visible', $event)"
    title="选择模型"
    width="680"
    :close-on-click-modal="false"
    class="model-select-dialog"
  >
    <!-- 顶部工具栏：搜索 + 全选/反选 + 计数 -->
    <div class="dialog-toolbar">
      <el-input
        v-model="searchKey"
        placeholder="搜索模型名称…"
        clearable
        size="small"
        class="search-input"
      >
        <template #prefix>
          <el-icon :size="14"><Search /></el-icon>
        </template>
      </el-input>
      <div class="toolbar-right">
        <span class="count-text">
          已选 <strong>{{ checkedCount }}</strong> / {{ models.length }}
        </span>
        <button type="button" class="btn sm" @click="toggleAll">
          {{ isAllChecked ? '取消全选' : '全选' }}
        </button>
      </div>
    </div>

    <!-- 加载中 -->
    <div v-if="loading" class="loading-wrap">
      <el-icon class="spin" :size="24"><Loading /></el-icon>
      <p>正在从远程加载模型列表…</p>
    </div>

    <!-- 空状态 -->
    <div v-else-if="models.length === 0" class="empty-wrap">
      <p>未获取到任何模型，请先点击「获取可用模型」。</p>
    </div>

    <!-- 模型 checkbox 列表 -->
    <el-scrollbar v-else class="model-scroll" max-height="420">
      <div class="model-grid">
        <label
          v-for="m in filteredModels"
          :key="m"
          class="model-item"
          :class="{ checked: isChecked(m) }"
        >
          <el-checkbox :model-value="isChecked(m)" @change="toggleModel(m)" />
          <span class="model-label" :title="m">{{ m }}</span>
        </label>
      </div>
      <div v-if="filteredModels.length === 0" class="no-match">
        未找到匹配「{{ searchKey }}」的模型
      </div>
    </el-scrollbar>

    <!-- 底部操作 -->
    <template #footer>
      <div class="dialog-footer">
        <span v-if="checkedCount > 0" class="footer-hint">
          确认后选中的模型将显示在主界面，首个作为默认模型
        </span>
        <span class="spacer" />
        <button type="button" class="btn" @click="$emit('update:visible', false)">取消</button>
        <button type="button" class="btn primary" :disabled="checkedCount === 0" @click="confirm">
          确认选择 ({{ checkedCount }})
        </button>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * ModelSelectDialog.vue — 模型多选对话框
 * 职责：以 checkbox 形式展示远程加载的模型列表，支持搜索过滤、全选/反选、多选确认。
 */
import { computed, ref, watch } from 'vue'
import { Search, Loading } from '@element-plus/icons-vue'

const props = defineProps<{
  visible: boolean
  models: string[]
  selected: string[]
  loading?: boolean
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  confirm: [models: string[]]
}>()

const searchKey = ref('')
const localChecked = ref<Set<string>>(new Set())

// 对话框打开时同步已选状态
watch(
  () => props.visible,
  (v) => {
    if (v) {
      localChecked.value = new Set(props.selected)
      searchKey.value = ''
    }
  },
)

// 搜索过滤后的模型列表
const filteredModels = computed(() => {
  const key = searchKey.value.trim().toLowerCase()
  if (!key) return props.models
  return props.models.filter((m) => m.toLowerCase().includes(key))
})

// 已选数量
const checkedCount = computed(() => localChecked.value.size)

// 是否全选
const isAllChecked = computed(
  () => props.models.length > 0 && localChecked.value.size === props.models.length,
)

/** 判断模型是否已选 */
function isChecked(m: string): boolean {
  return localChecked.value.has(m)
}

/** 切换模型选中状态 */
function toggleModel(m: string): void {
  const next = new Set(localChecked.value)
  if (next.has(m)) {
    next.delete(m)
  } else {
    next.add(m)
  }
  localChecked.value = next
}

/** 全选 / 取消全选 */
function toggleAll(): void {
  if (isAllChecked.value) {
    localChecked.value = new Set()
  } else {
    localChecked.value = new Set(props.models)
  }
}

/** 确认选择，保持已有模型的顺序，新选模型追加到末尾 */
function confirm(): void {
  const ordered: string[] = []
  for (const m of props.selected) {
    if (localChecked.value.has(m)) {
      ordered.push(m)
    }
  }
  for (const m of props.models) {
    if (localChecked.value.has(m) && !ordered.includes(m)) {
      ordered.push(m)
    }
  }
  emit('confirm', ordered)
  emit('update:visible', false)
}
</script>

<style scoped>
.dialog-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}
.search-input {
  flex: 1;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}
.count-text {
  font-size: 12px;
  color: var(--ink-3);
  white-space: nowrap;
}
.count-text strong {
  color: var(--accent);
}

.loading-wrap,
.empty-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 48px 0;
  color: var(--ink-3);
  font-size: 13px;
}
.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

.model-scroll {
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}
.model-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 2px;
  padding: 8px;
}
.model-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: background 0.12s;
}
.model-item:hover {
  background: var(--surface-2);
}
.model-item.checked {
  background: var(--accent-bg, rgba(99, 102, 241, 0.08));
}
.model-label {
  font-size: 12.5px;
  color: var(--ink-2);
  font-family: var(--font-mono, monospace);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.no-match {
  padding: 32px;
  text-align: center;
  font-size: 13px;
  color: var(--ink-4);
}

.dialog-footer {
  display: flex;
  align-items: center;
  gap: 10px;
}
.spacer { flex: 1; }
.footer-hint {
  font-size: 12px;
  color: var(--ink-4);
}

.btn {
  padding: 7px 16px;
  border-radius: var(--r-sm);
  font-size: 13px;
  border: 1px solid var(--line);
  background: var(--surface-2);
  color: var(--ink-1);
  cursor: pointer;
  transition: all 0.15s;
}
.btn:hover:not(:disabled) {
  border-color: var(--ink-4);
}
.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.btn.primary:hover:not(:disabled) {
  filter: brightness(1.1);
}
.btn.sm {
  padding: 4px 12px;
  font-size: 12px;
}

@media (max-width: 600px) {
  .model-grid {
    grid-template-columns: 1fr;
  }
}
</style>
