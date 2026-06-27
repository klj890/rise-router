import { NEUTRAL, FUNCTIONAL, SHADOW, FONT_SANS, FONT_MONO } from './tokens'
import { resolvePrimary } from './presets'
import type { AccentId, BrandOverride, ResolvedMode } from './types'

/** 半透明强调底（hover/selected/弱底）；用 color-mix 适配任意强调色。 */
function tint(color: string, percent: number): string {
  return `color-mix(in srgb, ${color} ${percent}%, transparent)`
}

/**
 * 把当前主题 token 写成 `--rr-*` CSS 变量到 :root。
 * 供自定义 CSS / 共享构件（KpiCard、StatusPill 等）与未来 Module Federation 第三方插件继承主题。
 * 变量集对齐设计稿 :root / .rr-dark 的语义命名。
 */
export function applyCssVars(
  mode: ResolvedMode,
  accentId: AccentId,
  override?: BrandOverride,
): void {
  const n = NEUTRAL[mode]
  const f = FUNCTIONAL[mode]
  const s = SHADOW[mode]
  const primary = resolvePrimary(accentId, mode, override)
  const root = document.documentElement

  const vars: Record<string, string> = {
    // 主色族
    '--rr-color-primary': primary,
    '--rr-primary': primary,
    '--rr-primary-weak': tint(primary, mode === 'dark' ? 16 : 12),
    '--rr-primary-border': tint(primary, mode === 'dark' ? 34 : 28),
    // 功能色 + 弱底
    '--rr-color-success': f.success,
    '--rr-success': f.success,
    '--rr-success-weak': f.successWeak,
    '--rr-color-warning': f.warning,
    '--rr-warning': f.warning,
    '--rr-warning-weak': f.warningWeak,
    '--rr-color-error': f.error,
    '--rr-danger': f.error,
    '--rr-danger-weak': f.errorWeak,
    '--rr-color-info': primary,
    '--rr-purple': f.purple,
    '--rr-cyan': f.cyan,
    // 中性面与边框
    '--rr-bg-layout': n.bgLayout,
    '--rr-bg': n.bgLayout,
    '--rr-bg-container': n.bgContainer,
    '--rr-surface': n.bgContainer,
    '--rr-surface-2': n.bgSubtle,
    '--rr-bg-elevated': n.bgElevated,
    '--rr-elev': n.bgElevated,
    '--rr-border': n.border,
    '--rr-border-secondary': n.borderSecondary,
    '--rr-border-2': n.borderStrong,
    // 文本
    '--rr-text': n.text,
    '--rr-text-2': n.textSecondary,
    '--rr-text-secondary': n.textSecondary,
    '--rr-text-3': n.textTertiary,
    '--rr-text-tertiary': n.textTertiary,
    '--rr-fill': n.fill,
    // 阴影
    '--rr-shadow': s.base,
    '--rr-shadow-lg': s.lg,
    // 字体
    '--rr-font-sans': override?.fontFamily ?? FONT_SANS,
    '--rr-font-mono': FONT_MONO,
  }
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v)

  root.setAttribute('data-theme', mode)
  // 设计稿原型用 .rr-dark 控制暗色；保留该 class 以兼容直接移植的样式片段。
  root.classList.toggle('rr-dark', mode === 'dark')
  root.style.colorScheme = mode
}
