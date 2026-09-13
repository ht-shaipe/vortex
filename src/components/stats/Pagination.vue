<template>
  <div class="pager flex items-center justify-between gap-[var(--gap-sm)] text-sm text-ink-3">
    <span class="pager-info whitespace-nowrap">
      共 <span class="tnum font-semibold text-ink-2">{{ total }}</span> 条 · 第
      <span class="tnum font-semibold text-ink-2">{{ page }}</span>/<span class="tnum">{{ totalPages }}</span> 页
    </span>
    <div class="pager-btns flex items-center gap-4px">
      <button
        type="button"
        class="btn sm icon"
        :disabled="page <= 1"
        aria-label="首页"
        @click="emit('update:page', 1)"
      >
        <el-icon :size="13"><DArrowLeft /></el-icon>
      </button>
      <button
        type="button"
        class="btn sm icon"
        :disabled="page <= 1"
        aria-label="上一页"
        @click="emit('update:page', page - 1)"
      >
        <el-icon :size="13"><ArrowLeft /></el-icon>
      </button>
      <template v-for="p in visiblePages" :key="p">
        <span v-if="p === '...'" class="pager-ellipsis inline-flex items-center px-4px text-ink-4 text-13px">…</span>
        <button
          v-else
          type="button"
          class="btn sm pager-num min-w-32px justify-center tabular-nums [&.active]:bg-accent [&.active]:text-white [&.active]:border-accent [&.active:hover]:bg-accent [&.active:hover]:text-white [&.active:hover]:border-accent"
          :class="{ active: p === page }"
          @click="emit('update:page', p as number)"
        >{{ p }}</button>
      </template>
      <button
        type="button"
        class="btn sm icon"
        :disabled="page >= totalPages"
        aria-label="下一页"
        @click="emit('update:page', page + 1)"
      >
        <el-icon :size="13"><ArrowRight /></el-icon>
      </button>
      <button
        type="button"
        class="btn sm icon"
        :disabled="page >= totalPages"
        aria-label="末页"
        @click="emit('update:page', totalPages)"
      >
        <el-icon :size="13"><DArrowRight /></el-icon>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * Pagination.vue — 分页
 * 职责：展示总条数、当前页/总页数，提供首页/上一页/页码/下一页/末页按钮。
 */
import { computed } from 'vue'
import { ArrowLeft, ArrowRight, DArrowLeft, DArrowRight } from '@element-plus/icons-vue'

const props = defineProps<{ page: number; pageSize: number; total: number }>()
const emit = defineEmits<{ 'update:page': [page: number] }>()

const totalPages = computed(() => Math.max(1, Math.ceil(props.total / props.pageSize)))

/** 可见页码列表：当前页前后各 2 页，超出范围用省略号折叠。 */
const visiblePages = computed<(number | '...')[]>(() => {
  const tp = totalPages.value
  const cur = props.page
  if (tp <= 7) return Array.from({ length: tp }, (_, i) => i + 1)
  const pages: (number | '...')[] = [1]
  const start = Math.max(2, cur - 1)
  const end = Math.min(tp - 1, cur + 1)
  if (start > 2) pages.push('...')
  for (let i = start; i <= end; i++) pages.push(i)
  if (end < tp - 1) pages.push('...')
  pages.push(tp)
  return pages
})
</script>

