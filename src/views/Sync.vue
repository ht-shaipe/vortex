<template>
  <div class="sync-page">
    <!-- 页面标题 -->
    <h1 class="page-title">同步</h1>

    <!-- 各同步能力区块：CC Switch 导入、WebDAV、远程备份列表、本地备份 -->
    <CcSwitchImport />
    <WebdavForm />
    <BackupList />
    <LocalBackup />
  </div>
</template>

<script setup lang="ts">
/**
 * 同步页面。
 * 职责：作为同步能力的容器，依次渲染 CC Switch 导入、WebDAV 配置、
 * 远程备份列表与本地备份四个子区块，具体逻辑由各子组件实现。
 */
import CcSwitchImport from '@/components/sync/CcSwitchImport.vue'
import WebdavForm from '@/components/sync/WebdavForm.vue'
import BackupList from '@/components/sync/BackupList.vue'
import LocalBackup from '@/components/sync/LocalBackup.vue'
</script>

<style scoped>
.sync-page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-lg);
  width: 100%;
  max-width: 780px;
  margin: 0 auto;
}
.page-title {
  margin: 0;
  font-size: var(--fs-h1);
  font-weight: 300;
  letter-spacing: -0.02em;
}
</style>

<!-- 同步页各区块的公共外壳：子组件共用，故非 scoped -->
<style>
.sync-card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
  padding: var(--pad-card);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  background: var(--surface);
  box-shadow: var(--shadow-sm);
}
.sync-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--gap-md);
}
.sync-title { margin: 0; font-size: var(--fs-body); font-weight: 600; color: var(--ink); }
.sync-desc { margin: 0; font-size: var(--fs-sm); color: var(--ink-3); line-height: 1.6; }
</style>
