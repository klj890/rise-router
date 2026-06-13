export type ThemeMode = 'dark' | 'light' | 'system'
export type ResolvedMode = 'dark' | 'light'
export type AccentId = 'aurora' | 'violet' | 'amber' | 'blue'

/** per-租户白标覆盖；任一字段缺省则回落到预设值。 */
export interface BrandOverride {
  colorPrimary?: string
  logoUrl?: string
  appName?: string
  borderRadius?: number
  fontFamily?: string
}

export interface AccentPreset {
  id: AccentId
  name: string
  /** 暗色模式主色 */
  dark: string
  /** 浅色模式主色（通常更深，以满足白底 AA 对比度） */
  light: string
}
