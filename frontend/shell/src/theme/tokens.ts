import type { ResolvedMode } from './types'

// 克制专业（Linear/Vercel 一派）：自托管字体，含中文回退。
export const FONT_SANS =
  "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif"
export const FONT_MONO =
  "'JetBrains Mono', 'SFMono-Regular', Menlo, Consolas, 'Liberation Mono', monospace"

export interface NeutralTokens {
  bgLayout: string
  bgContainer: string
  /** 次级面（表头、内嵌区块）—— 设计稿 surface-2 */
  bgSubtle: string
  bgElevated: string
  border: string
  borderSecondary: string
  /** 略重的边框（输入框、强分隔）—— 设计稿 border-2 */
  borderStrong: string
  text: string
  textSecondary: string
  textTertiary: string
  /** 极淡填充（hover 背景等） */
  fill: string
}

// 暗色优先：低饱和冷灰阶；浅色为匹配的白底灰阶。数值对齐设计稿 :root / .rr-dark。
export const NEUTRAL: Record<ResolvedMode, NeutralTokens> = {
  dark: {
    bgLayout: '#0B0D11',
    bgContainer: '#14171D',
    bgSubtle: '#181C23',
    bgElevated: '#1A1E25',
    border: '#242A33',
    borderSecondary: 'rgba(255, 255, 255, 0.05)',
    borderStrong: '#2E353F',
    text: '#E7EAEF',
    textSecondary: '#9AA3AF',
    textTertiary: '#69717C',
    fill: 'rgba(255, 255, 255, 0.04)',
  },
  light: {
    bgLayout: '#F4F5F7',
    bgContainer: '#FFFFFF',
    bgSubtle: '#F8F9FB',
    bgElevated: '#FFFFFF',
    border: '#EBEDF0',
    borderSecondary: '#F0F1F3',
    borderStrong: '#E0E3E8',
    text: '#161A1F',
    textSecondary: '#59616B',
    textTertiary: '#8B929C',
    fill: 'rgba(0, 0, 0, 0.03)',
  },
}

export interface FunctionalTokens {
  success: string
  successWeak: string
  warning: string
  warningWeak: string
  /** danger == error */
  error: string
  errorWeak: string
  info: string
  purple: string
  cyan: string
}

// 功能色：明暗两套（对齐设计稿，保证各自背景下对比度与「弱底」可读）。
export const FUNCTIONAL: Record<ResolvedMode, FunctionalTokens> = {
  dark: {
    success: '#3CCB7F',
    successWeak: '#13261C',
    warning: '#E0A235',
    warningWeak: '#2A2113',
    error: '#F06A5D',
    errorWeak: '#2A1614',
    info: '#7C75F5',
    purple: '#A78BFA',
    cyan: '#34C5D6',
  },
  light: {
    success: '#1A9E54',
    successWeak: '#E7F6ED',
    warning: '#C77700',
    warningWeak: '#FBF1E0',
    error: '#D92D20',
    errorWeak: '#FCEBE9',
    info: '#4F46E5',
    purple: '#7C4DDB',
    cyan: '#0E9AAF',
  },
}

// 卡片/抽屉/下拉阴影：明暗两套。
export const SHADOW: Record<ResolvedMode, { base: string; lg: string }> = {
  dark: {
    base: '0 1px 2px rgba(0,0,0,.3), 0 1px 3px rgba(0,0,0,.45)',
    lg: '0 12px 34px rgba(0,0,0,.55)',
  },
  light: {
    base: '0 1px 2px rgba(16,24,40,.04), 0 1px 3px rgba(16,24,40,.06)',
    lg: '0 10px 30px rgba(16,24,40,.10)',
  },
}

export const RADIUS_BASE = 9
export const RADIUS_CARD = 14
export const CONTROL_HEIGHT = 36
