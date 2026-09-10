/**
 * 设置管理 API。
 *
 * 职责：封装与后端 `/settings` 端点交互的接口，包括读取和更新全局设置。
 */
import api from './client'

/**
 * 获取当前全局设置。
 * @returns 包含通用设置键值对的对象
 */
export async function getSettings() {
  const { data } = await api.get('/settings')
  return data as { general: Record<string, unknown> }
}

/**
 * 更新全局设置。
 * @param updates - 需要更新的设置键值对
 * @returns 更新后的设置数据
 */
export async function updateSettings(updates: Record<string, unknown>) {
  const { data } = await api.patch('/settings', updates)
  return data
}
