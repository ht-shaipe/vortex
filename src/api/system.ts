/**
 * 系统状态与代理控制适配层。
 *
 * 桌面端走 Tauri IPC，Web 端走 HTTP API。
 */

import { runtime } from '@/lib/runtime'
import client from './client'

/** 系统运行状态（与 Rust 端 SystemStatus 对应） */
export interface SystemStatus {
  proxy_running: boolean
  proxy_port: number
  version: string
  database_ok: boolean
  provider_count: number
  active_provider_count: number
  api_key_count: number
  active_api_key_count: number
  today_requests: number
  today_tokens_input: number
  today_tokens_output: number
}

/** 获取系统运行状态 */
export async function getSystemStatus(): Promise<SystemStatus> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<SystemStatus>('get_system_status')
  }
  const { data } = await client.get<SystemStatus>('/system/status')
  return data
}

/** 启动本地代理服务器 */
export async function startProxy(): Promise<void> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('start_proxy')
    return
  }
  await client.post('/system/proxy/start')
}

/** 停止本地代理服务器 */
export async function stopProxy(): Promise<void> {
  if (runtime.kind === 'desktop') {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('stop_proxy')
    return
  }
  await client.post('/system/proxy/stop')
}
