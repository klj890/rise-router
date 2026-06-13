import type { ResolvedMode } from './types'

// 克制专业（Linear/Vercel 一派）：自托管字体，含中文回退。
export const FONT_SANS =
  "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif"
export const FONT_MONO =
  "'JetBrains Mono', 'SFMono-Regular', Menlo, Consolas, 'Liberation Mono', monospace"

export interface NeutralTokens {
  bgLayout: string
  bgContainer: string
  bgElevated: string
  border: string
  borderSecondary: string
  text: string
  textSecondary: string
  textTertiary: string
  /** 极淡填充（hover 背景等） */
  fill: string
}

// 暗色优先：低饱和冷灰阶；浅色为匹配的白底灰阶。
export const NEUTRAL: Record<ResolvedMode, NeutralTokens> = {
  dark: {
    bgLayout: '#0B0D11',
    bgContainer: '#14171D',
    bgElevated: '#1B1F27',
    border: 'rgba(255, 255, 255, 0.08)',
    borderSecondary: 'rgba(255, 255, 255, 0.05)',
    text: '#E6E9EF',
    textSecondary: '#9BA3B4',
    textTertiary: '#6B7385',
    fill: 'rgba(255, 255, 255, 0.04)',
  },
  light: {
    bgLayout: '#F7F8FA',
    bgContainer: '#FFFFFF',
    bgElevated: '#FFFFFF',
    border: '#E6E8EC',
    borderSecondary: '#F0F1F3',
    text: '#1A1D23',
    textSecondary: '#5B6472',
    textTertiary: '#9098A6',
    fill: 'rgba(0, 0, 0, 0.03)',
  },
}

// 功能色：明暗共用。
export const FUNCTIONAL = {
  success: '#1FB57A',
  warning: '#E8A33D',
  error: '#E5484D',
  info: '#4C8DF5',
}

export const RADIUS_BASE = 8
export const CONTROL_HEIGHT = 36
