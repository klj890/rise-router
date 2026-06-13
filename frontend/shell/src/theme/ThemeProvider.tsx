import { useEffect, useMemo, type ReactNode } from 'react'
import { ConfigProvider } from 'antd'
import { useThemeStore } from './store'
import { buildAntdTheme } from './presets'
import { applyCssVars } from './applyCssVars'
import { useResolvedMode } from './useResolvedMode'

export function ThemeProvider({ children }: { children: ReactNode }) {
  const accentId = useThemeStore((s) => s.accentId)
  const brand = useThemeStore((s) => s.brand)
  const resolved = useResolvedMode()

  const themeConfig = useMemo(
    () => buildAntdTheme(resolved, accentId, brand),
    [resolved, accentId, brand],
  )

  useEffect(() => {
    applyCssVars(resolved, accentId, brand)
  }, [resolved, accentId, brand])

  // 主题挂外层 ConfigProvider；locale 与 AntdApp 由内层 LocaleProvider 负责
  // （AntdApp 须在带 locale 的 ConfigProvider 内，否则 App.useApp() 静态方法读不到语言包）。
  return <ConfigProvider theme={themeConfig}>{children}</ConfigProvider>
}
