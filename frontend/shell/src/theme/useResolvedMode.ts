import { useEffect, useState } from 'react'
import { useThemeStore } from './store'
import type { ResolvedMode } from './types'

/** 把 mode(dark|light|system) 解析为具体明暗，并随系统偏好实时更新。 */
export function useResolvedMode(): ResolvedMode {
  const mode = useThemeStore((s) => s.mode)
  const [sysDark, setSysDark] = useState(
    () => window.matchMedia('(prefers-color-scheme: dark)').matches,
  )
  useEffect(() => {
    if (mode !== 'system') return
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = (e: MediaQueryListEvent) => setSysDark(e.matches)
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [mode])
  return mode === 'system' ? (sysDark ? 'dark' : 'light') : mode
}
