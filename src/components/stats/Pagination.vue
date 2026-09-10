<template>
  <div class="pager">
    <span>
      共 <span class="tnum">{{ total }}</span> 条 · 第
      <span class="tnum">{{ page }}</span>/<span class="tnum">{{ totalPages }}</span> 页
    </span>
    <div class="pager-btns">
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
import { computed } from 'vue'
import { ArrowLeft, ArrowRight } from '@element-plus/icons-vue'

const props = defineProps<{ page: number; pageSize: number; total: number }>()
const emit = defineEmits<{ 'update:page': [page: number] }>()

const totalPages = computed(() => Math.max(1, Math.ceil(props.total / props.pageSize)))
</script>

<style scoped>
.pager {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  font-size: var(--fs-sm);
  color: var(--ink-3);
}
.pager-btns { display: flex; align-items: center; gap: 4px; }
</style>
