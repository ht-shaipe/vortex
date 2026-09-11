<template>
  <div class="app" :class="{ 'sidebar-collapsed': collapsed }">
    <Sidebar
      :collapsed="collapsed"
      :collapsible="true"
      :active="route.path"
      @toggle="collapsed = !collapsed"
    />
    <main class="main" :class="{ flush }">
      <!-- 拖拽区：作为布局流中的实际头部元素，参照 dsa 项目实现 -->
      <WindowChrome />
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
 * AppLayout.vue — 应用根布局
 * 职责：组合侧边栏与主内容区，管理侧边栏折叠状态。
 * 窗口拖动由 main 区域顶部的 WindowChrome 组件处理，作为布局流中的实际头部元素。
 */
import { computed, ref } from 'vue'
import { useRoute } from 'vue-router'
import Sidebar from './Sidebar.vue'
import WindowChrome from './WindowChrome.vue'

const route = useRoute()
const collapsed = ref(false)
/** 通栏页面（如 /chat、实时路由、订阅编辑）自管理布局与滚动，直接铺满主区域 */
const FLUSH_PATHS = ['/chat', '/live-routing']
const NON_FLUSH_SUB = ['/subscriptions/new', '/subscriptions/custom']
const flush = computed(() => {
  if (FLUSH_PATHS.includes(route.path)) return true
  if (!route.path.startsWith('/subscriptions/')) return false
  return !NON_FLUSH_SUB.includes(route.path)
})
</script>

<style scoped>
.app {
  display: grid;
  grid-template-columns: auto 1fr;
  height: 100vh;
  overflow: hidden;
}
.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  height: 100vh;
  overflow: hidden;
}
.main.flush {
  padding: 0;
}
</style>
