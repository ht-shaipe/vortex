/**
 * 压缩内容回忆 API。
 *
 * Prompt 压缩管线以 SHA-256 为键保存原文，对应后端 `/api/compressed-content`：
 * 列表接口不返回原文，详情接口按 hash 取回完整内容。
 */
import api from './client'

/** 压缩内容列表项（不含原文） */
export interface CompressedContentEntry {
  /** 原文的 SHA-256 hash */
  hash: string
  /** 原文字节数 */
  contentSize: number
  /** 压缩节省的 Token 数 */
  savedTokens: number
  provider: string | null
  model: string | null
  createdAt: string
  /** 过期时间；null 表示永久保留 */
  expiresAt: string | null
}

/** 压缩内容详情（含原文） */
export interface CompressedContentDetail extends CompressedContentEntry {
  originalContent: string
}

export const compressedContentApi = {
  /** 列出最近的压缩内容记录 */
  list: () =>
    api.get('/compressed-content').then(r => r.data.entries as CompressedContentEntry[]),
  /** 按 hash 检索原始内容 */
  get: (hash: string) =>
    api.get(`/compressed-content/${hash}`).then(r => r.data.entry as CompressedContentDetail),
}
