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
import './styles/markdown.css'
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

// 全局禁用鼠标右键菜单（桌面应用无需 webview 默认右键菜单）
document.addEventListener('contextmenu', (e) => e.preventDefault())

// 将应用挂载到 #app 节点
app.mount('#app')

