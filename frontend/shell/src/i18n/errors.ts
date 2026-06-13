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
export function translateError(err: ApiErrorBody | undefined): string {
  if (!err?.code) return i18n.t('errors:UNKNOWN')
  return i18n.t(`errors:${err.code}`, {
    ...err.params,
    defaultValue: err.message || i18n.t('errors:UNKNOWN'),
  })
}
