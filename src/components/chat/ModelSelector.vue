<template>
  <el-popover
    v-model:visible="open"
    placement="bottom-end"
    :width="340"
    trigger="click"
    popper-class="ms-popper"
  >
    <template #reference>
      <button type="button" class="ms-trigger" :title="selected?.model || '选择模型'">
        <el-icon :size="14" class="ms-trigger-icon"><Cpu /></el-icon>
        <span class="ms-trigger-text">{{ selected?.model || '选择模型' }}</span>
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
          <p class="ms-group-name">{{ g.name }}</p>
          <ul>
            <li v-for="m in g.models" :key="m">
              <button
                type="button"
                class="ms-item"
                :class="{ active: modelKey(g.id, m) === value }"
                @click="pick(g.id, m)"
              >
                <span v-if="modelKey(g.id, m) === value" class="ms-bar" aria-hidden="true" />
                <span class="ms-item-text">{{ m }}</span>
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
import { computed, nextTick, ref, watch } from 'vue'
import { ArrowDown, Check, Cpu, Search, Setting } from '@element-plus/icons-vue'
import { modelKey, parseModelKey, type ModelOptionGroup } from '@/api/chat'

const props = withDefaults(
  defineProps<{
    value: string
    groups: ModelOptionGroup[]
    configureText?: string
  }>(),
  { configureText: '去配置端点' },
)

const emit = defineEmits<{
  'update:value': [key: string]
  configure: []
}>()

const open = ref(false)
const query = ref('')
const searchInput = ref<HTMLInputElement | null>(null)

watch(open, async (v) => {
  if (!v) {
    query.value = ''
    return
  }
  await nextTick()
  // 弹层动画结束后再聚焦，避免焦点被 popper 抢回
  window.setTimeout(() => searchInput.value?.focus(), 60)
})

const selected = computed(() => {
  const parsed = parseModelKey(props.value)
  if (!parsed) return null
  const g = props.groups.find((x) => x.id === parsed.endpointId)
  return { ...parsed, groupName: g?.name }
})

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

function pick(groupId: string, model: string): void {
  open.value = false
  emit('update:value', modelKey(groupId, model))
}

function onConfigure(): void {
  open.value = false
  emit('configure')
}
</script>

<style scoped>
.ms-trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 180px;
  max-width: 320px;
  height: 30px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface);
  color: var(--ink-2);
  font-size: var(--fs-sm);
  transition: border-color 0.12s, background 0.12s, color 0.12s;
}
.ms-trigger:hover { border-color: var(--line-2); color: var(--ink); background: var(--surface-2); }
.ms-trigger-icon { flex-shrink: 0; color: var(--ink-4); }
.ms-trigger-text {
  flex: 1;
  min-width: 0;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
