import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { DEFAULT_LOCALE, matchLocale, type Locale } from './config'

interface LocaleState {
  locale: Locale
  setLocale: (l: Locale) => void
}

/** locale 偏好的唯一前端正源（localStorage 持久化）；首访无持久值时按浏览器语言初始化。 */
export const useLocaleStore = create<LocaleState>()(
  persist(
    (set) => ({
      locale: matchLocale(typeof navigator !== 'undefined' ? navigator.language : DEFAULT_LOCALE),
      setLocale: (locale) => set({ locale }),
    }),
    { name: 'rise-locale' },
  ),
)
