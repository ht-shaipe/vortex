/**
 * Vue Router 路由配置。
 * 定义应用所有页面的路由表，使用懒加载引入各视图组件。
 * 桌面环境使用 history 模式，Web 环境使用 hash 模式。
 */
import { createRouter, createWebHistory, createWebHashHistory } from 'vue-router'
import { runtime } from '@/lib/runtime'

const router = createRouter({
  // Web 环境使用 hash 路由，桌面环境使用 history 路由
  history: runtime.kind === 'web' ? createWebHashHistory() : createWebHistory(),
  routes: [
    {
      // 托盘状态面板：独立布局，无边框透明窗口
      path: '/status-panel',
      name: 'status-panel',
      component: () => import('@/views/StatusPanel.vue'),
    },
    {
      // 根布局，所有页面作为子路由渲染在 AppLayout 内
      path: '/',
      component: () => import('@/components/layout/AppLayout.vue'),
      children: [
        // 默认重定向到实时路由页
        { path: '', redirect: '/live-routing' },
        // 使用引导页
        { path: 'guide', name: 'guide', component: () => import('@/views/Guide.vue') },
        // 实时路由页
        { path: 'live-routing', name: 'live-routing', component: () => import('@/views/LiveRouting.vue') },
        // 订阅管理列表页
        { path: 'subscriptions', name: 'subscriptions', component: () => import('@/views/Subscriptions.vue') },
        // 新建订阅页
        { path: 'subscriptions/new', name: 'subscriptions-new', component: () => import('@/views/SubscriptionNew.vue') },
        // 自定义订阅页
        { path: 'subscriptions/custom', name: 'subscriptions-custom', component: () => import('@/views/SubscriptionCustom.vue') },
        // 编辑订阅页（动态参数 id）
        { path: 'subscriptions/:id', name: 'subscriptions-edit', component: () => import('@/views/SubscriptionEdit.vue') },
        // 免费 Token 页
        { path: 'free-tokens', name: 'free-tokens', component: () => import('@/views/FreeTokens.vue') },
        // 请求日志页
        { path: 'request-logs', name: 'request-logs', component: () => import('@/views/RequestLogs.vue') },
        // 统计分析页
        { path: 'statistics', name: 'statistics', component: () => import('@/views/Statistics.vue') },
        // 数据同步页
        { path: 'sync', name: 'sync', component: () => import('@/views/Sync.vue') },
        // 聊天对话页
        { path: 'chat', name: 'chat', component: () => import('@/views/Chat.vue') },


        // 设置页
        { path: 'settings', name: 'settings', component: () => import('@/views/Settings.vue') },
        // 关于页
        { path: 'about', name: 'about', component: () => import('@/views/About.vue') },
      ],
    },
    // 未匹配路由重定向到首页
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router
