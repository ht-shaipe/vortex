<template>
  <section class="sync-card">
    <div class="sync-head">
      <h2 class="sync-title">云端备份</h2>
      <button type="button" class="btn sm" :disabled="backing" @click="onBackup">
        <el-icon :size="13"><Upload /></el-icon>{{ backing ? '备份中…' : '立即备份' }}
      </button>
    </div>

    <p v-if="loadError" class="sync-desc err">无法连接 WebDAV：{{ loadError }}。请先在上方配置并保存。</p>
    <p v-else-if="!loaded" class="sync-desc">读取中…</p>
    <p v-else-if="list.length === 0" class="sync-desc">暂无备份</p>

    <div v-else class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th>文件</th>
            <th class="num" style="width: 100px">大小</th>
            <th style="width: 170px">时间</th>
            <th style="width: 90px; text-align: right">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in list" :key="b.filename">
            <td class="mono">{{ b.filename }}</td>
            <td class="num">{{ (b.size / 1024).toFixed(1) }} KB</td>
            <td class="num">{{ new Date(b.modTime).toLocaleString('zh-CN') }}</td>
            <td>
              <div class="ops">
                <button
                  type="button"
                  class="btn bare icon"
                  title="恢复"
                  aria-label="恢复"
                  @click="onRestore(b.filename)"
                >
                  <el-icon :size="14"><Download /></el-icon>
                </button>
                <button
                  type="button"
                  class="btn bare icon"
                  title="删除"
                  aria-label="删除"
                  @click="onDelete(b.filename)"
                >
                  <el-icon :size="14"><Delete /></el-icon>
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, Delete, Upload } from '@element-plus/icons-vue'
import { webdavApi, errMsg, type BackupFile } from '@/api/sync'

const list = ref<BackupFile[]>([])
const loaded = ref(false)
const loadError = ref('')
const backing = ref(false)

async function load(): Promise<void> {
  loadError.value = ''
  try {
    list.value = await webdavApi.listBackups()
    loaded.value = true
  } catch (e) {
    loadError.value = errMsg(e)
    loaded.value = true
  }
}
void load()

async function onBackup(): Promise<void> {
  backing.value = true
  try {
    const name = await webdavApi.backup()
    ElMessage.success(`已备份：${name}`)
    await load()
  } catch (e) {
    ElMessage.error(`备份失败：${errMsg(e)}`)
  } finally {
    backing.value = false
  }
}

async function onRestore(filename: string): Promise<void> {
  try {
    await webdavApi.restore(filename)
    ElMessage.success('恢复完成')
  } catch (e) {
    ElMessage.error(`恢复失败：${errMsg(e)}`)
  }
}

async function onDelete(filename: string): Promise<void> {
  try {
    await webdavApi.deleteBackup(filename)
    ElMessage.success('已删除')
    await load()
  } catch (e) {
    ElMessage.error(`删除失败：${errMsg(e)}`)
  }
}
</script>

<style scoped>
.table-wrap { border: 1px solid var(--line); border-radius: var(--r-md); overflow: hidden; }
.ops { display: flex; justify-content: flex-end; gap: 2px; }
.err { color: var(--err); }
</style>
