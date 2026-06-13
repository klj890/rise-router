import { useThemeStore } from './store'
import type { BrandOverride } from './types'

/**
 * 从后端拉取 per-租户白标配置（私有化场景）。
 * 后端 `/api/branding` 未就绪时静默忽略，沿用本地持久化 / 内置默认主题。
 * 后端实现后即自动生效，无需改前端。
 */
export async function loadBranding(): Promise<void> {
  try {
    const res = await fetch('/api/branding', { headers: { Accept: 'application/json' } })
    if (!res.ok) return
    const data = (await res.json()) as { brand?: BrandOverride }
    if (data?.brand) useThemeStore.getState().setBrand(data.brand)
  } catch {
    // 后端未就绪：忽略
  }
}
