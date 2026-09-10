import api from './client'

export interface ComboStep {
  modelStr: string
  provider: string
  weight?: number
  label?: string
  connectionId?: string
}

export interface ComboData {
  strategy: string
  models: ComboStep[]
  config?: Record<string, unknown>
  systemMessage?: string
}

export interface Combo {
  id: string
  name: string
  data: ComboData
  sortOrder: number
  contextCacheProtection: boolean
  createdAt: string
  updatedAt: string
}

export const STRATEGIES = [
  { id: 'priority', name: 'Priority', description: 'Try targets in order, use first available' },
  { id: 'fill_first', name: 'Fill First', description: 'Fill each target\'s quota before moving on' },
  { id: 'weighted', name: 'Weighted', description: 'Weighted random by per-target weight' },
  { id: 'round_robin', name: 'Round Robin', description: 'Cycle through targets in order' },
  { id: 'p2c', name: 'Power of Two Choices', description: 'Random load balancing with P2C' },
  { id: 'least_used', name: 'Least Used', description: 'Pick target with lowest current load' },
  { id: 'random', name: 'Random', description: 'Uniform random pick (deduplicated)' },
  { id: 'strict_random', name: 'Strict Random', description: 'Random without deduplicating repeats' },
  { id: 'cost_optimized', name: 'Cost Optimized', description: 'Minimize $ per request from live pricing' },
  { id: 'headroom', name: 'Headroom', description: 'Pick target with most remaining quota' },
  { id: 'reset_window', name: 'Reset Window', description: 'Prefer target whose quota resets soonest' },
  { id: 'reset_aware', name: 'Reset Aware', description: 'Rank by quota reset time, short windows first' },
  { id: 'context_relay', name: 'Context Relay', description: 'Hand off context across targets' },
  { id: 'context_optimized', name: 'Context Optimized', description: 'Best fit for current context size' },
  { id: 'lkgp', name: 'LKGP', description: 'Last-Known-Good-Path sticky routing' },
  { id: 'auto', name: 'Auto', description: '9-factor live scoring across every connection' },
  { id: 'fusion', name: 'Fusion', description: 'Fan out to panel of models + judge synthesizes' },
]

export async function listCombos() {
  const { data } = await api.get('/combos')
  return data as { combos: Combo[] }
}

export async function createCombo(params: { name: string; strategy?: string; models?: ComboStep[] }) {
  const { data } = await api.post('/combos', params)
  return data as Combo
}

export async function getCombo(id: string) {
  const { data } = await api.get(`/combos/${id}`)
  return data as Combo
}

export async function updateCombo(id: string, updates: Record<string, unknown>) {
  const { data } = await api.patch(`/combos/${id}`, updates)
  return data as Combo
}

export async function deleteCombo(id: string) {
  const { data } = await api.delete(`/combos/${id}`)
  return data as { success: boolean }
}
