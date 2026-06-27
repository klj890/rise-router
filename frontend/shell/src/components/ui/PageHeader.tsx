import type { ReactNode } from 'react'

interface PageHeaderProps {
  title: ReactNode
  subtitle?: ReactNode
  /** 右侧操作区（按钮等） */
  extra?: ReactNode
}

/**
 * 页面标题区：H1(22/700) + 副标题 + 右侧操作槽。
 * 取代各页内联的 `<div flex justify-between><Title/>...</div>` 样板。
 */
export default function PageHeader({ title, subtitle, extra }: PageHeaderProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'space-between',
        gap: 16,
        marginBottom: 22,
      }}
    >
      <div style={{ minWidth: 0 }}>
        <h1
          style={{
            margin: 0,
            fontSize: 22,
            fontWeight: 700,
            letterSpacing: '-0.02em',
            color: 'var(--rr-text)',
            lineHeight: 1.3,
          }}
        >
          {title}
        </h1>
        {subtitle && (
          <div style={{ marginTop: 6, fontSize: 13.5, color: 'var(--rr-text-2)', maxWidth: 720, lineHeight: 1.6 }}>
            {subtitle}
          </div>
        )}
      </div>
      {extra && <div style={{ flexShrink: 0 }}>{extra}</div>}
    </div>
  )
}
