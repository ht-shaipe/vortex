<template>
  <div>
    <PageHeader :title="tab === 'aliases' ? '模型映射' : '路由配置'" :sub="tab === 'aliases' ? '配置虚拟模型名，按优先级故障转移到已接入的真实模型' : '命名回退链：把多个提供方/模型串成有序目标，客户端以 auto:名称 引用'">
      <template #actions>
        <template v-if="tab === 'aliases'">
          <button type="button" class="btn" :disabled="autoGenerating" @click="autoGroup">
            <el-icon :size="14" class="spin" v-if="autoGenerating"><Loading /></el-icon>
            <el-icon :size="14" v-else><MagicStick /></el-icon> 自动归纳
          </button>
          <button type="button" class="btn accent" @click="openCreate">
            <el-icon :size="14"><Plus /></el-icon> 新建映射
          </button>
        </template>
        <button v-else type="button" class="btn accent" @click="openProfileCreate">
          <el-icon :size="14"><Plus /></el-icon> 新建配置
        </button>
      </template>
    </PageHeader>

    <div class="tabs mb-[var(--gap-lg)]">
      <button type="button" class="tab" :class="{ active: tab === 'aliases' }" @click="tab = 'aliases'">模型映射</button>
      <button type="button" class="tab" :class="{ active: tab === 'profiles' }" @click="switchTab('profiles')">路由配置</button>
    </div>

    <template v-if="tab === 'aliases'">
    <div v-if="loading" class="text-center text-ink-4 py-40px">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <div v-else-if="aliases.length === 0" class="card nil-state py-60px">
      <el-icon :size="32" class="text-ink-5"><Connection /></el-icon>
      <p class="text-ink-3 mt-12px">尚未配置虚拟模型映射</p>
      <p class="text-12px text-ink-4 mt-4px">创建虚拟模型名后，客户端用该名称请求，网关按优先级依次尝试映射的真实模型</p>
    </div>

    <div v-else class="flex flex-col gap-12px">
      <div v-for="a in aliases" :key="a.id" class="card py-16px px-20px">
        <div class="alias-head flex items-center justify-between">
          <div class="flex items-center gap-10px">
            <span class="alias-name font-mono text-15px font-semibold">{{ a.alias }}</span>
            <span class="pill" :class="a.is_active ? 'ok' : 'neutral'">{{ a.is_active ? '启用' : '停用' }}</span>
            <span class="pill" :class="a.source === 'auto' ? 'info' : 'neutral'">{{ a.source === 'auto' ? '自动' : '手动' }}</span>
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
          <div v-for="(t, i) in a.targets" :key="i" class="target-row flex items-center gap-8px py-4px text-13px">
            <span class="target-idx inline-flex items-center justify-center w-20px h-20px rounded-full bg-surface-3 text-ink-4 text-11px font-semibold shrink-0">{{ i + 1 }}</span>
            <span class="pill info font-mono">{{ t.provider }}</span>
            <span class="text-13px text-ink-2 font-mono">{{ t.model }}</span>
            <span v-if="t.connection_id" class="text-11px text-ink-4">· {{ connNameMap[t.connection_id] ?? t.connection_id.slice(0, 8) }}</span>
          </div>
        </div>
      </div>
    </div>
    </template>

    <template v-else>
    <!-- 使用说明：解释 auto:名称 引用方式 -->
    <div class="card flex items-start gap-8px py-10px px-16px mb-12px text-12px text-ink-3 leading-[1.7]">
      <el-icon :size="13" class="shrink-0 mt-3px text-ink-4"><InfoFilled /></el-icon>
      <div>
        <b class="text-ink-2">使用方式：</b>创建配置后，客户端把请求的 <span class="mono">model</span> 字段填为
        <span class="mono text-ink-2">auto:配置名</span>（如 <span class="mono text-ink-2">auto:daily-driver</span>），
        网关会按目标列表顺序依次尝试：前一个失败或熔断时自动跳到下一个，直到成功。
      </div>
    </div>

    <div v-if="profilesLoading" class="text-center text-ink-4 py-40px">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <div v-else-if="profiles.length === 0" class="card nil-state py-60px">
      <el-icon :size="32" class="text-ink-5"><Connection /></el-icon>
      <p class="text-ink-3 mt-12px">还没有路由配置</p>
      <p class="text-12px text-ink-4 mt-4px">创建一条命名回退链，例如：主力模型 → 备用模型 → 免费模型，故障时自动降级</p>
    </div>

    <div v-else class="flex flex-col gap-12px">
      <div v-for="p in profiles" :key="p.id" class="card py-16px px-20px">
        <div class="profile-head flex items-center justify-between">
          <div class="flex items-center gap-10px flex-wrap">
            <span class="profile-name font-mono text-15px font-semibold">{{ p.name }}</span>
            <span class="pill" :class="p.isActive ? 'ok' : 'neutral'">{{ p.isActive ? '启用' : '停用' }}</span>
            <el-tooltip content="客户端引用方式：model 字段填 auto:配置名" placement="top">
              <span class="mono text-11.5px text-ink-4 bg-surface-2 py-2px px-8px rounded-4px cursor-help">auto:{{ p.name }}</span>
            </el-tooltip>
          </div>
          <div class="flex items-center gap-8px">
            <button class="btn sm ghost" @click="removeProfile(p)">
              <el-icon :size="13"><Delete /></el-icon>
            </button>
          </div>
        </div>
        <div v-if="p.description" class="text-13px text-ink-3 mt-8px">{{ p.description }}</div>
        <!-- 回退链可视化 -->
        <div class="flex items-center flex-wrap gap-6px mt-10px">
          <template v-for="(t, i) in p.targets" :key="i">
            <el-icon v-if="i > 0" :size="12" class="text-ink-5"><ArrowRight /></el-icon>
            <span class="chain-node inline-flex items-center gap-6px py-4px px-10px bg-surface-2 border border-solid border-line rounded-sm text-12px">
              <ProviderLogo :name="t.provider" :size="16" />
              <span class="mono text-ink-2">{{ t.provider }}/{{ t.model }}</span>
              <span v-if="t.connectionId && connNameMap[t.connectionId]" class="text-11px text-ink-4">· {{ connNameMap[t.connectionId] }}</span>
            </span>
          </template>
          <span v-if="p.targets.length === 0" class="text-12px text-ink-4">无目标</span>
        </div>
      </div>
    </div>
    </template>

    <!-- 编辑/创建弹窗 -->
    <el-dialog v-model="dialogOpen" :title="editing ? '编辑映射' : '新建映射'" width="600">
      <div class="dialog-body flex flex-col gap-16px">
        <div class="form-row flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">虚拟模型名</label>
          <el-input v-model="form.alias" placeholder="如 my-smart-model" class="font-mono" />
        </div>
        <div class="form-row flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">映射目标（按顺序故障转移）</label>
          <div v-if="modelOptions.length === 0" class="text-13px text-ink-4 py-8px">
            当前没有已接入的模型，请先在「提供商管理」中添加连接并配置模型
          </div>
          <div v-for="(t, i) in form.targets" :key="i" class="target-edit flex items-center gap-8px mb-8px flex-wrap">
            <span class="target-idx inline-flex items-center justify-center w-20px h-20px rounded-full bg-surface-3 text-ink-4 text-11px font-semibold shrink-0">{{ i + 1 }}</span>
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
        <div class="form-row flex flex-col gap-6px">
          <el-switch v-model="form.is_active" />
          <span class="ml-8px text-13px text-ink-3">启用</span>
        </div>
      </div>
      <template #footer>
        <button class="btn" @click="dialogOpen = false">取消</button>
        <button class="btn accent" @click="save">{{ editing ? '保存' : '创建' }}</button>
      </template>
    </el-dialog>

    <el-dialog v-model="profileDialogOpen" title="新建路由配置" width="600">
      <div class="dialog-body flex flex-col gap-16px">
        <div class="form-row flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">名称</label>
          <el-input v-model="profileForm.name" placeholder="如 my-routing-profile" class="font-mono" />
        </div>
        <div class="form-row flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">描述</label>
          <el-input v-model="profileForm.description" placeholder="可选" />
        </div>
        <div class="form-row flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">路由目标</label>
          <div v-if="modelOptions.length === 0" class="text-13px text-ink-4 py-8px">
            当前没有已接入的模型，请先在「提供商管理」中添加连接并配置模型
          </div>
          <div v-for="(t, i) in profileForm.targets" :key="i" class="target-edit flex items-center gap-8px mb-8px flex-wrap">
            <span class="target-idx inline-flex items-center justify-center w-20px h-20px rounded-full bg-surface-3 text-ink-4 text-11px font-semibold shrink-0">{{ i + 1 }}</span>
            <el-select
              v-model="t._selected"
              placeholder="选择已接入的模型"
              style="flex: 1"
              filterable
              @change="(v: string) => onProfileTargetSelect(t, v)"
            >
              <el-option
                v-for="opt in modelOptions"
                :key="opt.key"
                :label="opt.label"
                :value="opt.key"
              />
            </el-select>
            <button class="btn sm" @click="moveProfileTarget(i, -1)" :disabled="i === 0">
              <el-icon :size="12"><ArrowUp /></el-icon>
            </button>
            <button class="btn sm" @click="moveProfileTarget(i, 1)" :disabled="i === profileForm.targets.length - 1">
              <el-icon :size="12"><ArrowDown /></el-icon>
            </button>
            <button class="btn sm ghost" @click="profileForm.targets.splice(i, 1)" :disabled="profileForm.targets.length <= 1">
              <el-icon :size="12"><Delete /></el-icon>
            </button>
          </div>
          <button class="btn sm accent" @click="addProfileTarget">
            <el-icon :size="12"><Plus /></el-icon> 添加目标
          </button>
        </div>
        <div class="form-row flex flex-col gap-6px">
          <el-switch v-model="profileForm.isActive" />
          <span class="ml-8px text-13px text-ink-3">启用</span>
        </div>
      </div>
      <template #footer>
        <button class="btn" @click="profileDialogOpen = false">取消</button>
        <button class="btn accent" @click="saveProfile">创建</button>
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
import { Plus, Edit, Delete, Loading, Connection, ArrowUp, ArrowDown, ArrowRight, InfoFilled, MagicStick } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listAliases, createAlias, updateAlias, deleteAlias, autoGenerate, type ModelAlias, type ModelAliasTarget } from '@/api/modelAliases'
import { listProviders } from '@/api/providers'
import { routingProfilesApi, type RoutingProfile, type ProfileTarget } from '@/api/routingProfiles'

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

interface ProfileFormTarget extends ProfileTarget {
  _selected: string
}

const loading = ref(true)
const aliases = ref<ModelAlias[]>([])
const dialogOpen = ref(false)
const editing = ref<string | null>(null)
const modelOptions = ref<ModelOption[]>([])
const connNameMap = ref<Record<string, string>>({})
const autoGenerating = ref(false)
const tab = ref<'aliases' | 'profiles'>('aliases')
const profiles = ref<RoutingProfile[]>([])
const profilesLoading = ref(false)
const profileDialogOpen = ref(false)

const form = reactive({
  alias: '',
  targets: [] as FormTarget[],
  is_active: true,
})

const profileForm = reactive({
  name: '',
  description: '',
  targets: [] as ProfileFormTarget[],
  isActive: true,
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

/** 自动归纳：按模型家族分组生成虚拟别名。 */
async function autoGroup() {
  autoGenerating.value = true
  try {
    const res = await autoGenerate()
    ElMessage.success(`已归纳 ${res.total} 个虚拟模型名`)
    await load()
  } catch {
    ElMessage.error('自动归纳失败')
  } finally {
    autoGenerating.value = false
  }
}

async function loadProfiles() {
  profilesLoading.value = true
  try {
    profiles.value = await routingProfilesApi.list()
  } catch {
    /* ignore */
  } finally {
    profilesLoading.value = false
  }
}

function switchTab(t: 'aliases' | 'profiles') {
  tab.value = t
  if (t === 'profiles' && profiles.value.length === 0 && !profilesLoading.value) {
    loadProfiles()
  }
}

function onProfileTargetSelect(t: ProfileFormTarget, key: string) {
  const opt = modelOptions.value.find((o) => o.key === key)
  if (opt) {
    t.provider = opt.provider
    t.model = opt.model
    t.connectionId = opt.connection_id
  }
}

function openProfileCreate() {
  profileForm.name = ''
  profileForm.description = ''
  profileForm.targets = [{ provider: '', model: '', _selected: '' }]
  profileForm.isActive = true
  profileDialogOpen.value = true
}

function addProfileTarget() {
  profileForm.targets.push({ provider: '', model: '', _selected: '' })
}

function moveProfileTarget(i: number, dir: number) {
  const j = i + dir
  if (j < 0 || j >= profileForm.targets.length) return
  const tmp = profileForm.targets[i]
  profileForm.targets[i] = profileForm.targets[j]
  profileForm.targets[j] = tmp
}

async function saveProfile() {
  if (!profileForm.name.trim()) {
    ElMessage.warning('请输入名称')
    return
  }
  if (profileForm.targets.length === 0 || profileForm.targets.some((t) => !t.provider || !t.model)) {
    ElMessage.warning('请为每个目标选择一个已接入的模型')
    return
  }
  const payload = profileForm.targets.map(({ _selected, ...rest }) => rest)
  try {
    await routingProfilesApi.create({
      name: profileForm.name,
      description: profileForm.description || undefined,
      targets: payload,
      isActive: profileForm.isActive,
    })
    ElMessage.success('已创建')
    profileDialogOpen.value = false
    await loadProfiles()
  } catch {
    ElMessage.error('操作失败')
  }
}

async function removeProfile(p: RoutingProfile) {
  try {
    await ElMessageBox.confirm(`确定删除路由配置「${p.name}」？`, '删除确认', { type: 'warning' })
    await routingProfilesApi.delete(p.id)
    ElMessage.success('已删除')
    await loadProfiles()
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
/* 回退链节点：提供方/模型不换行展示 */
.chain-node {
  white-space: nowrap;
}
</style>
