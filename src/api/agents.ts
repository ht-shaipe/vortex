/**
 * 智能体集成适配层。
 *
 * 桌面端走 Tauri IPC，Web 端走 HTTP API。
 */

import { runtime } from '@/lib/runtime'
import client from './client'

export type AgentKind =
  | 'dsh' | 'claude_code' | 'opencode' | 'codex' | 'qwen_code'
  | 'gemini_cli' | 'cursor_agent' | 'windsurf' | 'copilot'
  | 'aider' | 'zed' | 'amp'

export type VortexConfigStatus = 'not_configured' | 'configured' | 'needs_update' | 'conflict' | 'failed'
export type FileOperation = 'create' | 'modify' | 'delete'

export interface DetectedAgent {
  kind: AgentKind
  installed: boolean
  install_path: string | null
  version: string | null
  config_exists: boolean
  config_path: string | null
  supports_auto_config: boolean
  unsupported_reason: string | null
  vortex_status: VortexConfigStatus
}

export interface FileChange {
  path: string
  operation: FileOperation
  summary: string
}

export interface ConfigPreview {
  agent: AgentKind
  files_to_modify: FileChange[]
  requires_restart: boolean
  warnings: string[]
}

export interface ConfigResult {
  agent: AgentKind
  success: boolean
  error: string | null
  backup_id: string | null
  requires_restart: boolean
}

export interface BackupInfo {
  id: string
  agent: AgentKind
  created_at: string
  file_count: number
}

/** 检测所有智能体 */
export async function detectAgents(): Promise<{ agents: DetectedAgent[]; elapsed_ms: number }> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<{ agents: DetectedAgent[]; elapsed_ms: number }>('agent_detect')
  }
  const { data } = await client.get<{ agents: DetectedAgent[]; elapsed_ms: number }>('/agents/detect')
  return data
}

/** 预览配置变更 */
export async function previewConfig(agent: AgentKind): Promise<{ preview: ConfigPreview; warnings: string[] }> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<{ preview: ConfigPreview; warnings: string[] }>('agent_preview_config', { agent })
  }
  const { data } = await client.post<{ preview: ConfigPreview; warnings: string[] }>('/agents/preview', agent)
  return data
}

/** 应用配置 */
export async function applyConfig(agent: AgentKind, confirmed: boolean): Promise<{ result: ConfigResult }> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<{ result: ConfigResult }>('agent_apply_config', { request: { agent, confirmed } })
  }
  const { data } = await client.post<{ result: ConfigResult }>('/agents/apply', { agent, confirmed })
  return data
}

/** 恢复配置 */
export async function restoreConfig(agent: AgentKind, backupId: string): Promise<void> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('agent_restore_config', { request: { agent, backup_id: backupId } })
    return
  }
  await client.post('/agents/restore', { agent, backup_id: backupId })
}

/** 列出备份 */
export async function listBackups(): Promise<{ backups: BackupInfo[] }> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<{ backups: BackupInfo[] }>('agent_list_backups')
  }
  const { data } = await client.get<{ backups: BackupInfo[] }>('/agents/backups')
  return data
}
