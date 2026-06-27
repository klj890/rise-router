import { theme as antdTheme, type ThemeConfig } from 'antd'
import type { AccentId, AccentPreset, BrandOverride, ResolvedMode } from './types'
import {
  NEUTRAL,
  FUNCTIONAL,
  FONT_SANS,
  FONT_MONO,
  RADIUS_BASE,
  RADIUS_CARD,
  CONTROL_HEIGHT,
} from './tokens'

// 强调色预设：indigo（靛蓝紫）为设计稿品牌默认；其余证明"预设切换"能力。
// 暗色用更亮的值、浅色用更深的值以保证各自背景下的对比度。
export const ACCENTS: Record<AccentId, AccentPreset> = {
  indigo: { id: 'indigo', name: '靛蓝紫', dark: '#7C75F5', light: '#4F46E5' },
  aurora: { id: 'aurora', name: '极光青绿', dark: '#2EE6C0', light: '#0FB89A' },
  violet: { id: 'violet', name: '电光紫', dark: '#7C5CFF', light: '#6D45F0' },
  amber: { id: 'amber', name: '琥珀橙', dark: '#FF7A3C', light: '#E0621F' },
  blue: { id: 'blue', name: '深海蓝', dark: '#4C8DF5', light: '#2563EB' },
}

export const ACCENT_LIST = Object.values(ACCENTS)

export function resolvePrimary(
  accentId: AccentId,
  mode: ResolvedMode,
  override?: BrandOverride,
): string {
  if (override?.colorPrimary) return override.colorPrimary
  return ACCENTS[accentId][mode]
}

/** 半透明强调底（hover/selected）；用 color-mix 适配任意强调色。 */
function tint(primary: string, percent: number): string {
  return `color-mix(in srgb, ${primary} ${percent}%, transparent)`
}

/**
 * 由 (模式 × 强调色 × 白标覆盖) 合成 Ant Design 6 主题。
 * 组件级覆盖用于实现"克制专业"质感：细边框、无彩色发光、强调色克制使用。
 */
export function buildAntdTheme(
  mode: ResolvedMode,
  accentId: AccentId,
  override?: BrandOverride,
): ThemeConfig {
  const n = NEUTRAL[mode]
  const f = FUNCTIONAL[mode]
  const primary = resolvePrimary(accentId, mode, override)
  const radius = override?.borderRadius ?? RADIUS_BASE
  const fontFamily = override?.fontFamily ?? FONT_SANS

  return {
    algorithm: mode === 'dark' ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
    token: {
      colorPrimary: primary,
      colorInfo: primary,
      colorSuccess: f.success,
      colorWarning: f.warning,
      colorError: f.error,
      colorBgBase: n.bgLayout,
      colorBgLayout: n.bgLayout,
      colorBgContainer: n.bgContainer,
      colorBgElevated: n.bgElevated,
      colorBorder: n.border,
      colorBorderSecondary: n.borderSecondary,
      colorText: n.text,
      colorTextSecondary: n.textSecondary,
      colorTextTertiary: n.textTertiary,
      fontFamily,
      fontFamilyCode: FONT_MONO,
      fontSize: 14,
      borderRadius: radius,
      borderRadiusLG: RADIUS_CARD,
      borderRadiusSM: 6,
      controlHeight: CONTROL_HEIGHT,
      wireframe: false,
    },
    components: {
      Layout: {
        headerBg: n.bgContainer,
        siderBg: n.bgContainer,
        bodyBg: n.bgLayout,
        headerHeight: 56,
      },
      Menu: {
        itemBg: 'transparent',
        subMenuItemBg: 'transparent',
        itemSelectedBg: tint(primary, 12),
        itemSelectedColor: primary,
        itemHoverBg: n.fill,
        itemBorderRadius: radius,
        activeBarWidth: 0,
        activeBarBorderWidth: 0,
        // 暗色菜单走独立 token；克制：选中态用半透明底而非实色填充
        darkItemBg: 'transparent',
        darkSubMenuItemBg: 'transparent',
        darkItemSelectedBg: tint(primary, 14),
        darkItemSelectedColor: primary,
        darkItemHoverBg: n.fill,
      },
      Card: {
        colorBorderSecondary: n.border,
        borderRadiusLG: RADIUS_CARD,
        paddingLG: 20,
      },
      Button: {
        fontWeight: 500,
        borderRadius: radius,
        primaryShadow: 'none',
        defaultShadow: 'none',
        dangerShadow: 'none',
      },
      Input: {
        activeShadow: 'none',
        borderRadius: radius,
      },
      Table: {
        headerBg: n.bgSubtle,
        headerColor: n.textTertiary,
        rowHoverBg: n.fill,
        borderColor: n.borderSecondary,
        cellPaddingBlock: 14,
      },
      Drawer: {
        paddingLG: 24,
      },
      Statistic: {
        contentFontSize: 27,
      },
      Segmented: {
        borderRadius: radius,
        trackPadding: 3,
      },
    },
  }
}
