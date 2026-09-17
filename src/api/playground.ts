import api from './client'

export interface PlaygroundRequest {
  model: string
  messages: any[]
  stream?: boolean
  temperature?: number
  maxTokens?: number
  topP?: number
  extra?: Record<string, any>
}

export const playgroundApi = {
  send: (data: PlaygroundRequest) => api.post('/playground', data).then(r => r.data),
}
