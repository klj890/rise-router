import { useMemo, useState } from 'react'
import { Input, Button, Slider, Alert, Empty, Spin, Table, Tag, message } from 'antd'
import { useMutation, useQuery } from '@tanstack/react-query'
import { ThunderboltOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { PageHeader, SectionCard, StatusPill } from '../../components/ui'
import { pricePreview, type PricePreview } from '../../api/admin'
import { api } from '../../api/client'
import { useAuthStore } from '../../store/auth'

// 解耦五要素：模型 × 渠道 × 分组 → 价格 − 折扣
const FACTORS = [
  { key: '模型', en: 'Model', desc: '能力规格与计量单位', tone: 'var(--rr-primary)' },
  { key: '渠道', en: 'Channel', desc: '上游供应与成本来源', tone: 'var(--rr-cyan)' },
  { key: '用户分组', en: 'Group', desc: '套餐与归属档位', tone: 'var(--rr-purple)' },
]

interface PriceRow {
  id: number
  model_id: number
  group_id: number | null
  currency: string
  billing_unit: string
  unit_prices: Record<string, number>
}

function unitPriceOf(p: unknown, key: string): number {
  const o = (p ?? {}) as Record<string, number>
  return Number(o[key] ?? 0)
}

export default function PricingFive() {
  const navigate = useNavigate()
  const adminToken = useAuthStore((s) => s.adminToken)
  const [model, setModel] = useState('')
  const [group, setGroup] = useState('')
  const [inputTokens, setInputTokens] = useState(500_000)
  const [outputTokens, setOutputTokens] = useState(500_000)
  const [costRate, setCostRate] = useState(55) // 渠道成本占售价比（估算，可调）

  const preview = useMutation({ mutationFn: () => pricePreview(model, group) })
  const result: PricePreview | undefined = preview.data

  const rules = useQuery({
    queryKey: ['admin', '/api/pricing/prices'],
    queryFn: async () => (await api.get<PriceRow[]>('/api/pricing/prices')).data,
    enabled: !!adminToken,
  })

  // 计算器：售价 = 单价(元/百万) × 用量/1e6；成本/毛利按可调成本率估算。
  const calc = useMemo(() => {
    if (!result) return null
    const pin = unitPriceOf(result.final_unit_prices, 'input')
    const pout = unitPriceOf(result.final_unit_prices, 'output')
    const sell = (pin * inputTokens) / 1e6 + (pout * outputTokens) / 1e6
    const cost = (sell * costRate) / 100
    const margin = sell - cost
    const marginPct = sell > 0 ? (margin / sell) * 100 : 0
    return { pin, pout, sell, cost, margin, marginPct }
  }, [result, inputTokens, outputTokens, costRate])

  const fmt = (n: number) => n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 4 })

  return (
    <div>
      <PageHeader
        title="定价五要素"
        subtitle="摒弃 new-api 的多层倍率叠加，改为 模型 / 渠道 / 价格 / 用户分组 / 折扣 五个要素完全解耦的关系模型 —— 每条定价规则独立、可组合、可追溯。"
      />

      {/* 解耦关系流程条 */}
      <SectionCard style={{ marginBottom: 16 }}>
        <div style={{ display: 'flex', alignItems: 'stretch', gap: 12, flexWrap: 'wrap' }}>
          {FACTORS.map((f, i) => (
            <div key={f.key} style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 160 }}>
              <div className="rr-stat-cell" style={{ flex: 1 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span style={{ width: 7, height: 7, borderRadius: '50%', background: f.tone }} />
                  <span style={{ fontSize: 12, color: 'var(--rr-text-2)' }}>{f.key}</span>
                </div>
                <div style={{ fontWeight: 700, fontSize: 16, marginTop: 4, color: 'var(--rr-text)' }}>{f.en}</div>
                <div style={{ fontSize: 11.5, color: 'var(--rr-text-3)', marginTop: 2 }}>{f.desc}</div>
              </div>
              <span style={{ color: 'var(--rr-text-3)', fontSize: 16 }}>{i < FACTORS.length - 1 ? '×' : '→'}</span>
            </div>
          ))}
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 240 }}>
            <div className="rr-stat-cell" style={{ flex: 1 }}>
              <div style={{ fontSize: 12, color: 'var(--rr-text-2)' }}>价格</div>
              <div style={{ fontWeight: 700, fontSize: 16, marginTop: 4 }}>Price</div>
              <div style={{ fontSize: 11.5, color: 'var(--rr-text-3)', marginTop: 2 }}>输入 / 输出基础单价</div>
            </div>
            <span style={{ color: 'var(--rr-text-3)', fontSize: 16 }}>−</span>
            <div className="rr-stat-cell" style={{ flex: 1 }}>
              <div style={{ fontSize: 12, color: 'var(--rr-success)' }}>折扣</div>
              <div style={{ fontWeight: 700, fontSize: 16, marginTop: 4 }}>Discount</div>
              <div style={{ fontSize: 11.5, color: 'var(--rr-text-3)', marginTop: 2 }}>分组 / 时段 / 阶梯优惠</div>
            </div>
          </div>
        </div>
      </SectionCard>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
        {/* 价格预览计算器 */}
        <SectionCard
          title={
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              <ThunderboltOutlined /> 价格预览 · 所见即所得
            </span>
          }
        >
          <div style={{ display: 'flex', gap: 10, marginBottom: 14 }}>
            <Input placeholder="模型 slug，如 gpt-4o" value={model} onChange={(e) => setModel(e.target.value)} />
            <Input
              placeholder="分组 slug（留空=默认价）"
              value={group}
              onChange={(e) => setGroup(e.target.value)}
              style={{ width: 200 }}
            />
            <Button
              type="primary"
              loading={preview.isPending}
              disabled={!model.trim()}
              onClick={() =>
                preview.mutate(undefined, {
                  onError: (e) =>
                    message.error(
                      (e as { localizedMessage?: string }).localizedMessage ??
                        '预览失败：请检查 slug 是否存在、是否已配价',
                    ),
                })
              }
            >
              预览
            </Button>
          </div>

          {!result ? (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="输入模型与分组后预览生效单价" style={{ padding: 24 }} />
          ) : (
            <>
              <div style={{ display: 'flex', gap: 18, marginBottom: 14, flexWrap: 'wrap' }}>
                <div>
                  <div className="rr-eyebrow">计费量纲</div>
                  <div className="rr-num" style={{ fontSize: 15, marginTop: 2 }}>{result.billing_unit}</div>
                </div>
                <div>
                  <div className="rr-eyebrow">折扣系数</div>
                  <div className="rr-num" style={{ fontSize: 15, marginTop: 2 }}>{result.discount_factor}</div>
                </div>
                <div>
                  <div className="rr-eyebrow">价格版本</div>
                  <div className="rr-num" style={{ fontSize: 15, marginTop: 2 }}>v{result.price_version}</div>
                </div>
              </div>

              <div style={{ marginBottom: 8 }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, marginBottom: 4 }}>
                  <span style={{ color: 'var(--rr-text-2)' }}>输入用量（token）</span>
                  <span className="rr-num">{inputTokens.toLocaleString()}</span>
                </div>
                <Slider min={0} max={5_000_000} step={50_000} value={inputTokens} onChange={setInputTokens} tooltip={{ open: false }} />
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, margin: '4px 0' }}>
                  <span style={{ color: 'var(--rr-text-2)' }}>输出用量（token）</span>
                  <span className="rr-num">{outputTokens.toLocaleString()}</span>
                </div>
                <Slider min={0} max={5_000_000} step={50_000} value={outputTokens} onChange={setOutputTokens} tooltip={{ open: false }} />
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12.5, margin: '4px 0' }}>
                  <span style={{ color: 'var(--rr-text-2)' }}>渠道成本率（估算）</span>
                  <span className="rr-num">{costRate}%</span>
                </div>
                <Slider min={0} max={100} value={costRate} onChange={setCostRate} tooltip={{ open: false }} />
              </div>

              {calc && (
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10, marginTop: 8 }}>
                  <div className="rr-stat-cell">
                    <div className="rr-eyebrow">生效单价（输入/输出，元/百万）</div>
                    <div className="rr-num" style={{ fontSize: 15, marginTop: 4 }}>{fmt(calc.pin)} / {fmt(calc.pout)}</div>
                  </div>
                  <div className="rr-stat-cell">
                    <div className="rr-eyebrow">预估售价</div>
                    <div className="rr-num" style={{ fontSize: 18, marginTop: 4, color: 'var(--rr-primary)' }}>¥{fmt(calc.sell)}</div>
                  </div>
                  <div className="rr-stat-cell">
                    <div className="rr-eyebrow">渠道成本（估算）</div>
                    <div className="rr-num" style={{ fontSize: 15, marginTop: 4 }}>¥{fmt(calc.cost)}</div>
                  </div>
                  <div className="rr-stat-cell">
                    <div className="rr-eyebrow">毛利 / 毛利率</div>
                    <div className="rr-num" style={{ fontSize: 15, marginTop: 4, color: 'var(--rr-success)' }}>
                      ¥{fmt(calc.margin)} · {calc.marginPct.toFixed(1)}%
                    </div>
                  </div>
                </div>
              )}

              {result.applied_discounts.length > 0 && (
                <div style={{ marginTop: 12 }}>
                  <div className="rr-eyebrow" style={{ marginBottom: 6 }}>命中折扣</div>
                  {result.applied_discounts.map((d) => (
                    <Tag key={d.id} color={d.applied ? 'green' : 'default'} style={{ marginBottom: 4 }}>
                      {d.name} · {d.kind} {d.value}
                    </Tag>
                  ))}
                </div>
              )}
            </>
          )}
        </SectionCard>

        {/* 定价规则表 */}
        <SectionCard
          title="定价规则"
          extra={<Button size="small" onClick={() => navigate('/admin/prices')}>管理价格</Button>}
          flush
        >
          {!adminToken ? (
            <div style={{ padding: 24 }}>
              <Alert type="info" showIcon message="设置管理令牌后可在此预览价格规则" />
            </div>
          ) : rules.isLoading ? (
            <div style={{ padding: 40, textAlign: 'center' }}>
              <Spin />
            </div>
          ) : (
            <Table<PriceRow>
              rowKey="id"
              size="small"
              pagination={{ pageSize: 8 }}
              dataSource={rules.data ?? []}
              columns={[
                { title: '模型 ID', dataIndex: 'model_id', width: 90, render: (v) => <span className="rr-num">#{v}</span> },
                {
                  title: '分组',
                  dataIndex: 'group_id',
                  render: (v) => (v == null ? <StatusPill>默认价</StatusPill> : <span className="rr-num">#{v}</span>),
                },
                { title: '量纲', dataIndex: 'billing_unit' },
                {
                  title: '单价',
                  dataIndex: 'unit_prices',
                  render: (v: Record<string, number>) => (
                    <span className="rr-num" style={{ fontSize: 12 }}>
                      {Object.entries(v ?? {})
                        .map(([k, val]) => `${k}:${val}`)
                        .join(' / ')}
                    </span>
                  ),
                },
              ]}
            />
          )}
        </SectionCard>
      </div>
    </div>
  )
}
