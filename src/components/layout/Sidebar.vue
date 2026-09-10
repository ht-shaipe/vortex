<template>
  <el-aside width="220px" class="vortex-sidebar" :collapse="collapsed" :collapse-transition="true">
    <div class="sidebar-brand" @click="collapsed = !collapsed">
      <span class="brand-icon">⚡</span>
      <span v-if="!collapsed" class="brand-text">Vortex</span>
    </div>

    <el-menu
      :default-active="activeRoute"
      :collapse="collapsed"
      :collapse-transition="true"
      background-color="transparent"
      text-color="#a1a1aa"
      active-text-color="#e54d5e"
      router
    >
      <el-menu-item index="/">
        <el-icon><Odometer /></el-icon>
        <template #title>Dashboard</template>
      </el-menu-item>
      <el-menu-item index="/providers">
        <el-icon><Connection /></el-icon>
        <template #title>Providers</template>
      </el-menu-item>
      <el-menu-item index="/combos">
        <el-icon><Switch /></el-icon>
        <template #title>Combos</template>
      </el-menu-item>
      <el-menu-item index="/keys">
        <el-icon><Key /></el-icon>
        <template #title>API Keys</template>
      </el-menu-item>
      <el-menu-item index="/usage">
        <el-icon><DataLine /></el-icon>
        <template #title>Usage</template>
      </el-menu-item>
      <el-menu-item index="/settings">
        <el-icon><Setting /></el-icon>
        <template #title>Settings</template>
      </el-menu-item>
    </el-menu>

    <div class="sidebar-footer">
      <div class="endpoint-info" v-if="!collapsed">
        <span class="endpoint-label">Endpoint</span>
        <code class="endpoint-url">localhost:20128/v1</code>
      </div>
    </div>
  </el-aside>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute } from 'vue-router'
import { Odometer, Connection, Switch, Key, DataLine, Setting } from '@element-plus/icons-vue'

const collapsed = ref(false)
const route = useRoute()
const activeRoute = computed(() => route.path)
</script>

<style scoped>
.vortex-sidebar {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

.sidebar-brand {
  display: flex;
  align-items: center;
  padding: 20px 16px;
  cursor: pointer;
  user-select: none;
}

.brand-icon {
  font-size: 24px;
  margin-right: 10px;
}

.brand-text {
  font-size: 20px;
  font-weight: 700;
  background: linear-gradient(135deg, #e54d5e, #a855f7);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

.sidebar-footer {
  margin-top: auto;
  padding: 12px 16px;
  border-top: 1px solid var(--color-border);
}

.endpoint-label {
  font-size: 11px;
  color: var(--color-text-muted);
  display: block;
  margin-bottom: 4px;
}

.endpoint-url {
  font-size: 12px;
  color: var(--color-accent);
  background: var(--color-surface-2);
  padding: 4px 8px;
  border-radius: 4px;
  font-family: ui-monospace, monospace;
}
</style>
