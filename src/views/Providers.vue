<template>
  <div class="providers-page">
    <div class="page-actions">
      <el-input v-model="searchQuery" placeholder="Search providers..." style="width: 240px" clearable />
      <el-button type="primary" @click="showAddDialog = true">
        <el-icon><Plus /></el-icon> Add Provider
      </el-button>
    </div>

    <el-row :gutter="16">
      <el-col :xs="24" :sm="12" :md="8" :lg="6" v-for="p in filteredProviders" :key="p.id">
        <el-card shadow="never" class="provider-card">
          <div class="provider-header">
            <div class="provider-icon" :style="{ backgroundColor: p.color + '22', color: p.color }">
              {{ p.name.charAt(0) }}
            </div>
            <div class="provider-info">
              <div class="provider-title">{{ p.name }}</div>
              <div class="provider-subtitle">
                <el-tag v-if="p.hasFree" type="success" size="small" effect="dark">Free</el-tag>
                <el-tag v-if="p.noAuth" type="info" size="small" effect="dark">No Auth</el-tag>
              </div>
            </div>
          </div>

          <div class="provider-stats">
            <span>{{ p.activeConnections }} active connection{{ p.activeConnections !== 1 ? 's' : '' }}</span>
          </div>

          <div class="provider-kinds">
            <el-tag v-for="kind in p.serviceKinds.slice(0, 3)" :key="kind" size="small" effect="plain" class="kind-tag">
              {{ kind }}
            </el-tag>
          </div>

          <div class="provider-actions">
            <el-button size="small" @click="handleAddConnection(p)">
              <el-icon><Plus /></el-icon> Connect
            </el-button>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <el-card shadow="never" style="margin-top: 20px" v-if="providerStore.connections.length > 0">
      <template #header>
        <span>Active Connections</span>
      </template>
      <el-table :data="providerStore.connections" stripe style="width: 100%">
        <el-table-column prop="name" label="Name" />
        <el-table-column prop="provider" label="Provider" width="120" />
        <el-table-column prop="authType" label="Auth Type" width="100" />
        <el-table-column label="Status" width="100">
          <template #default="{ row }">
            <el-tag :type="row.isActive ? 'success' : 'danger'" size="small">
              {{ row.isActive ? 'Active' : 'Inactive' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="Health" width="100">
          <template #default="{ row }">
            <el-tag :type="row.testStatus === 'ok' ? 'success' : row.testStatus === 'unknown' ? 'info' : 'danger'" size="small">
              {{ row.testStatus }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="Actions" width="160">
          <template #default="{ row }">
            <el-button size="small" @click="handleTest(row.id)">Test</el-button>
            <el-button size="small" type="danger" @click="handleDelete(row.id)">Delete</el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <el-dialog v-model="showAddDialog" title="Add Provider Connection" width="480px">
      <el-form :model="addForm" label-width="100px">
        <el-form-item label="Provider">
          <el-select v-model="addForm.provider" filterable placeholder="Select provider" style="width: 100%">
            <el-option
              v-for="p in providerStore.providers"
              :key="p.id"
              :label="p.name"
              :value="p.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="Name">
          <el-input v-model="addForm.name" placeholder="Connection name" />
        </el-form-item>
        <el-form-item label="API Key" v-if="!selectedProviderNoAuth">
          <el-input v-model="addForm.apiKey" type="password" placeholder="sk-..." show-password />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showAddDialog = false">Cancel</el-button>
        <el-button type="primary" @click="handleAdd" :loading="adding">Add</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useProviderStore } from '@/stores/provider'

const providerStore = useProviderStore()
const searchQuery = ref('')
const showAddDialog = ref(false)
const adding = ref(false)
const addForm = ref({ provider: '', name: '', apiKey: '' })

const filteredProviders = computed(() => {
  const q = searchQuery.value.toLowerCase()
  return providerStore.providers.filter(p =>
    p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q)
  )
})

const selectedProviderNoAuth = computed(() => {
  const p = providerStore.providers.find(p => p.id === addForm.value.provider)
  return p?.noAuth ?? false
})

function handleAddConnection(provider: { id: string; name: string }) {
  addForm.value.provider = provider.id
  addForm.value.name = provider.name + ' Connection'
  showAddDialog.value = true
}

async function handleAdd() {
  if (!addForm.value.provider || !addForm.value.name) {
    ElMessage.warning('Please fill in all required fields')
    return
  }
  adding.value = true
  try {
    await providerStore.addProvider({
      provider: addForm.value.provider,
      name: addForm.value.name,
      apiKey: addForm.value.apiKey || undefined,
    })
    ElMessage.success('Provider connection added')
    showAddDialog.value = false
    addForm.value = { provider: '', name: '', apiKey: '' }
  } catch (e: unknown) {
    ElMessage.error('Failed to add provider: ' + (e instanceof Error ? e.message : String(e)))
  } finally {
    adding.value = false
  }
}

async function handleTest(id: string) {
  try {
    const result = await providerStore.testConnection(id)
    ElMessage.success(result.status === 'ok' ? 'Connection OK' : `Error: ${result.error}`)
  } catch {
    ElMessage.error('Test failed')
  }
}

async function handleDelete(id: string) {
  try {
    await ElMessageBox.confirm('Delete this provider connection?', 'Confirm')
    await providerStore.removeProvider(id)
    ElMessage.success('Deleted')
  } catch { /* cancelled */ }
}

onMounted(() => {
  providerStore.fetchProviders()
})
</script>

<style scoped>
.page-actions {
  display: flex;
  justify-content: space-between;
  margin-bottom: 20px;
}

.provider-card {
  margin-bottom: 16px;
}

.provider-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}

.provider-icon {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  font-weight: 700;
}

.provider-title {
  font-weight: 600;
  font-size: 15px;
}

.provider-subtitle {
  display: flex;
  gap: 4px;
  margin-top: 2px;
}

.provider-stats {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-bottom: 8px;
}

.provider-kinds {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  margin-bottom: 12px;
}

.kind-tag {
  font-size: 11px;
}

.provider-actions {
  border-top: 1px solid var(--color-border);
  padding-top: 12px;
}
</style>
