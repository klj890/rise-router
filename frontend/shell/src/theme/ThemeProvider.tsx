import { useEffect, useMemo, type ReactNode } from 'react'
import { App as AntdApp, ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
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

  return (
    <ConfigProvider locale={zhCN} theme={themeConfig}>
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  )
}
