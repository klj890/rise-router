import { useEffect, type ReactNode } from 'react'
import { App as AntdApp, ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import dayjs from 'dayjs'
import 'dayjs/locale/zh-cn'
import 'dayjs/locale/en'
import i18n from './index'
import { useLocaleStore } from './store'
import type { Locale } from './config'

const ANTD_LOCALE = { 'zh-CN': zhCN, 'en-US': enUS } as const
const DAYJS_LOCALE: Record<Locale, string> = { 'zh-CN': 'zh-cn', 'en-US': 'en' }

/**
 * 单一 locale 源驱动三处：i18next（文案）、AntD ConfigProvider（组件文案）、dayjs（日期）。
 * 嵌在 ThemeProvider 内层 —— 主题与语言各自独立的 Provider，互不耦合。
 */
export function LocaleProvider({ children }: { children: ReactNode }) {
  const locale = useLocaleStore((s) => s.locale)

  useEffect(() => {
    void i18n.changeLanguage(locale)
    dayjs.locale(DAYJS_LOCALE[locale])
    document.documentElement.lang = locale
  }, [locale])

  // AntdApp 须嵌在带 locale 的 ConfigProvider 内，App.useApp() 静态方法（message/modal/notification）才会跟随语言。
  return (
    <ConfigProvider locale={ANTD_LOCALE[locale]}>
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  )
}
