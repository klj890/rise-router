import i18n from './index'

/** 后端 API 错误体：{ code, params, message }（契约见 docs/i18n.md §4.1）。 */
export interface ApiErrorBody {
  code: string
  params?: Record<string, unknown>
  message?: string
}

/**
 * 把后端 error code + 参数映射为当前 locale 的文案。
 * 后端不本地化文案；前端按 `errors` 命名空间插值。未知 code 回落到 UNKNOWN。
 */
// i18next 保留字：若混进后端参数会干扰翻译配置（语言/命名空间/兜底等），需剔除。
const I18NEXT_RESERVED = new Set([
  'lng',
  'lngs',
  'ns',
  'defaultValue',
  'fallbackLng',
  'context',
  'replace',
  'count',
])

export function translateError(err: ApiErrorBody | undefined): string {
  if (!err?.code) return i18n.t('errors:UNKNOWN')
  const safeParams: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(err.params ?? {})) {
    if (!I18NEXT_RESERVED.has(k)) safeParams[k] = v
  }
  return i18n.t(`errors:${err.code}`, {
    ...safeParams,
    defaultValue: err.message || i18n.t('errors:UNKNOWN'),
  })
}
