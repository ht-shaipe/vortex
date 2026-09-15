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

      <div class="btn relative">
        <span>{{ strategy === 'skip' ? '跳过同名' : '覆盖同名' }}</span>
        <el-icon :size="13"><ArrowDown /></el-icon>
        <select
          v-model="strategy"
          class="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
          aria-label="同名处理策略"
        >
          <option value="skip">跳过同名</option>
          <option value="overwrite">覆盖同名</option>
        </select>
      </div>

      <button type="button" class="btn" :disabled="busy" @click="onImport">
        <el-icon :size="13"><Upload /></el-icon>导入配置
      </button>
    </div>

    <p v-if="lastExportName" class="export-hint flex items-center flex-wrap gap-x-8px gap-y-2px mt-8px text-13px text-ink-3 m-0">
      <span>已导出：<code class="font-mono text-ink-2">{{ lastExportName }}</code></span>
      <button type="button" class="link-btn" :disabled="!lastExportDir" @click="openExportDir">
        打开所在目录
      </button>
    </p>
  </section>
</template>

<script setup lang="ts">
/**
 * LocalBackup.vue — 本地备份
 * 职责：提供配置导出（JSON 文件）与导入功能，支持同名跳过/覆盖策略选择。
 */
import { computed, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { ArrowDown, Download, Upload } from '@element-plus/icons-vue'
import { backupApi, errMsg, type ImportStrategy } from '@/api/sync'
import { open } from '@tauri-apps/plugin-shell'
import { downloadDir } from '@tauri-apps/api/path'

const strategy = ref<ImportStrategy>('skip') // 导入时同名处理策略
const exporting = ref(false) // 是否正在导出
const importing = ref(false) // 是否正在导入
const busy = computed(() => exporting.value || importing.value) // 是否忙碌

const lastExportName = ref<string | null>(null) // 最近一次导出的文件名
const lastExportDir = ref<string | null>(null) // 最近一次导出的所在目录

/** 导出配置到本地 JSON 文件。 */
async function onExport(): Promise<void> {
  exporting.value = true
  try {
    const name = await backupApi.exportConfig()
    ElMessage.success(`已导出到 ${name}`)
    lastExportName.value = name
    lastExportDir.value = await downloadDir().catch(() => null) // 记住目录，供"打开所在目录"使用
  } catch (e) {
    ElMessage.error(`导出失败：${errMsg(e)}`)
  } finally {
    exporting.value = false
  }
}

/** 在文件管理器中打开最近一次导出的所在目录。 */
async function openExportDir(): Promise<void> {
  if (!lastExportDir.value) {
    ElMessage.warning('无法确定导出目录')
    return
  }
  try {
    await open(lastExportDir.value)
  } catch (e) {
    ElMessage.error(`打开目录失败：${errMsg(e)}`)
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

<style scoped>
.export-hint code {
  background: var(--el-fill-color-light);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 12px;
}
.link-btn {
  background: none;
  border: none;
  padding: 0;
  margin: 0;
  font: inherit;
  color: var(--el-color-primary);
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
}
.link-btn:hover {
  color: var(--el-color-primary-light-3);
}
.link-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  text-decoration: none;
}
</style>


