<template>
  <div>
    <PageHeader title="模型映射" sub="配置虚拟模型名，按优先级故障转移到已接入的真实模型">
      <template #actions>
        <button type="button" class="btn accent" @click="openCreate">
          <el-icon :size="14"><Plus /></el-icon> 新建映射
        </button>
      </template>
    </PageHeader>

    <div v-if="loading" class="text-center text-ink-4 py-40px">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <div v-else-if="aliases.length === 0" class="card empty-state">
      <el-icon :size="32" class="text-ink-5"><Connection /></el-icon>
      <p class="text-ink-3 mt-12px">尚未配置虚拟模型映射</p>
      <p class="text-12px text-ink-4 mt-4px">创建虚拟模型名后，客户端用该名称请求，网关按优先级依次尝试映射的真实模型</p>
    </div>

    <div v-else class="alias-list">
      <div v-for="a in aliases" :key="a.id" class="card alias-card">
        <div class="alias-head flex items-center justify-between">
          <div class="flex items-center gap-10px">
            <span class="alias-name font-mono text-15px font-semibold">{{ a.alias }}</span>
            <span class="pill" :class="a.is_active ? 'pill--ok' : ''">{{ a.is_active ? '启用' : '停用' }}</span>
            <span class="text-12px text-ink-4">{{ a.targets.length }} 个目标</span>
          </div>
          <div class="flex items-center gap-8px">
            <el-switch :model-value="a.is_active" @change="(v: boolean) => toggleActive(a, v)" />
            <button class="btn sm" @click="openEdit(a)">
              <el-icon :size="13"><Edit /></el-icon> 编辑
            </button>
            <button class="btn sm ghost" @click="remove(a)">
              <el-icon :size="13"><Delete /></el-icon>
            </button>
          </div>
        </div>
        <div class="alias-targets mt-12px">
          <div v-for="(t, i) in a.targets" :key="i" class="target-row flex items-center gap-8px py-4px">
            <span class="target-idx">{{ i + 1 }}</span>
            <span class="pill pill--accent font-mono">{{ t.provider }}</span>
            <span class="text-13px text-ink-2 font-mono">{{ t.model }}</span>
            <span v-if="t.connection_id" class="text-11px text-ink-4">· {{ connNameMap[t.connection_id] ?? t.connection_id.slice(0, 8) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 编辑/创建弹窗 -->
    <el-dialog v-model="dialogOpen" :title="editing ? '编辑映射' : '新建映射'" width="600">
      <div class="dialog-body">
        <div class="form-row">
          <label>虚拟模型名</label>
          <el-input v-model="form.alias" placeholder="如 my-smart-model" class="font-mono" />
        </div>
        <div class="form-row">
          <label>映射目标（按顺序故障转移）</label>
          <div v-if="modelOptions.length === 0" class="text-13px text-ink-4 py-8px">
            当前没有已接入的模型，请先在「提供商管理」中添加连接并配置模型
          </div>
          <div v-for="(t, i) in form.targets" :key="i" class="target-edit flex items-center gap-8px mb-8px">
            <span class="target-idx">{{ i + 1 }}</span>
            <el-select
              v-model="t._selected"
              placeholder="选择已接入的模型"
              style="flex: 1"
              filterable
              @change="(v: string) => onTargetSelect(t, v)"
            >
              <el-option
                v-for="opt in modelOptions"
                :key="opt.key"
                :label="opt.label"
                :value="opt.key"
              />
            </el-select>
            <button class="btn sm" @click="moveTarget(i, -1)" :disabled="i === 0">
              <el-icon :size="12"><ArrowUp /></el-icon>
            </button>
            <button class="btn sm" @click="moveTarget(i, 1)" :disabled="i === form.targets.length - 1">
              <el-icon :size="12"><ArrowDown /></el-icon>
            </button>
            <button class="btn sm ghost" @click="form.targets.splice(i, 1)" :disabled="form.targets.length <= 1">
              <el-icon :size="12"><Delete /></el-icon>
            </button>
          </div>
          <button class="btn sm accent" @click="addTarget">
            <el-icon :size="12"><Plus /></el-icon> 添加目标
          </button>
        </div>
        <div class="form-row">
          <el-switch v-model="form.is_active" />
          <span class="ml-8px text-13px text-ink-3">启用</span>
        </div>
      </div>
      <template #footer>
        <button class="btn" @click="dialogOpen = false">取消</button>
        <button class="btn accent" @click="save">{{ editing ? '保存' : '创建' }}</button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 模型映射页面。
 * 管理虚拟模型别名，配置到真实模型的映射关系与故障转移优先级。
 */
import { ref, reactive, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Edit, Delete, Loading, Connection, ArrowUp, ArrowDown } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import { listAliases, createAlias, updateAlias, deleteAlias, type ModelAlias, type ModelAliasTarget } from '@/api/modelAliases'
import { listProviders } from '@/api/providers'

/** 可选模型项：一个连接下的一个具体模型。 */
interface ModelOption {
  key: string
  label: string
  provider: string
  model: string
  connection_id: string
}

/** 表单中的目标行，带选中状态。 */
interface FormTarget extends ModelAliasTarget {
  _selected: string
}

const loading = ref(true)
const aliases = ref<ModelAlias[]>([])
const dialogOpen = ref(false)
const editing = ref<string | null>(null)
const modelOptions = ref<ModelOption[]>([])
const connNameMap = ref<Record<string, string>>({})

const form = reactive({
  alias: '',
  targets: [] as FormTarget[],
  is_active: true,
})

/** 加载别名列表。 */
async function load() {
  loading.value = true
  try {
    aliases.value = await listAliases()
  } catch {
    /* ignore */
  } finally {
    loading.value = false
  }
}

/** 加载所有已接入的模型，构建选项列表。 */
async function loadModelOptions() {
  try {
    const data = await listProviders()
    const opts: ModelOption[] = []
    const nameMap: Record<string, string> = {}
    for (const c of data.connections ?? []) {
      if (!c.isActive) continue
      nameMap[c.id] = c.name
      for (const m of c.models ?? []) {
        const label = `${c.name} · ${m.name || m.id}`
        opts.push({
          key: `${c.id}::${m.id}`,
          label,
          provider: c.provider,
          model: m.id,
          connection_id: c.id,
        })
      }
    }
    modelOptions.value = opts
    connNameMap.value = nameMap
  } catch {
    /* ignore */
  }
}

/** 根据 provider + model + connection_id 查找选项 key。 */
function findOptionKey(t: ModelAliasTarget): string {
  if (t.connection_id) return `${t.connection_id}::${t.model}`
  const opt = modelOptions.value.find((o) => o.provider === t.provider && o.model === t.model)
  return opt?.key ?? ''
}

/** 目标选择回调：从选项 key 填充 provider/model/connection_id。 */
function onTargetSelect(t: FormTarget, key: string) {
  const opt = modelOptions.value.find((o) => o.key === key)
  if (opt) {
    t.provider = opt.provider
    t.model = opt.model
    t.connection_id = opt.connection_id
  }
}

/** 打开创建弹窗。 */
function openCreate() {
  editing.value = null
  form.alias = ''
  form.targets = [{ provider: '', model: '', _selected: '' }]
  form.is_active = true
  dialogOpen.value = true
}

/** 打开编辑弹窗。 */
function openEdit(a: ModelAlias) {
  editing.value = a.id
  form.alias = a.alias
  form.targets = a.targets.map((t) => ({ ...t, _selected: findOptionKey(t) }))
  form.is_active = a.is_active
  dialogOpen.value = true
}

/** 添加映射目标。 */
function addTarget() {
  form.targets.push({ provider: '', model: '', _selected: '' })
}

/** 移动目标顺序。 */
function moveTarget(i: number, dir: number) {
  const j = i + dir
  if (j < 0 || j >= form.targets.length) return
  const tmp = form.targets[i]
  form.targets[i] = form.targets[j]
  form.targets[j] = tmp
}

/** 保存（创建或更新）。 */
async function save() {
  if (!form.alias.trim()) {
    ElMessage.warning('请输入虚拟模型名')
    return
  }
  if (form.targets.length === 0 || form.targets.some((t) => !t.provider || !t.model)) {
    ElMessage.warning('请为每个目标选择一个已接入的模型')
    return
  }
  const payload = form.targets.map(({ _selected, ...rest }) => rest)
  try {
    if (editing.value) {
      await updateAlias(editing.value, {
        alias: form.alias,
        targets: payload,
        is_active: form.is_active,
      })
      ElMessage.success('已保存')
    } else {
      await createAlias({
        alias: form.alias,
        targets: payload,
        is_active: form.is_active,
      })
      ElMessage.success('已创建')
    }
    dialogOpen.value = false
    await load()
  } catch {
    ElMessage.error('操作失败')
  }
}

/** 切换启用状态。 */
async function toggleActive(a: ModelAlias, v: boolean) {
  try {
    await updateAlias(a.id, { is_active: v })
    a.is_active = v
  } catch {
    ElMessage.error('操作失败')
  }
}

/** 删除别名。 */
async function remove(a: ModelAlias) {
  try {
    await ElMessageBox.confirm(`确定删除映射「${a.alias}」？`, '删除确认', { type: 'warning' })
    await deleteAlias(a.id)
    ElMessage.success('已删除')
    await load()
  } catch {
    /* cancelled */
  }
}

onMounted(() => {
  load()
  loadModelOptions()
})
</script>

<style scoped>
.empty-state {
  text-align: center;
  padding: 60px 24px;
}
.alias-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.alias-card {
  padding: 16px 20px;
}
.target-row {
  font-size: 13px;
}
.target-idx {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--surface-3);
  color: var(--ink-4);
  font-size: 11px;
  font-weight: 600;
  flex-shrink: 0;
}
.dialog-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.form-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.form-row label {
  font-size: 13px;
  font-weight: 500;
  color: var(--ink-2);
}
.target-edit {
  flex-wrap: wrap;
}
</style>
