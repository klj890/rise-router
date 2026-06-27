import type { ReactNode } from 'react'

export type PillTone = 'success' | 'warning' | 'danger' | 'primary' | 'purple' | 'cyan' | 'neutral'

const TONE: Record<PillTone, { bg: string; fg: string }> = {
  success: { bg: 'var(--rr-success-weak)', fg: 'var(--rr-success)' },
  warning: { bg: 'var(--rr-warning-weak)', fg: 'var(--rr-warning)' },
  danger: { bg: 'var(--rr-danger-weak)', fg: 'var(--rr-danger)' },
  primary: { bg: 'var(--rr-primary-weak)', fg: 'var(--rr-primary)' },
  purple: { bg: 'var(--rr-primary-weak)', fg: 'var(--rr-purple)' },
  cyan: { bg: 'var(--rr-surface-2)', fg: 'var(--rr-cyan)' },
  neutral: { bg: 'var(--rr-surface-2)', fg: 'var(--rr-text-2)' },
}

interface StatusPillProps {
  tone?: PillTone
  children: ReactNode
  /** 前置状态点 */
  dot?: boolean
}

/** 状态药丸：语义弱底 + 实色字，替代裸 Tag。明暗自动联动（走 CSS 变量）。 */
export default function StatusPill({ tone = 'neutral', children, dot }: StatusPillProps) {
  const c = TONE[tone]
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 5,
        padding: '2px 9px',
        borderRadius: 7,
        background: c.bg,
        color: c.fg,
        fontSize: 12,
        fontWeight: 600,
        lineHeight: 1.5,
        whiteSpace: 'nowrap',
      }}
    >
      {dot && (
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: c.fg, display: 'inline-block' }} />
      )}
      {children}
    </span>
  )
}
