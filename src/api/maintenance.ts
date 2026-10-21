/**
 * 存储维护 API。
 *
 * 对应后端 `/api/maintenance/storage`：大表（压缩存档/请求日志）限额
 * 清理。统计数据（usage_history）永不因预算被删。
 */
import api from './client'

/** 存储占用统计 */
export interface StorageStats {
  dbFileBytes: number
  compressedRows: number
  compressedBytes: number
  callLogRows: number
  callLogDetailBytes: number
  usageRows: number
}

/** 清理报告 */
export interface StorageCleanupReport {
  beforeBytes: number
  afterBytes: number
  expiredDeleted: number
  compressedDeleted: number
  callLogTrimmed: number
  callLogDeleted: number
  vacuumed: boolean
  dryRun: boolean
}

export const maintenanceApi = {
  /** 当前存储占用与预算配置 */
  info: () => api.get('/maintenance/storage').then(r => r.data as {
    stats: StorageStats
    budgetMb: number
  }),
  /** 执行存储预算清理。先 dryRun=true 预览，确认后传 false 执行 */
  cleanup: (dryRun: boolean) => api.post('/maintenance/storage/cleanup', { dryRun }).then(r => r.data as {
    budgetMb: number
    report: StorageCleanupReport
  }),
}
