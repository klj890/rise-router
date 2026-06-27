import { useMemo, useState } from 'react'
import { Segmented } from 'antd'
import { useQuery } from '@tanstack/react-query'
import {
  ResponsiveContainer,
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
} from 'recharts'
import { api } from '../api/client'
import { PageHeader, KpiCard, SectionCard, StatusPill } from '../components/ui'

interface ReadyResp {
  status: string
  db: string
}

// —— 演示用 mock 数据（设计稿即全 mock；接口就绪后替换）——
const RANGE_DAYS: Record<string, number> = { '7': 7, '30': 30, '90': 90 }

function trend(days: number, base: number, amp: number, seed: number) {
  return Array.from({ length: days }, (_, i) => {
    const wave = Math.sin((i + seed) * 0.7) + Math.cos((i + seed) * 0.31)
    return Math.max(0, Math.round(base + wave * amp + (i / days) * base * 0.25))
  })
}

const CHANNELS = [
  { name: 'OpenAI 官方', adapter: 'openai', usage: '512K', err: 0.04, tone: 'success' as const, label: '健康' },
  { name: 'Azure OpenAI', adapter: 'azure', usage: '486K', err: 0.02, tone: 'success' as const, label: '健康' },
  { name: 'Anthropic', adapter: 'anthropic', usage: '198K', err: 0.11, tone: 'success' as const, label: '健康' },
  { name: 'Google Vertex', adapter: 'vertex', usage: '54K', err: 2.3, tone: 'warning' as const, label: '降级' },
  { name: '通义千问', adapter: 'qwen', usage: '321K', err: 0.07, tone: 'success' as const, label: '健康' },
]

const MODEL_USAGE = [
  { name: 'gpt-4o', v: 842 },
  { name: 'claude-3.7', v: 613 },
  { name: 'deepseek-v3', v: 521 },
  { name: 'qwen-max', v: 388 },
  { name: 'gemini-2.5', v: 245 },
  { name: 'gpt-4o-mini', v: 192 },
]

const RECENT = [
  { id: 'req_7f3a91', model: 'gpt-4o', org: 'Acme 智能科技', tokens: '3,182', cost: '¥0.42', ms: 812, ok: true },
  { id: 'req_7f3a8c', model: 'claude-3.7-sonnet', org: '云帆数据', tokens: '1,904', cost: '¥0.31', ms: 1240, ok: true },
  { id: 'req_7f3a72', model: 'deepseek-v3', org: 'Acme 智能科技', tokens: '5,021', cost: '¥0.08', ms: 640, ok: true },
  { id: 'req_7f3a55', model: 'gemini-2.5-pro', org: '星河科技', tokens: '—', cost: '—', ms: 0, ok: false },
  { id: 'req_7f3a41', model: 'qwen-max', org: '云帆数据', tokens: '2,210', cost: '¥0.12', ms: 540, ok: true },
]

/** 概览页：KPI + 请求/成本趋势 + 渠道健康 + 模型用量 + 最近调用。实时探测 /readyz 作系统健康指示。 */
export default function DashboardPage() {
  const [range, setRange] = useState('7')
  const ready = useQuery<ReadyResp>({
    queryKey: ['readyz'],
    queryFn: async () => (await api.get<ReadyResp>('/readyz')).data,
    refetchInterval: 10000,
    retry: false,
  })
  const backendUp = ready.isSuccess
  const dbUp = ready.data?.db === 'up'

  const days = RANGE_DAYS[range]
  const series = useMemo(() => {
    const req = trend(days, 1200, 220, 1)
    const cost = trend(days, 340, 70, 4)
    return req.map((v, i) => ({ day: `D${i + 1}`, 请求: v, 成本: cost[i] }))
  }, [days])
  const reqSpark = useMemo(() => trend(14, 1200, 220, 1), [])
  const costSpark = useMemo(() => trend(14, 340, 70, 4), [])
  const latSpark = useMemo(() => trend(14, 820, 90, 2), [])
  const maxUsage = Math.max(...MODEL_USAGE.map((m) => m.v))

  return (
    <div>
      <PageHeader
        title="总览"
        subtitle="平台请求、成本、渠道健康与最近调用的实时概览。"
        extra={
          <Segmented
            value={range}
            onChange={(v) => setRange(v as string)}
            options={[
              { label: '近 7 天', value: '7' },
              { label: '近 30 天', value: '30' },
              { label: '近 90 天', value: '90' },
            ]}
          />
        }
      />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label="今日调用" value="1,284,920" accent hint="较昨日 +8.2%" hintTone="positive" spark={reqSpark} />
        <KpiCard label="本月成本" value="¥86,420" suffix="" hint="较上月 +12.4%" hintTone="negative" spark={costSpark} sparkColor="var(--rr-warning)" />
        <KpiCard label="活跃密钥" value="142" hint="本月新增 18" hintTone="muted" />
        <KpiCard label="平均时延" value="812" suffix="ms" hint="P99 1.24s" hintTone="muted" spark={latSpark} sparkColor="var(--rr-cyan)" />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1.6fr 1fr', gap: 16, marginBottom: 16 }}>
        <SectionCard title="请求 / 成本趋势">
          <div style={{ width: '100%', height: 280 }}>
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={series} margin={{ top: 8, right: 8, left: -8, bottom: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="var(--rr-border)" vertical={false} />
                <XAxis dataKey="day" stroke="var(--rr-text-3)" fontSize={11} tickLine={false} />
                <YAxis stroke="var(--rr-text-3)" fontSize={11} tickLine={false} axisLine={false} />
                <Tooltip
                  contentStyle={{
                    background: 'var(--rr-elev)',
                    border: '1px solid var(--rr-border)',
                    borderRadius: 10,
                    fontSize: 12,
                  }}
                />
                <Line type="monotone" dataKey="请求" stroke="#7C75F5" strokeWidth={2} dot={false} />
                <Line type="monotone" dataKey="成本" stroke="#34C5D6" strokeWidth={2} dot={false} />
              </LineChart>
            </ResponsiveContainer>
          </div>
        </SectionCard>

        <SectionCard title="渠道健康度">
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            {CHANNELS.map((c) => (
              <div
                key={c.name}
                style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '9px 0', borderBottom: '1px solid var(--rr-border-secondary)' }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 13.5, fontWeight: 500, color: 'var(--rr-text)' }}>{c.name}</div>
                  <div className="rr-num" style={{ fontSize: 11.5, color: 'var(--rr-text-3)' }}>
                    {c.adapter} · {c.usage}
                  </div>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <span className="rr-num" style={{ fontSize: 12, color: c.err > 1 ? 'var(--rr-warning)' : 'var(--rr-text-2)' }}>
                    {c.err}%
                  </span>
                  <StatusPill tone={c.tone} dot>
                    {c.label}
                  </StatusPill>
                </div>
              </div>
            ))}
          </div>
        </SectionCard>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1.6fr', gap: 16 }}>
        <SectionCard title="模型用量 Top 6">
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            {MODEL_USAGE.map((m) => (
              <div key={m.name}>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, marginBottom: 4 }}>
                  <span className="rr-num" style={{ color: 'var(--rr-text-2)' }}>{m.name}</span>
                  <span className="rr-num" style={{ color: 'var(--rr-text)' }}>{m.v}K</span>
                </div>
                <div style={{ height: 6, borderRadius: 3, background: 'var(--rr-surface-2)', overflow: 'hidden' }}>
                  <div style={{ width: `${(m.v / maxUsage) * 100}%`, height: '100%', background: 'var(--rr-primary)' }} />
                </div>
              </div>
            ))}
          </div>
        </SectionCard>

        <SectionCard
          title="最近调用"
          extra={
            <span style={{ display: 'inline-flex', gap: 10, alignItems: 'center' }}>
              <StatusPill tone={ready.isLoading ? 'warning' : backendUp ? 'success' : 'danger'} dot>
                后端 {ready.isLoading ? '连接中' : backendUp ? '在线' : '离线'}
              </StatusPill>
              <StatusPill tone={ready.isLoading ? 'warning' : dbUp ? 'success' : 'warning'} dot>
                DB {ready.isLoading ? '探测中' : dbUp ? '正常' : ready.data?.db ?? '异常'}
              </StatusPill>
            </span>
          }
          flush
        >
          <table className="rr-table">
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>请求 ID</th>
                <th style={{ textAlign: 'left' }}>模型</th>
                <th style={{ textAlign: 'left' }}>租户</th>
                <th style={{ textAlign: 'right' }}>Tokens</th>
                <th style={{ textAlign: 'right' }}>计费</th>
                <th style={{ textAlign: 'right' }}>耗时</th>
                <th style={{ textAlign: 'right' }}>状态</th>
              </tr>
            </thead>
            <tbody>
              {RECENT.map((r) => (
                <tr key={r.id}>
                  <td className="rr-num" style={{ color: 'var(--rr-text-2)' }}>{r.id}</td>
                  <td>{r.model}</td>
                  <td style={{ color: 'var(--rr-text-2)' }}>{r.org}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{r.tokens}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{r.cost}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{r.ms ? `${r.ms}ms` : '—'}</td>
                  <td style={{ textAlign: 'right' }}>
                    <StatusPill tone={r.ok ? 'success' : 'danger'} dot>
                      {r.ok ? '成功' : '失败'}
                    </StatusPill>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </SectionCard>
      </div>
    </div>
  )
}
