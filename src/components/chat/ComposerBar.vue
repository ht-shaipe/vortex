<template>
  <div class="composer-wrap">
    <div
      ref="cardRef"
      class="composer"
      :class="{ resizing: dragH !== null }"
      :style="{ height: shellPx + 'px' }"
    >
      <!-- 顶部拖拽条：手往上 = 变高 -->
      <div
        class="composer-grip"
        :class="{ locked: expanded }"
        role="separator"
        aria-orientation="horizontal"
        :aria-label="expanded ? '展开时不可拖拽' : '拖拽调整输入框高度'"
        :title="expanded ? '展开时不可拖拽' : '拖拽调整高度'"
        @pointerenter="!expanded && (gripHover = true)"
        @pointerleave="!dragRef && (gripHover = false)"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="onPointerCancel"
      >
        <span class="grip-bar" :class="{ on: gripHover || dragH !== null }" />
      </div>

      <!-- 右上角展开/收起 -->
      <div class="composer-corner" @pointerenter="cornerHover = true" @pointerleave="cornerHover = false">
        <button
          type="button"
          class="corner-btn"
          :class="{ show: cornerHover }"
          :aria-pressed="expanded"
          :aria-label="expanded ? '收起输入框' : '展开输入框'"
          @click="toggleExpand"
          @blur="cornerHover = false"
        >
          <el-icon :size="12"><component :is="expanded ? Fold : Expand" /></el-icon>
        </button>
      </div>

      <textarea
        ref="taRef"
        v-model="draft"
        class="composer-input"
        rows="1"
        :disabled="disabled"
        placeholder="输入消息，Enter 发送，Shift+Enter 换行"
        @compositionstart="composing = true"
        @compositionend="onCompositionEnd"
        @keydown="onKeydown"
      ></textarea>

      <div class="composer-foot">
        <span v-if="busy" class="foot-hint">生成中…</span>
        <span v-else class="foot-hint">Enter 发送 · Shift+Enter 换行</span>

        <button
          v-if="busy"
          type="button"
          class="send-btn"
          title="停止生成"
          aria-label="停止生成"
          @click="emit('abort')"
        >
          <el-icon :size="13"><VideoPause /></el-icon>
        </button>
        <button
          v-else
          type="button"
          class="send-btn primary"
          :disabled="!canSend"
          title="发送"
          aria-label="发送"
          @click="doSend"
        >
          <el-icon :size="13"><Top /></el-icon>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Expand, Fold, Top, VideoPause } from '@element-plus/icons-vue'
import { useChatLayoutStore, COMPOSER_EXPAND_RATIO, COMPOSER_MAX_PX, COMPOSER_MIN_PX, clampComposerHeight, shellFromContent } from '@/stores/chatLayout'

const props = withDefaults(
  defineProps<{
    value: string
    busy?: boolean
    disabled?: boolean
    /** 主列容器，用于计算展开高度。 */
    columnEl?: HTMLElement | null
  }>(),
  { busy: false, disabled: false, columnEl: null },
)

const emit = defineEmits<{
  'update:value': [v: string]
  send: []
  abort: []
}>()

const layout = useChatLayoutStore()

const draft = computed({
  get: () => props.value,
  set: (v: string) => emit('update:value', v),
})

const cardRef = ref<HTMLDivElement | null>(null)
const taRef = ref<HTMLTextAreaElement | null>(null)
const composing = ref(false)
const gripHover = ref(false)
const cornerHover = ref(false)

const dragH = ref<number | null>(null)
const columnH = ref(0)
const dragRef = ref<{ pointerId: number; startY: number; startH: number } | null>(null)

let ro: ResizeObserver | null = null
watch(
  () => props.columnEl,
  (el) => {
    ro?.disconnect()
    ro = null
    if (!el) return
    columnH.value = el.clientHeight
    ro = new ResizeObserver((entries) => {
      columnH.value = entries[0]?.contentRect.height ?? 0
    })
    ro.observe(el)
  },
  { immediate: true },
)

/**
 * 输入变化时仅在需要时增高；已收缩到下限不会被已有内容顶回去。
 * 与 ccMesh 一致：手动拖小后保持小高度。
 */
watch(
  () => props.value,
  () => {
    const ta = taRef.value
    if (!ta || layout.composerExpanded || dragH.value !== null) return
    const prev = ta.style.height
    ta.style.height = '0px'
    const needed = shellFromContent(ta.scrollHeight)
    ta.style.height = prev
    if (needed > layout.composerHeightPx) layout.setComposerHeightPx(needed)
  },
)

const expandH = computed(() => Math.round(Math.max(columnH.value, 320) * COMPOSER_EXPAND_RATIO))
const shellPx = computed(() =>
  layout.composerExpanded ? expandH.value : (dragH.value ?? clampComposerHeight(layout.composerHeightPx)),
)
const canSend = computed(() => !!props.value.trim() && !props.disabled && !props.busy)

function onPointerDown(e: PointerEvent): void {
  if (layout.composerExpanded) return
  e.preventDefault()
  const startH = cardRef.value?.offsetHeight ?? shellPx.value
  dragRef.value = { pointerId: e.pointerId, startY: e.clientY, startH }
  dragH.value = startH
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}

function onPointerMove(e: PointerEvent): void {
  const d = dragRef.value
  if (!d || d.pointerId !== e.pointerId) return
  dragH.value = clampComposerHeight(d.startH + (d.startY - e.clientY), COMPOSER_MAX_PX)
}

function endDrag(e: PointerEvent): void {
  const d = dragRef.value
  if (!d || d.pointerId !== e.pointerId) return
  // 用事件坐标结算，避免闭包里的 dragH 过期
  const finalH = clampComposerHeight(d.startH + (d.startY - e.clientY), COMPOSER_MAX_PX)
  dragRef.value = null
  dragH.value = null
  layout.setComposerHeightPx(finalH)
  try {
    ;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
  } catch {
    /* 已释放 */
  }
  gripHover.value = false
}

function onPointerUp(e: PointerEvent): void {
  endDrag(e)
}
function onPointerCancel(e: PointerEvent): void {
  endDrag(e)
}

function toggleExpand(): void {
  if (layout.composerExpanded) {
    layout.setComposerExpanded(false)
    layout.setComposerHeightPx(COMPOSER_MIN_PX)
  } else {
    layout.setComposerExpanded(true)
  }
  cornerHover.value = false
}

function doSend(): void {
  if (!canSend.value) return
  emit('send')
}

function onCompositionEnd(): void {
  // IME 结束后一拍再解锁，避免中文回车直接发出
  window.setTimeout(() => {
    composing.value = false
  }, 0)
}

function onKeydown(e: KeyboardEvent): void {
  if (e.isComposing || e.keyCode === 229 || composing.value) return
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    doSend()
  }
}
</script>

<style scoped>
.composer-wrap { padding: 8px 20px 16px; }
.composer {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  max-width: 780px;
  margin: 0 auto;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  background: var(--surface);
  box-shadow: var(--shadow-sm);
  transition: height 0.2s;
}
/* 拖拽时关掉过渡，否则不跟手 */
.composer.resizing { transition: none; }

.composer-grip {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 12px;
  flex-shrink: 0;
  cursor: ns-resize;
}
.composer-grip.locked { cursor: default; }
.grip-bar {
  display: block;
  width: 32px;
  height: 3px;
  border-radius: 999px;
  background: var(--ink-5);
  opacity: 0;
  transition: opacity 0.15s;
}
.grip-bar.on { opacity: 1; }

.composer-corner { position: absolute; top: 0; right: 0; z-index: 2; width: 32px; height: 30px; }
.corner-btn {
  position: absolute;
  top: 6px;
  right: 6px;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 50%;
  background: var(--surface-3);
  color: var(--ink-3);
  opacity: 0;
  transform: translate(6px, -6px) scale(0.75);
  pointer-events: none;
  transition: opacity 0.15s, transform 0.15s, background 0.12s, color 0.12s;
}
.corner-btn.show {
  opacity: 1;
  transform: translate(0, 0) scale(1);
  pointer-events: auto;
}
.corner-btn:hover { color: var(--ink); background: var(--line); }

.composer-input {
  flex: 1;
  min-height: 0;
  width: 100%;
  padding: 4px 12px;
  border: none;
  outline: none;
  resize: none;
  overflow-y: auto;
  background: transparent;
  color: var(--ink);
  font-size: 14px;
  line-height: 1.6;
}
.composer-input::placeholder { color: var(--ink-4); }
.composer-input:disabled { opacity: 0.5; }

.composer-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  padding: 4px 8px 8px 12px;
}
.foot-hint { font-size: 11px; color: var(--ink-4); }

.send-btn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--line-2);
  border-radius: 50%;
  background: var(--surface);
  color: var(--ink-2);
  transition: background 0.12s, color 0.12s, opacity 0.12s;
}
.send-btn:hover { background: var(--surface-3); color: var(--ink); }
.send-btn.primary { background: var(--accent); border-color: transparent; color: #fff; }
.send-btn.primary:hover { opacity: 0.9; background: var(--accent); color: #fff; }
.send-btn:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
