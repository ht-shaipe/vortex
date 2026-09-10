<template>
  <div class="settings-page">
    <el-card shadow="never">
      <template #header>
        <span>General Settings</span>
      </template>
      <el-form :model="generalForm" label-width="160px">
        <el-form-item label="Require API Key">
          <el-switch v-model="generalForm.requireApiKey" />
        </el-form-item>
        <el-form-item label="Default Theme">
          <el-radio-group v-model="generalForm.theme">
            <el-radio value="dark">Dark</el-radio>
            <el-radio value="light">Light</el-radio>
            <el-radio value="system">System</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="saveGeneral">Save</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <el-card shadow="never" style="margin-top: 20px">
      <template #header>
        <span>Routing Settings</span>
      </template>
      <el-form :model="routingForm" label-width="160px">
        <el-form-item label="Default Strategy">
          <el-select v-model="routingForm.defaultStrategy" style="width: 100%">
            <el-option
              v-for="s in strategies"
              :key="s.id"
              :label="s.name"
              :value="s.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="Auto-Combo">
          <el-switch v-model="routingForm.autoComboEnabled" />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="saveRouting">Save</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <el-card shadow="never" style="margin-top: 20px">
      <template #header>
        <span>Endpoint Information</span>
      </template>
      <div class="endpoint-info">
        <p>OpenAI-compatible endpoint: <code>http://localhost:20128/v1</code></p>
        <p>Set your tools to use this base URL with any API key from the Keys page.</p>
      </div>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '@/stores/settings'
import { STRATEGIES } from '@/api/combos'

const settingsStore = useSettingsStore()
const strategies = STRATEGIES

const generalForm = ref({
  requireApiKey: false,
  theme: 'dark',
})

const routingForm = ref({
  defaultStrategy: 'priority',
  autoComboEnabled: true,
})

async function loadSettings() {
  await settingsStore.fetchSettings()
  const s = settingsStore.settings as any
  if (s.general) {
    generalForm.value.requireApiKey = s.general.requireApiKey ?? false
    generalForm.value.theme = s.general.theme ?? 'dark'
  }
  if (s.routing) {
    routingForm.value.defaultStrategy = s.routing.defaultStrategy ?? 'priority'
    routingForm.value.autoComboEnabled = s.routing.autoComboEnabled ?? true
  }
}

async function saveGeneral() {
  await settingsStore.saveSettings({ general: generalForm.value })
  ElMessage.success('Settings saved')
}

async function saveRouting() {
  await settingsStore.saveSettings({ routing: routingForm.value })
  ElMessage.success('Routing settings saved')
}

onMounted(loadSettings)
</script>

<style scoped>
.endpoint-info code {
  background: var(--color-surface-2);
  padding: 2px 8px;
  border-radius: 4px;
  font-family: ui-monospace, monospace;
  color: var(--color-accent);
}

.endpoint-info p {
  margin: 8px 0;
  font-size: 14px;
}
</style>
