<template>
  <div class="pager flex items-center justify-between text-sm text-ink-3">
    <span>
      共 <span class="tnum">{{ total }}</span> 条 · 第
      <span class="tnum">{{ page }}</span>/<span class="tnum">{{ totalPages }}</span> 页
    </span>
    <div class="pager-btns flex items-center gap-4px">
      <button
        type="button"
        class="btn sm icon"
        :disabled="page <= 1"
        aria-label="上一页"
        @click="emit('update:page', page - 1)"
      >
        <el-icon :size="13"><ArrowLeft /></el-icon>
      </button>
      <button
        type="button"
        class="btn sm icon"
        :disabled="page >= totalPages"
        aria-label="下一页"
        @click="emit('update:page', page + 1)"
      >
        <el-icon :size="13"><ArrowRight /></el-icon>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * Pagination.vue — 分页
 * 职责：展示总条数、当前页/总页数，并提供上一页/下一页按钮。
 */
import { computed } from 'vue'
import { ArrowLeft, ArrowRight } from '@element-plus/icons-vue'

// Props 定义：page 为当前页码，pageSize 为每页条数，total 为总记录数
const props = defineProps<{ page: number; pageSize: number; total: number }>()
// Emits 定义：update:page 同步页码变化
const emit = defineEmits<{ 'update:page': [page: number] }>()

const totalPages = computed(() => Math.max(1, Math.ceil(props.total / props.pageSize))) // 总页数
</script>

<style scoped>
.pager { gap: var(--gap-sm); }
</style>
