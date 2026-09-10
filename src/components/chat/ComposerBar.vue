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
        placeholder="输入消息…"
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
/**
 * ComposerBar.vue — 输入框
 * 职责：提供消息输入区域，支持拖拽调整高度、展开/收起、IME 组合输入处理与发送/中止操作。
 */
import { computed, ref, watch } from 'vue'
import { Expand, Fold, Top, VideoPause } from '@element-plus/icons-vue'
import { useChatLayoutStore, COMPOSER_EXPAND_RATIO, COMPOSER_MAX_PX, COMPOSER_MIN_PX, clampComposerHeight, shellFromContent } from '@/stores/chatLayout'

// Props 定义：value 为输入文本，busy 标识生成中，disabled 禁用输入，columnEl 为展开高度参考容器
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

// Emits 定义：update:value 同步输入文本，send 发送消息，abort 中止生成
const emit = defineEmits<{
  'update:value': [v: string]
  send: []
  abort: []
}>()

const layout = useChatLayoutStore() // 布局持久化 store

/** 输入框是否处于展开态（模板引用；来源为布局 store 的持久化状态）。 */
const expanded = computed(() => layout.composerExpanded)

// 输入草稿（双向绑定到 props.value）
const draft = computed({
  get: () => props.value,
  set: (v: string) => emit('update:value', v),
})

const cardRef = ref<HTMLDivElement | null>(null) // 输入框容器引用
const taRef = ref<HTMLTextAreaElement | null>(null) // textarea 引用
const composing = ref(false) // 是否处于 IME 组合输入中
const gripHover = ref(false) // 拖拽条是否悬停
const cornerHover = ref(false) // 右上角按钮区是否悬停

const dragH = ref<number | null>(null) // 拖拽过程中的实时高度（非拖拽时为 null）
const columnH = ref(0) // 主列容器高度（用于计算展开高度）
const dragRef = ref<{ pointerId: number; startY: number; startH: number } | null>(null) // 拖拽会话状态

let ro: ResizeObserver | null = null // 容器尺寸观察器
// 监听主列容器变化，更新高度并建立 ResizeObserver
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

const expandH = computed(() => Math.round(Math.max(columnH.value, 320) * COMPOSER_EXPAND_RATIO)) // 展开态高度
const shellPx = computed(() =>
  layout.composerExpanded ? expandH.value : (dragH.value ?? clampComposerHeight(layout.composerHeightPx)),
) // 当前实际高度（展开 > 拖拽中 > 持久化值）
const canSend = computed(() => !!props.value.trim() && !props.disabled && !props.busy) // 是否可发送

/** 拖拽开始：记录起点与初始高度，捕获指针事件。 */
function onPointerDown(e: PointerEvent): void {
  if (layout.composerExpanded) return
  e.preventDefault()
  const startH = cardRef.value?.offsetHeight ?? shellPx.value
  dragRef.value = { pointerId: e.pointerId, startY: e.clientY, startH }
  dragH.value = startH
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}

/** 拖拽移动：根据指针位移更新实时高度。 */
function onPointerMove(e: PointerEvent): void {
  const d = dragRef.value
  if (!d || d.pointerId !== e.pointerId) return
  dragH.value = clampComposerHeight(d.startH + (d.startY - e.clientY), COMPOSER_MAX_PX)
}

/** 拖拽结束：结算最终高度并持久化。 */
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

/** 指针抬起时结束拖拽。 */
function onPointerUp(e: PointerEvent): void {
  endDrag(e)
}
/** 指针取消时结束拖拽。 */
function onPointerCancel(e: PointerEvent): void {
  endDrag(e)
}

/** 切换展开/收起状态。 */
function toggleExpand(): void {
  if (layout.composerExpanded) {
    layout.setComposerExpanded(false)
    layout.setComposerHeightPx(COMPOSER_MIN_PX)
  } else {
    layout.setComposerExpanded(true)
  }
  cornerHover.value = false
}

/** 发送消息（满足条件时触发 send 事件）。 */
function doSend(): void {
  if (!canSend.value) return
  emit('send')
}

/** IME 组合输入结束：延迟一拍解锁，避免中文回车直接发出。 */
function onCompositionEnd(): void {
  // IME 结束后一拍再解锁，避免中文回车直接发出
  window.setTimeout(() => {
    composing.value = false
  }, 0)
}

/** 键盘事件处理：Enter 发送，Shift+Enter 换行。 */
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
.composer:focus-within {
  border-color: var(--accent-line, var(--line-2));
  box-shadow: 0 0 0 3px var(--accent-bg, rgba(0, 0, 0, 0.03));
}

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
  padding: 6px 14px 2px;
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
  padding: 2px 10px 10px 14px;
}
.foot-hint {
  font-size: 11px;
  color: var(--ink-4);
  letter-spacing: 0.01em;
}

.send-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--line-2);
  border-radius: 50%;
  background: var(--surface);
  color: var(--ink-2);
  transition: background 0.12s, color 0.12s, opacity 0.12s, transform 0.12s;
  flex-shrink: 0;
}
.send-btn:hover:not(:disabled) { background: var(--surface-3); color: var(--ink); }
.send-btn.primary { background: var(--accent); border-color: transparent; color: #fff; }
.send-btn.primary:hover:not(:disabled) { opacity: 0.92; transform: translateY(-1px); }
.send-btn:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
