import api from './client'

export interface BulkImportResult {
  successCount: number
  failCount: number
  errors: string[]
}

export const bulkKeysApi = {
  import: (content: string, format: string) =>
    api.post('/bulk-keys/import', { content, format }).then(r => r.data as BulkImportResult),
  export: (format: string) =>
    api.post('/bulk-keys/export', { format }).then(r => r.data.content as string),
}
