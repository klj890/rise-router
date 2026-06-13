// 前后端共享的 locale 常量（BCP 47）。后端 LocaleLayer 用同一份支持集。
export const SUPPORTED_LOCALES = ['zh-CN', 'en-US'] as const
export type Locale = (typeof SUPPORTED_LOCALES)[number]

export const DEFAULT_LOCALE: Locale = 'zh-CN' // 国情默认
export const FALLBACK_LOCALE: Locale = 'en-US'

export const LOCALE_LABELS: Record<Locale, string> = {
  'zh-CN': '简体中文',
  'en-US': 'English',
}

export function isLocale(v: string): v is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(v)
}

/** 把任意 BCP 47 串匹配到支持集（变体回落，如 zh-TW→zh-CN、en-GB→en-US）。 */
export function matchLocale(input: string | undefined | null): Locale {
  if (!input) return DEFAULT_LOCALE
  if (isLocale(input)) return input
  const lower = input.toLowerCase()
  if (lower.startsWith('zh')) return 'zh-CN'
  if (lower.startsWith('en')) return 'en-US'
  return DEFAULT_LOCALE
}
