import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface AuthState {
  token: string | null
  username: string | null
  /** 管理令牌（X-Admin-Token；RBAC 落地前的过渡，匹配后端 RR_ADMIN_TOKEN）。 */
  adminToken: string | null
  login: (token: string, username: string) => void
  logout: () => void
  setAdminToken: (t: string | null) => void
}

/** 登录态（Zustand + localStorage 持久化，跨页面保持）。 */
export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      token: null,
      username: null,
      adminToken: null,
      login: (token, username) => set({ token, username }),
      logout: () => set({ token: null, username: null }),
      setAdminToken: (t) => set({ adminToken: t && t.trim() ? t.trim() : null }),
    }),
    { name: 'rise-auth' },
  ),
)
