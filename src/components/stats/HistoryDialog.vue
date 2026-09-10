<template>
  <button type="button" class="btn sm" @click="openDialog">
    <el-icon :size="13"><Clock /></el-icon>历史记录
  </button>

  <el-dialog v-model="open" title="历史记录" width="1000px" align-center>
    <p v-if="loading" class="hint">加载中…</p>
    <p v-else-if="rows.length === 0" class="hint">暂无历史记录</p>
    <div v-else class="hist">
      <el-scrollbar class="hist-scroll" max-height="58vh">
        <table class="table">
          <thead>
            <tr>
              <th>日期</th>
              <th>端点</th>
              <th class="right">请求</th>
              <th class="right">错误</th>
              <th class="right">输入</th>
              <th class="right">输出</th>
              <th class="right">缓存</th>
              <th class="right">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(r, i) in rows" :key="`${r.date}-${r.endpointName}-${i}`">
              <td class="num">{{ r.date }}</td>
              <td>{{ r.endpointName }}</td>
              <td class="right num">{{ fmtInt(r.requests) }}</td>
              <td class="right num" :class="{ bad: r.errors > 0 }">{{ fmtInt(r.errors) }}</td>
              <td class="right num">{{ fmtInt(r.inputTokens) }}</td>
              <td class="right num">{{ fmtInt(r.outputTokens) }}</td>
              <td class="right num">{{ fmtInt(r.cacheCreationTokens + r.cacheReadTokens) }}</td>
              <td class="right">
                <div class="ops">
                  <button
                    type="button"
                    class="btn bare icon"
                    :disabled="pending"
                    aria-label="删除该行"
                    @click="delRow(r)"
                  >
                    <el-icon :size="13"><Delete /></el-icon>
                  </button>
                  <button
                    type="button"
                    class="btn bare sm"
                    :disabled="pending"
                    @click="delDay(r.date)"
                  >
                    整天
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </el-scrollbar>
      <Pagination v-model:page="page" :page-size="PAGE_SIZE" :total="total" />
    </div>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * HistoryDialog.vue — 历史对话框
 * 职责：以弹窗展示按天统计的历史记录，支持分页、删除单行与删除整天数据。
 */
import { ref, watch } from 'vue'
import { Clock, Delete } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import Pagination from './Pagination.vue'
import { fmtInt } from '@/lib/format'
import { statsApi, type DailyStat } from '@/api/stats'

const PAGE_SIZE = 12 // 每页行数

const open = ref(false) // 弹窗是否打开
const page = ref(1) // 当前页码
const loading = ref(false) // 是否正在加载
const pending = ref(false) // 是否正在执行删除操作
const rows = ref<DailyStat[]>([]) // 历史统计行列表
const total = ref(0) // 总记录数

// Emits 定义：changed 通知外部数据已变更（删除后需刷新）
const emit = defineEmits<{ changed: [] }>()

/** 提取错误信息文案。 */
function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

/** 打开弹窗并加载第一页数据。 */
function openDialog(): void {
  page.value = 1
  open.value = true
  void load()
}

/** 加载历史统计数据。 */
async function load(): Promise<void> {
  loading.value = true
  try {
    const res = await statsApi.getStatsHistory(page.value, PAGE_SIZE)
    rows.value = res.items
    total.value = res.total
  } catch (e) {
    ElMessage.error(`加载失败：${errMsg(e)}`)
  } finally {
    loading.value = false
  }
}

// 翻页时重新加载（仅弹窗打开时）
watch(page, () => {
  if (open.value) void load()
})

/** 删除单行记录（指定端点 + 日期）。 */
async function delRow(r: DailyStat): Promise<void> {
  pending.value = true
  try {
    await statsApi.deleteDailyStat(r.endpointName, r.date)
    ElMessage.success('已删除该记录')
    await load()
    emit('changed')
  } catch (e) {
    ElMessage.error(`删除失败：${errMsg(e)}`)
  } finally {
    pending.value = false
  }
}

/** 删除整天的所有记录。 */
async function delDay(date: string): Promise<void> {
  pending.value = true
  try {
    const n = await statsApi.deleteStatsByDate(date)
    ElMessage.success(`已删除该日 ${n} 条记录`)
    page.value = 1
    await load()
    emit('changed')
  } catch (e) {
    ElMessage.error(`删除失败：${errMsg(e)}`)
  } finally {
    pending.value = false
  }
}
</script>

<style scoped>
.hint { margin: 0; font-size: var(--fs-body); color: var(--ink-4); }
.hist { display: flex; flex-direction: column; gap: var(--gap-md); }
.hist-scroll {

  border: 1px solid var(--line);
  border-radius: var(--r-md);
}
.hist-scroll .table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--surface);
}
.table tbody tr { cursor: default; }
.right { text-align: right; }
.bad { color: var(--err); }
.ops { display: inline-flex; align-items: center; gap: 2px; }
</style>
