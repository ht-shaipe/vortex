import api from './client'

export interface BackupInfo {
  name: string
  path: string
  size: number
  modified: number
}

export interface BackupResult {
  success: boolean
  backupPath: string
  fileSize: number
  encrypted: boolean
}

export const backupApi = {
  list: () => api.get('/backups').then(r => r.data.backups as BackupInfo[]),
  create: (backupDir?: string) => api.post('/backups', { backupDir }).then(r => r.data as BackupResult),
}
