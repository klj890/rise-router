import { useEffect, useMemo, type ReactNode } from 'react'
import { App as AntdApp, ConfigProvider } from 'antd'
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

  // 主题挂外层 ConfigProvider；locale 由内层 LocaleProvider 的 ConfigProvider 负责（关注点分离）。
  return (
    <ConfigProvider theme={themeConfig}>
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  )
}
