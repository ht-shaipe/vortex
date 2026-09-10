import { createRouter, createWebHistory, createWebHashHistory } from 'vue-router'
import { runtime } from '@/lib/runtime'

const router = createRouter({
  history: runtime.kind === 'web' ? createWebHashHistory() : createWebHistory(),
  routes: [
    {
      path: '/',
      component: () => import('@/components/layout/AppLayout.vue'),
      children: [
        { path: '', redirect: '/live-routing' },
        { path: 'guide', name: 'guide', component: () => import('@/views/Guide.vue') },
        { path: 'live-routing', name: 'live-routing', component: () => import('@/views/LiveRouting.vue') },
        { path: 'subscriptions', name: 'subscriptions', component: () => import('@/views/Subscriptions.vue') },
        { path: 'subscriptions/new', name: 'subscriptions-new', component: () => import('@/views/SubscriptionNew.vue') },
        { path: 'subscriptions/custom', name: 'subscriptions-custom', component: () => import('@/views/SubscriptionCustom.vue') },
        { path: 'subscriptions/:id', name: 'subscriptions-edit', component: () => import('@/views/SubscriptionEdit.vue') },
        { path: 'free-tokens', name: 'free-tokens', component: () => import('@/views/FreeTokens.vue') },
        { path: 'request-logs', name: 'request-logs', component: () => import('@/views/RequestLogs.vue') },
        { path: 'statistics', name: 'statistics', component: () => import('@/views/Statistics.vue') },
        { path: 'sync', name: 'sync', component: () => import('@/views/Sync.vue') },
        { path: 'chat', name: 'chat', component: () => import('@/views/Chat.vue') },


        { path: 'settings', name: 'settings', component: () => import('@/views/Settings.vue') },
        { path: 'about', name: 'about', component: () => import('@/views/About.vue') },
      ],
    },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router