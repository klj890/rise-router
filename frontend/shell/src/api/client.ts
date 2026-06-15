import axios from 'axios'
import { useAuthStore } from '../store/auth'
import { useLocaleStore } from '../i18n/store'
import { translateError, type ApiErrorBody } from '../i18n/errors'

// 扩展 AxiosError，使 localizedMessage 类型安全（消费方可直接读取）。
declare module 'axios' {
  export interface AxiosError {
    localizedMessage?: string
  }
}

/** 统一 API 客户端；自动附带 Bearer token 与 X-Locale。 */
export const api = axios.create({ baseURL: '/' })

api.interceptors.request.use((config) => {
  const { token, adminToken } = useAuthStore.getState()
  if (token) config.headers.Authorization = `Bearer ${token}`
  // 管理令牌：存在即附带（非管理端点会忽略此头），供管理台 CRUD 走 admin_guard。
  if (adminToken) config.headers['X-Admin-Token'] = adminToken
  config.headers['X-Locale'] = useLocaleStore.getState().locale
  return config
})

// 后端错误体两种形态：① code+params 对象（i18n 契约）② 纯字符串（当前多数端点 {"error":"..."}）。
// 前者走 translateError 本地化；后者直接展示后端可读串；都没有再回落 UNKNOWN。
api.interceptors.response.use(
  (resp) => resp,
  (error) => {
    if (error) {
      const raw = error.response?.data?.error
      if (typeof raw === 'string') {
        error.localizedMessage = raw
      } else {
        error.localizedMessage = translateError(raw as ApiErrorBody | undefined)
      }
    }
    return Promise.reject(error)
  },
)
