<template>
  <div class="app">
    <WindowChrome />
    <Sidebar />
    <main class="main" :class="{ flush }">
      <router-view v-if="flush" />
      <el-scrollbar v-else>
        <div class="main-inner">
          <router-view />
        </div>
      </el-scrollbar>
    </main>
  </div>
</template>

<script setup lang="ts">
/**
 * AppLayout.vue — 应用整体布局
 * 职责：组合窗口控制条、侧边栏与主内容区，根据路由决定主内容区是否贴边渲染（flush）。
 */
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import Sidebar from './Sidebar.vue'
import WindowChrome from './WindowChrome.vue'

// 需要贴边（无内边距滚动条）的路由前缀
const FLUSH_ROUTES = ['/live-routing', '/subscriptions/', '/chat']
// 即使命中贴边路由也需排除的例外路径前缀
const FLUSH_EXCEPTIONS = ['/subscriptions/new']

const route = useRoute() // 当前路由对象
// 是否贴边渲染：命中贴边路由且不在例外列表中时为 true
const flush = computed(
  () =>
    !FLUSH_EXCEPTIONS.some((p) => route.path.startsWith(p)) &&
    FLUSH_ROUTES.some((p) => route.path.startsWith(p)),
)
</script>
