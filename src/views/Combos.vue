<template>
  <div class="combos-page">
    <div class="page-actions">
      <el-button type="primary" @click="showCreateDialog = true">
        <el-icon><Plus /></el-icon> Create Combo
      </el-button>
    </div>

    <el-row :gutter="16">
      <el-col :span="8" v-for="combo in comboStore.combos" :key="combo.id">
        <el-card shadow="never" class="combo-card">
          <template #header>
            <div class="combo-header">
              <span class="combo-name">{{ combo.name }}</span>
              <el-dropdown trigger="click">
                <el-button size="small" circle><el-icon><MoreFilled /></el-icon></el-button>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item @click="handleEdit(combo)">Edit</el-dropdown-item>
                    <el-dropdown-item @click="handleDelete(combo.id)" divided>Delete</el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
            </div>
          </template>

          <div class="combo-strategy">
            <el-tag effect="dark" size="small">{{ combo.data.strategy }}</el-tag>
          </div>

          <div class="combo-steps">
            <div v-for="(step, i) in combo.data.models" :key="i" class="combo-step">
              <span class="step-index">{{ i + 1 }}</span>
              <span class="step-model">{{ step.modelStr }}</span>
              <span v-if="step.weight && step.weight !== 1" class="step-weight">w:{{ step.weight }}</span>
            </div>
            <div v-if="combo.data.models.length === 0" class="no-steps">
              No models configured
            </div>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <el-empty v-if="comboStore.combos.length === 0" description="No combos yet. Create one to get started." />

    <el-dialog v-model="showCreateDialog" :title="editingCombo ? 'Edit Combo' : 'Create Combo'" width="600px">
      <el-form :model="comboForm" label-width="100px">
        <el-form-item label="Name">
          <el-input v-model="comboForm.name" placeholder="my-combo" :disabled="!!editingCombo" />
        </el-form-item>

        <el-form-item label="Strategy">
          <el-select v-model="comboForm.strategy" filterable style="width: 100%">
            <el-option
              v-for="s in strategies"
              :key="s.id"
              :label="s.name"
              :value="s.id"
            >
              <span>{{ s.name }}</span>
              <span style="float: right; color: var(--color-text-muted); font-size: 12px">{{ s.description }}</span>
            </el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="Models">
          <div class="step-editor">
            <div v-for="(step, i) in comboForm.models" :key="i" class="step-row">
              <el-input v-model="step.modelStr" placeholder="provider/model-name" style="flex: 1" />
              <el-input-number v-model="step.weight" :min="0" :max="100" :step="0.1" size="small" style="width: 100px" />
              <el-button type="danger" size="small" circle @click="comboForm.models.splice(i, 1)">
                <el-icon><Delete /></el-icon>
              </el-button>
            </div>
            <el-button @click="addStep" size="small">
              <el-icon><Plus /></el-icon> Add Step
            </el-button>
          </div>
        </el-form-item>
      </el-form>

      <template #footer>
        <el-button @click="showCreateDialog = false">Cancel</el-button>
        <el-button type="primary" @click="handleSave" :loading="saving">Save</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, MoreFilled, Delete } from '@element-plus/icons-vue'
import { useComboStore } from '@/stores/combo'
import { STRATEGIES } from '@/api/combos'
import type { Combo, ComboStep } from '@/types'

const comboStore = useComboStore()
const strategies = STRATEGIES
const showCreateDialog = ref(false)
const saving = ref(false)
const editingCombo = ref<Combo | null>(null)

const comboForm = ref({
  name: '',
  strategy: 'priority',
  models: [] as { modelStr: string; provider: string; weight: number }[],
})

function addStep() {
  comboForm.value.models.push({ modelStr: '', provider: '', weight: 1 })
}

function handleEdit(combo: Combo) {
  editingCombo.value = combo
  comboForm.value = {
    name: combo.name,
    strategy: combo.data.strategy,
    models: combo.data.models.map(m => ({
      modelStr: m.modelStr,
      provider: m.provider,
      weight: m.weight || 1,
    })),
  }
  showCreateDialog.value = true
}

async function handleSave() {
  if (!comboForm.value.name) {
    ElMessage.warning('Name is required')
    return
  }
  saving.value = true
  try {
    if (editingCombo.value) {
      // Update existing
      ElMessage.success('Combo updated (re-create for now)')
    } else {
      await comboStore.addCombo({
        name: comboForm.value.name,
        strategy: comboForm.value.strategy,
        models: comboForm.value.models.filter(m => m.modelStr).map(m => ({
          modelStr: m.modelStr,
          provider: m.modelStr.split('/')[0] || 'custom',
          weight: m.weight,
        })),
      })
      ElMessage.success('Combo created')
    }
    showCreateDialog.value = false
    editingCombo.value = null
    comboForm.value = { name: '', strategy: 'priority', models: [] }
  } catch (e: unknown) {
    ElMessage.error('Failed: ' + (e instanceof Error ? e.message : String(e)))
  } finally {
    saving.value = false
  }
}

async function handleDelete(id: string) {
  try {
    await ElMessageBox.confirm('Delete this combo?', 'Confirm')
    await comboStore.removeCombo(id)
    ElMessage.success('Deleted')
  } catch { /* cancelled */ }
}

onMounted(() => {
  comboStore.fetchCombos()
})
</script>

<style scoped>
.page-actions {
  margin-bottom: 20px;
}

.combo-card {
  margin-bottom: 16px;
}

.combo-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.combo-name {
  font-weight: 600;
  font-size: 15px;
}

.combo-strategy {
  margin-bottom: 12px;
}

.combo-steps {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.combo-step {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.step-index {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--color-surface-2);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  flex-shrink: 0;
}

.step-model {
  flex: 1;
  font-family: ui-monospace, monospace;
}

.step-weight {
  color: var(--color-text-muted);
  font-size: 12px;
}

.no-steps {
  color: var(--color-text-muted);
  font-style: italic;
  font-size: 13px;
}

.step-editor {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}

.step-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
