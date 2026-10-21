/**
 * API 密钥批量导入 / 导出。
 *
 * 对应后端 `/api/bulk-keys`：支持从文本批量创建密钥，以及按格式导出全部密钥。
 */
import api from './client'

/** 批量导入结果 */
export interface BulkImportResult {
  /** 成功创建的数量 */
  successCount: number
  /** 失败数量 */
  failCount: number
  /** 逐条失败原因 */
  errors: string[]
}

export const bulkKeysApi = {
  /** 从文本内容导入密钥；`format` 指定文本格式 */
  import: (content: string, format: string) =>
    api.post('/bulk-keys/import', { content, format }).then(r => r.data as BulkImportResult),
  /** 按指定格式导出全部密钥，返回文本内容 */
  export: (format: string) =>
    api.post('/bulk-keys/export', { format }).then(r => r.data.content as string),
}
