<template>
  <el-container class="app-layout">
    <Sidebar />
    <el-container direction="vertical">
      <Header />
      <el-main class="vortex-content">
        <router-view />
      </el-main>
    </el-container>
  </el-container>
</template>

<script setup lang="ts">
import Sidebar from './Sidebar.vue'
import Header from './Header.vue'
</script>

<style scoped>
.app-layout {
  height: 100vh;
  overflow: hidden;
}
</style>
