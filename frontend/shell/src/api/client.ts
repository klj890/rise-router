import axios from 'axios'
import { useAuthStore } from '../store/auth'
import { useLocaleStore } from '../i18n/store'
import { translateError, type ApiErrorBody } from '../i18n/errors'

/** 统一 API 客户端；自动附带 Bearer token 与 X-Locale。 */
export const api = axios.create({ baseURL: '/' })

api.interceptors.request.use((config) => {
  const token = useAuthStore.getState().token
  if (token) config.headers.Authorization = `Bearer ${token}`
  config.headers['X-Locale'] = useLocaleStore.getState().locale
  return config
})

// 后端返回 error code + 参数；前端在此映射为当前 locale 的文案，挂到 error.localizedMessage。
api.interceptors.response.use(
  (resp) => resp,
  (error) => {
    // 统一赋予本地化文案：UI 可无脑展示 error.localizedMessage（无标准错误体时回落 UNKNOWN）。
    if (error) {
      const body = error.response?.data?.error as ApiErrorBody | undefined
      error.localizedMessage = translateError(body)
    }
    return Promise.reject(error)
  },
)
