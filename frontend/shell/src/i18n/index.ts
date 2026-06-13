import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { FALLBACK_LOCALE, SUPPORTED_LOCALES } from './config'
import { useLocaleStore } from './store'

import zhCommon from './locales/zh-CN/common.json'
import zhAuth from './locales/zh-CN/auth.json'
import zhErrors from './locales/zh-CN/errors.json'
import enCommon from './locales/en-US/common.json'
import enAuth from './locales/en-US/auth.json'
import enErrors from './locales/en-US/errors.json'

// 基础命名空间随包内置；业务域/第三方插件命名空间后续按路由懒加载 / addResourceBundle 注册。
export const NAMESPACES = ['common', 'auth', 'errors'] as const

void i18n.use(initReactI18next).init({
  resources: {
    'zh-CN': { common: zhCommon, auth: zhAuth, errors: zhErrors },
    'en-US': { common: enCommon, auth: enAuth, errors: enErrors },
  },
  // 同步采用持久化偏好作为初始语言，避免首屏从默认语言闪烁到目标语言（FOUC）
  lng: useLocaleStore.getState().locale,
  fallbackLng: FALLBACK_LOCALE,
  supportedLngs: SUPPORTED_LOCALES as unknown as string[],
  ns: NAMESPACES as unknown as string[],
  defaultNS: 'common',
  interpolation: { escapeValue: false }, // React 已转义
})

export default i18n
