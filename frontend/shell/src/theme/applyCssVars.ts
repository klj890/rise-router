import { NEUTRAL, FUNCTIONAL, FONT_SANS, FONT_MONO } from './tokens'
import { resolvePrimary } from './presets'
import type { AccentId, BrandOverride, ResolvedMode } from './types'

/**
 * 把当前主题 token 写成 `--rr-*` CSS 变量到 :root。
 * 供自定义 CSS 与未来 Module Federation 第三方插件继承主题。
 */
export function applyCssVars(
  mode: ResolvedMode,
  accentId: AccentId,
  override?: BrandOverride,
): void {
  const n = NEUTRAL[mode]
  const primary = resolvePrimary(accentId, mode, override)
  const root = document.documentElement

  const vars: Record<string, string> = {
    '--rr-color-primary': primary,
    '--rr-color-success': FUNCTIONAL.success,
    '--rr-color-warning': FUNCTIONAL.warning,
    '--rr-color-error': FUNCTIONAL.error,
    '--rr-color-info': FUNCTIONAL.info,
    '--rr-bg-layout': n.bgLayout,
    '--rr-bg-container': n.bgContainer,
    '--rr-bg-elevated': n.bgElevated,
    '--rr-border': n.border,
    '--rr-border-secondary': n.borderSecondary,
    '--rr-text': n.text,
    '--rr-text-secondary': n.textSecondary,
    '--rr-text-tertiary': n.textTertiary,
    '--rr-fill': n.fill,
    '--rr-font-sans': override?.fontFamily ?? FONT_SANS,
    '--rr-font-mono': FONT_MONO,
  }
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v)

  root.setAttribute('data-theme', mode)
  root.style.colorScheme = mode
}
