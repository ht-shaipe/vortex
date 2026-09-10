<template>
  <section class="sync-card">
    <div class="sync-head">
      <h2 class="sync-title">WebDAV 配置</h2>
      <span class="pill warn"><i class="dot" />后端未接入</span>
    </div>
    <p class="sync-desc">
      配置会保存在本机，等 vortex 后端提供 <code class="mono">/api/sync/webdav</code>
      后即可启用云端备份与恢复。
    </p>

    <div class="form-grid">
      <div v-for="f in FIELDS" :key="f.k" class="field">
        <label class="field-label" :for="`wd-${f.k}`">{{ f.label }}</label>
        <input
          :id="`wd-${f.k}`"
          v-model="form[f.k]"
          class="field-input"
          :type="f.type ?? 'text'"
          :placeholder="f.ph"
        />
      </div>
    </div>

    <div class="form-foot">
      <button type="button" class="btn" :disabled="testing" @click="onTest">
        {{ testing ? '测试中…' : '测试连接' }}
      </button>
      <button type="button" class="btn primary" :disabled="saving" @click="onSave">
        {{ saving ? '保存中…' : '保存' }}
      </button>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * WebdavForm.vue — WebDAV 表单
 * 职责：配置 WebDAV 连接参数（URL、用户名、密码、路径），支持测试连接与保存到本机。
 */
import { onMounted, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { webdavApi, errMsg, type WebDavConfig } from '@/api/sync'

// 表单字段配置（key、标签、输入类型、占位符）
const FIELDS: Array<{ k: keyof WebDavConfig; label: string; type?: string; ph?: string }> = [
  { k: 'url', label: '服务器 URL', ph: 'https://dav.example.com/' },
  { k: 'username', label: '用户名' },
  { k: 'password', label: '密码', type: 'password' },
  { k: 'configPath', label: '配置路径', ph: '/vortex' },
]

// 表单数据（响应式）
const form = reactive<WebDavConfig>({
  url: '',
  username: '',
  password: '',
  configPath: '/vortex',
  statsPath: '/vortex/stats',
})
const testing = ref(false) // 是否正在测试连接
const saving = ref(false) // 是否正在保存

// 挂载时加载已保存的配置
onMounted(async () => {
  Object.assign(form, await webdavApi.getConfig())
})

/** 测试 WebDAV 连接。 */
async function onTest(): Promise<void> {
  testing.value = true
  try {
    const r = await webdavApi.test({ ...form })
    r.success ? ElMessage.success(r.message) : ElMessage.error(r.message)
  } catch (e) {
    ElMessage.error(errMsg(e))
  } finally {
    testing.value = false
  }
}

/** 保存 WebDAV 配置到本机。 */
async function onSave(): Promise<void> {
  saving.value = true
  try {
    await webdavApi.saveConfig({ ...form })
    ElMessage.success('WebDAV 配置已保存到本机')
  } catch (e) {
    ElMessage.error(`保存失败：${errMsg(e)}`)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.field { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
.field-label { font-size: var(--fs-sm); color: var(--ink-3); }
.field-input {
  height: 32px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--surface);
  color: var(--ink);
  font-size: var(--fs-body);
  outline: none;
  transition: border-color 0.12s;
}
.field-input:focus { border-color: var(--accent); }
.field-input::placeholder { color: var(--ink-4); }
.form-foot { display: flex; gap: 8px; }
</style>
