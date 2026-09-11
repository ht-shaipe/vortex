<template>
  <section class="sync-card">
    <div class="sync-head">
      <h2 class="sync-title">本地备份 / 配置迁移</h2>
    </div>
    <p class="sync-desc">
      导出提供商连接与设置为 JSON，用于换机迁移。
      导出的文件可能包含明文 API Key，请妥善保管。
    </p>

    <div class="ops-row flex flex-wrap items-center gap-8px">
      <button type="button" class="btn" :disabled="busy" @click="onExport">
        <el-icon :size="13"><Download /></el-icon>导出配置
      </button>

      <div class="import-group flex items-center gap-8px">
        <select v-model="strategy" class="strategy-select h-30px px-8px border border-line rounded-sm bg-surface text-ink text-sm outline-none" aria-label="同名处理策略">
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
/**
 * LocalBackup.vue — 本地备份
 * 职责：提供配置导出（JSON 文件）与导入功能，支持同名跳过/覆盖策略选择。
 */
import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, Upload } from '@element-plus/icons-vue'
import { backupApi, errMsg, type ImportStrategy } from '@/api/sync'

const strategy = ref<ImportStrategy>('skip') // 导入时同名处理策略
const exporting = ref(false) // 是否正在导出
const importing = ref(false) // 是否正在导入
const busy = computed(() => exporting.value || importing.value) // 是否忙碌

/** 导出配置到本地 JSON 文件。 */
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

/** 导入配置：选择文件并按策略导入。 */
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


