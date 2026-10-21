/**
 * Playground 调试 API。
 *
 * 对应后端 `/api/playground`：走完整代理链路执行一次测试请求，
 * 便于在不配置客户端的情况下验证模型与路由。
 */
import api from './client'

/** Playground 请求参数 */
export interface PlaygroundRequest {
  model: string
  /** OpenAI 格式消息数组 */
  messages: any[]
  /** 是否以 SSE 流式返回，默认 false */
  stream?: boolean
  temperature?: number
  maxTokens?: number
  topP?: number
  /** 透传给上游的额外参数 */
  extra?: Record<string, any>
}

export const playgroundApi = {
  /** 发送测试请求，返回上游原始响应（流式时为 SSE 响应） */
  send: (data: PlaygroundRequest) => api.post('/playground', data).then(r => r.data),
}
