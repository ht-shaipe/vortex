<template>
  <div class="keys-page">
    <div class="page-actions">
      <el-button type="primary" @click="showCreateDialog = true">
        <el-icon><Plus /></el-icon> Create API Key
      </el-button>
    </div>

    <el-card shadow="never">
      <el-table :data="keys" stripe style="width: 100%">
        <el-table-column prop="name" label="Name" />
        <el-table-column label="Key" width="280">
          <template #default="{ row }">
            <code class="api-key-value">{{ row.key }}</code>
          </template>
        </el-table-column>
        <el-table-column label="Status" width="100">
          <template #default="{ row }">
            <el-tag :type="row.isActive && !row.isBanned ? 'success' : 'danger'" size="small">
              {{ row.isActive && !row.isBanned ? 'Active' : 'Disabled' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="createdAt" label="Created" width="180">
          <template #default="{ row }">
            {{ new Date(row.createdAt).toLocaleDateString() }}
          </template>
        </el-table-column>
        <el-table-column label="Actions" width="100">
          <template #default="{ row }">
            <el-button size="small" type="danger" @click="handleDelete(row.id)">Delete</el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <el-dialog v-model="showCreateDialog" title="Create API Key" width="480px">
      <el-form :model="createForm" label-width="100px">
        <el-form-item label="Name">
          <el-input v-model="createForm.name" placeholder="My API Key" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showCreateDialog = false">Cancel</el-button>
        <el-button type="primary" @click="handleCreate" :loading="creating">Create</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import * as api from '@/api/keys'
import type { ApiKey } from '@/types'

const keys = ref<ApiKey[]>([])
const showCreateDialog = ref(false)
const creating = ref(false)
const createForm = ref({ name: '' })

async function fetchKeys() {
  const data = await api.listKeys()
  keys.value = data.keys
}

async function handleCreate() {
  if (!createForm.value.name) {
    ElMessage.warning('Name is required')
    return
  }
  creating.value = true
  try {
    const key = await api.createKey({ name: createForm.value.name })
    ElMessage.success(`API Key created: ${key.key}`)
    showCreateDialog.value = false
    createForm.value = { name: '' }
    await fetchKeys()
  } catch (e: unknown) {
    ElMessage.error('Failed: ' + (e instanceof Error ? e.message : String(e)))
  } finally {
    creating.value = false
  }
}

async function handleDelete(id: string) {
  try {
    await ElMessageBox.confirm('Delete this API key?', 'Confirm')
    await api.deleteKey(id)
    ElMessage.success('Deleted')
    await fetchKeys()
  } catch { /* cancelled */ }
}

onMounted(fetchKeys)
</script>

<style scoped>
.page-actions {
  margin-bottom: 20px;
}

.api-key-value {
  font-family: ui-monospace, monospace;
  font-size: 12px;
  background: var(--color-surface-2);
  padding: 2px 8px;
  border-radius: 4px;
}
</style>
