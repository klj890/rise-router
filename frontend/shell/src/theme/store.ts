import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { AccentId, BrandOverride, ThemeMode } from './types'

interface ThemeState {
  mode: ThemeMode
  accentId: AccentId
  brand: BrandOverride
  setMode: (m: ThemeMode) => void
  setAccent: (a: AccentId) => void
  setBrand: (b: BrandOverride) => void
  resetBrand: () => void
}

/** 主题偏好（暗/浅、强调色、白标覆盖），localStorage 持久化跨刷新保持。 */
export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      mode: 'dark', // 暗色优先
      accentId: 'indigo', // 设计稿默认主色：靛蓝紫
      brand: {},
      setMode: (mode) => set({ mode }),
      setAccent: (accentId) => set({ accentId }),
      setBrand: (brand) => set((s) => ({ brand: { ...s.brand, ...brand } })),
      resetBrand: () => set({ brand: {} }),
    }),
    { name: 'rise-theme' },
  ),
)
