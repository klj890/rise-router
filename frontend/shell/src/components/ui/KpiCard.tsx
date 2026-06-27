import type { ReactNode } from 'react'
import Sparkline from './Sparkline'

interface KpiCardProps {
  label: ReactNode
  value: ReactNode
  /** 副信息（环比、范围说明等） */
  hint?: ReactNode
  /** hint 语义色：positive 绿 / negative 红 / 默认次级文本 */
  hintTone?: 'positive' | 'negative' | 'muted'
  suffix?: ReactNode
  /** 主色高亮数字 */
  accent?: boolean
  /** 右侧/底部 sparkline 数据 */
  spark?: number[]
  sparkColor?: string
}

/** KPI 卡：标签 + 等宽大数字(27/600) + 环比 + 可选 sparkline。设计稿总览/CRM/运维通用。 */
export default function KpiCard({
  label,
  value,
  hint,
  hintTone = 'muted',
  suffix,
  accent,
  spark,
  sparkColor,
}: KpiCardProps) {
  const hintColor =
    hintTone === 'positive'
      ? 'var(--rr-success)'
      : hintTone === 'negative'
        ? 'var(--rr-danger)'
        : 'var(--rr-text-3)'
  return (
    <div className="rr-card" style={{ padding: 18, height: '100%' }}>
      <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', fontWeight: 500 }}>{label}</div>
      <div style={{ marginTop: 10, display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span
          className="rr-num"
          style={{ fontSize: 27, fontWeight: 600, lineHeight: 1.1, color: accent ? 'var(--rr-primary)' : 'var(--rr-text)' }}
        >
          {value}
        </span>
        {suffix && <span style={{ fontSize: 13, color: 'var(--rr-text-2)' }}>{suffix}</span>}
      </div>
      <div style={{ marginTop: 8, display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', gap: 8 }}>
        {hint != null ? (
          <span className="rr-num" style={{ fontSize: 12, color: hintColor }}>
            {hint}
          </span>
        ) : (
          <span />
        )}
        {spark && spark.length > 1 && (
          <Sparkline data={spark} width={92} height={28} color={sparkColor ?? 'var(--rr-primary)'} />
        )}
      </div>
    </div>
  )
}
