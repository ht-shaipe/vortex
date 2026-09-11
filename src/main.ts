/**
 * Vue 应用入口文件。
 * 负责创建 Vue 实例，并全局注册 Pinia 状态管理、Vue Router 路由、
 * Element Plus 组件库及其图标，最后将应用挂载到 DOM。
 */
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import App from './App.vue'
import router from './router'
import 'virtual:uno.css'
import './styles/theme.css'
import './styles/components.css'
import '@/composables/useTheme'

// 创建 Vue 应用实例与 Pinia 实例
const app = createApp(App)
const pinia = createPinia()

// 注册 Pinia 状态管理
app.use(pinia)
// 注册路由
app.use(router)
// 注册 Element Plus，统一组件尺寸为 default
app.use(ElementPlus, { size: 'default' })

// 全局注册 Element Plus 图标组件
for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component)
}

// 将应用挂载到 #app 节点
app.mount('#app')

// ==== TEMP DEBUG: 排查 data-tauri-drag-region 二次拖动失效 ====
// mousedown/mouseup 的 detail 与命中元素经 IPC 命令写入应用日志（/tmp/vortex-tauri-dev.log）。
// 用 capture 阶段，确保在 drag.js（document 冒泡阶段，含 stopImmediatePropagation）之前执行。
if ('__TAURI_INTERNALS__' in window) {
  const report = (kind: string, e: MouseEvent) => {
    const path = e
      .composedPath()
      .slice(0, 4)
      .map((el) =>
        el instanceof HTMLElement
          ? `${el.tagName}${el.hasAttribute('data-tauri-drag-region') ? '[DR]' : ''}`
          : `#${(el as Record<string, string>).nodeName ?? 'node'}`,
      )
      .join('>')
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ;(window as any).__TAURI_INTERNALS__.invoke('debug_log', {
      message: `${kind} detail=${e.detail} btn=${e.button} ts=${Date.now() % 100000} path=${path}`,
    }).catch(() => {})
  }
  document.addEventListener('mousedown', (e) => report('down', e), true)
  document.addEventListener('mouseup', (e) => report('up', e), true)

  // 决定性实验：在满足 drag.js 条件的按下时，直接调用 start_dragging 并记录结果
  // - Promise resolve → 命令执行了，失败在 tao 原生层（currentEvent 竞态）
  // - Promise reject  → 权限/ACL 拒绝，错误信息会显示原因
  document.addEventListener(
    'mousedown',
    (e) => {
      const target = e.composedPath()[0]
      if (!(target instanceof HTMLElement)) return
      if (!target.hasAttribute('data-tauri-drag-region')) return
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ;(window as any).__TAURI_INTERNALS__
        .invoke('plugin:window|start_dragging')
        .then(() => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          ;(window as any).__TAURI_INTERNALS__.invoke('debug_log', {
            message: `sd OK ts=${Date.now() % 100000}`,
          })
        })
        .catch((err: unknown) => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          ;(window as any).__TAURI_INTERNALS__.invoke('debug_log', {
            message: `sd ERR ts=${Date.now() % 100000} ${String(err).slice(0, 200)}`,
          })
        })
    },
    true,
  )
}
// ==== TEMP DEBUG END ====
