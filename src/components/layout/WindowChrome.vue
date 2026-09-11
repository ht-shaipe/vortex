<template>
  <!-- 顶部拖拽区：纯 JS 闭包方式实现，每次拖动独立状态，避免拖一次后失效 -->
  <div class="window-chrome" @mousedown="onDragStart" data-tauri-drag-region />
</template>

<script setup lang="ts">
/**
 * WindowChrome.vue — 窗口拖拽区域
 * 职责：手动实现窗口拖动逻辑。
 * 关键：使用闭包变量，每次 mousedown 创建独立的 onMove/onUp 函数，
 * 不使用模块级状态，避免"拖一次后第二次失效"的问题。
 */
import { getCurrentWindow, type PhysicalPosition } from '@tauri-apps/api/window'

/** 顶部拖拽区高度 */
const DRAG_HEIGHT = 38

/** mousedown：开始拖动，使用闭包变量，每次独立 */
function onDragStart(e: MouseEvent): void {
  if (e.button !== 0) return
  if (e.clientY > DRAG_HEIGHT) return
  // 点在可交互元素上时不拖动
  const target = e.target as HTMLElement
  if (target.closest('button, input, select, textarea, a, [role="button"]')) return

  e.preventDefault()
  e.stopPropagation()

  // 闭包变量：每次拖动独立，不污染模块级状态
  const startMouseX = e.screenX
  const startMouseY = e.screenY
  let startWinX = 0
  let startWinY = 0
  let positionReady = false
  let rafId = 0
  let pendingX = 0
  let pendingY = 0
  let hasPending = false

  // 闭包函数：每次拖动独立的引用，确保 removeEventListener 能正确匹配
  const onMove = (ev: MouseEvent) => {
    if (!positionReady) return
    ev.preventDefault()
    pendingX = startWinX + (ev.screenX - startMouseX)
    pendingY = startWinY + (ev.screenY - startMouseY)
    hasPending = true
    if (!rafId) {
      rafId = requestAnimationFrame(() => {
        rafId = 0
        if (hasPending) {
          hasPending = false
          getCurrentWindow()
            .setPosition(new PhysicalPosition(pendingX, pendingY))
            .catch(() => {})
        }
      })
    }
  }

  const onUp = () => {
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    if (rafId) {
      cancelAnimationFrame(rafId)
      rafId = 0
    }
  }

  // 同步添加事件监听器（关键：不能在 await 之后添加）
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)

  // 异步获取窗口当前位置，不阻塞事件监听
  getCurrentWindow()
    .outerPosition()
    .then((pos) => {
      startWinX = pos.x
      startWinY = pos.y
      positionReady = true
    })
    .catch(() => {
      positionReady = true
    })
}
</script>

<style scoped>
.window-chrome {
  width: 100%;
  height: 38px;
  flex-shrink: 0;
  user-select: none;
  -webkit-user-select: none;
  background: transparent;
}
</style>
