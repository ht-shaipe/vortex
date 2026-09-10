/**
 * Axios HTTP 客户端实例配置。
 *
 * 职责：创建并导出全局共享的 axios 实例，统一配置 baseURL、超时时间、
 * 默认请求头以及响应错误拦截器，供 api 目录下其余模块复用。
 */
import axios from 'axios'

// 创建 axios 实例，统一配置基础路径、超时与请求头
const api = axios.create({
  baseURL: 'http://localhost:20128/api', // 后端 API 基础地址
  timeout: 30000, // 请求超时时间：30 秒
  headers: {
    'Content-Type': 'application/json', // 默认 JSON 请求体
  },
})

// 响应拦截器：统一捕获并打印错误，再将错误继续向上抛出
api.interceptors.response.use(
  (response) => response,
  (error) => {
    // 优先打印后端返回的错误信息，兜底打印网络错误消息
    console.error('API Error:', error.response?.data || error.message)
    return Promise.reject(error)
  }
)

export default api
