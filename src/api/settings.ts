import api from './client'

export async function getSettings() {
  const { data } = await api.get('/settings')
  return data as { general: Record<string, unknown> }
}

export async function updateSettings(updates: Record<string, unknown>) {
  const { data } = await api.patch('/settings', updates)
  return data
}
