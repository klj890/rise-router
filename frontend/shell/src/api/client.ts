import axios from 'axios'
import { useAuthStore } from '../store/auth'

/** 统一 API 客户端；自动附带 Bearer token。 */
export const api = axios.create({ baseURL: '/' })

api.interceptors.request.use((config) => {
  const token = useAuthStore.getState().token
  if (token) config.headers.Authorization = `Bearer ${token}`
  return config
})
