<template>
  <section class="sync-card">
    <div class="sync-head">
      <h2 class="sync-title">本地备份 / 配置迁移</h2>
    </div>
    <p class="sync-desc">
      导出提供商连接与设置为 JSON，用于换机迁移。
      导出的文件可能包含明文 API Key，请妥善保管。
    </p>

    <div class="ops-row">
      <button type="button" class="btn" :disabled="busy" @click="onExport">
        <el-icon :size="13"><Download /></el-icon>导出配置
      </button>

      <div class="import-group">
        <select v-model="strategy" class="strategy-select" aria-label="同名处理策略">
          <option value="skip">跳过同名</option>
          <option value="overwrite">覆盖同名</option>
        </select>
        <button type="button" class="btn" :disabled="busy" @click="onImport">
          <el-icon :size="13"><Upload /></el-icon>导入配置
        </button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, Upload } from '@element-plus/icons-vue'
import { backupApi, errMsg, type ImportStrategy } from '@/api/sync'

const strategy = ref<ImportStrategy>('skip')
const exporting = ref(false)
const importing = ref(false)
const busy = computed(() => exporting.value || importing.value)

async function onExport(): Promise<void> {
  exporting.value = true
  try {
    const name = await backupApi.exportConfig()
    ElMessage.success(`已导出到 ${name}`)
  } catch (e) {
    ElMessage.error(`导出失败：${errMsg(e)}`)
  } finally {
    exporting.value = false
  }
}

async function onImport(): Promise<void> {
  importing.value = true
  try {
    const s = await backupApi.importConfig(strategy.value)
    if (!s) return // 用户取消
    ElMessage.success(
      `导入完成：新增 ${s.endpointsAdded} · 更新 ${s.endpointsUpdated} · 跳过 ${s.endpointsSkipped} 个连接，` +
        `设置 ${s.settingsKeys} 项`,
    )
  } catch (e) {
    ElMessage.error(`导入失败：${errMsg(e)}`)
  } finally {
    importing.value = false
  }
}
</script>

<style scoped>
.ops-row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.import-group { display: flex; align-items: center; gap: 8px; }
.strategy-select {
  height: 30px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface);
  color: var(--ink);
  font-size: var(--fs-sm);
  outline: none;
}
</style>
