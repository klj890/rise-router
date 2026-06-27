import type { ReactNode } from 'react'

interface SectionCardProps {
  title?: ReactNode
  extra?: ReactNode
  children: ReactNode
  /** 去掉内边距（如内嵌表格自带 padding 时） */
  flush?: boolean
  style?: React.CSSProperties
  bodyStyle?: React.CSSProperties
}

/** 标准内容卡：14px 圆角 + 细边 + 阴影；可选标题行。统一替代散乱的 AntD Card 用法。 */
export default function SectionCard({ title, extra, children, flush, style, bodyStyle }: SectionCardProps) {
  return (
    <div className="rr-card" style={style}>
      {(title || extra) && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 12,
            padding: '15px 18px',
            borderBottom: '1px solid var(--rr-border)',
          }}
        >
          {title ? <div style={{ fontSize: 15, fontWeight: 600, color: 'var(--rr-text)' }}>{title}</div> : <span />}
          {extra}
        </div>
      )}
      <div style={{ padding: flush ? 0 : 18, ...bodyStyle }}>{children}</div>
    </div>
  )
}
