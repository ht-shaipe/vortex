<template>
  <div class="app">
    <WindowChrome />
    <Sidebar />
    <main class="main" :class="{ flush }">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import Sidebar from './Sidebar.vue'
import WindowChrome from './WindowChrome.vue'

const FLUSH_ROUTES = ['/live-routing', '/subscriptions/']
const FLUSH_EXCEPTIONS = ['/subscriptions/new']

const route = useRoute()
const flush = computed(
  () =>
    !FLUSH_EXCEPTIONS.some((p) => route.path.startsWith(p)) &&
    FLUSH_ROUTES.some((p) => route.path.startsWith(p)),
)
</script>