import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      component: () => import('@/components/layout/AppLayout.vue'),
      children: [
        { path: '', name: 'dashboard', component: () => import('@/views/Dashboard.vue') },
        { path: 'providers', name: 'providers', component: () => import('@/views/Providers.vue') },
        { path: 'combos', name: 'combos', component: () => import('@/views/Combos.vue') },
        { path: 'keys', name: 'keys', component: () => import('@/views/ApiKeys.vue') },
        { path: 'usage', name: 'usage', component: () => import('@/views/Usage.vue') },
        { path: 'settings', name: 'settings', component: () => import('@/views/Settings.vue') },
      ],
    },
  ],
})

export default router
