import { useId } from 'react'

interface SparklineProps {
  data: number[]
  width?: number
  height?: number
  /** 线/面积色，默认主色 */
  color?: string
  /** 是否填充渐变面积，默认 true */
  area?: boolean
  strokeWidth?: number
}

/**
 * 内联 SVG 微缩折线图（设计稿 KPI / 渠道健康 / 运维卡通用）。
 * 无依赖、随容器主题色走；data 少于 2 个点时不渲染。
 */
export default function Sparkline({
  data,
  width = 120,
  height = 36,
  color = 'var(--rr-primary)',
  area = true,
  strokeWidth = 1.8,
}: SparklineProps) {
  const gid = useId().replace(/:/g, '')
  if (!data || data.length < 2) return <svg width={width} height={height} />

  const min = Math.min(...data)
  const max = Math.max(...data)
  const span = max - min || 1
  const stepX = width / (data.length - 1)
  const pad = strokeWidth
  const points = data.map((v, i) => {
    const x = i * stepX
    const y = pad + (1 - (v - min) / span) * (height - pad * 2)
    return [x, y] as const
  })
  const line = points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`).join(' ')
  const fill = `${line} L${width} ${height} L0 ${height} Z`

  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      {area && (
        <>
          <defs>
            <linearGradient id={`sg-${gid}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={color} stopOpacity={0.18} />
              <stop offset="100%" stopColor={color} stopOpacity={0} />
            </linearGradient>
          </defs>
          <path d={fill} fill={`url(#sg-${gid})`} stroke="none" />
        </>
      )}
      <path d={line} fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  )
}
