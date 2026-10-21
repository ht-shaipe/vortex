/**
 * 数据库备份 API。
 *
 * 对应后端 `/api/backups`：创建一致性快照（SQLite `VACUUM INTO`）并列出备份文件。
 */
import api from './client'

/** 备份文件元信息 */
export interface BackupInfo {
  /** 文件名（`vortex_backup_YYYYMMDD_HHMMSS.db`） */
  name: string
  /** 文件绝对路径 */
  path: string
  /** 文件大小（字节） */
  size: number
  /** 最后修改时间（Unix 秒） */
  modified: number
}

/** 创建备份结果 */
export interface BackupResult {
  success: boolean
  backupPath: string
  fileSize: number
  encrypted: boolean
}

export const backupApi = {
  /** 列出数据目录下的备份文件 */
  list: () => api.get('/backups').then(r => r.data.backups as BackupInfo[]),
  /** 创建新备份；`backupDir` 缺省时写入数据目录 */
  create: (backupDir?: string) => api.post('/backups', { backupDir }).then(r => r.data as BackupResult),
}
