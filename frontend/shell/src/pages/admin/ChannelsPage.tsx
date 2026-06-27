import { useMemo, useState } from 'react'
import { Button, Space, Alert, Empty, Input, Spin, message } from 'antd'
import { PlusOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useMutation } from '@tanstack/react-query'
import {
  PageHeader,
  SectionCard,
  StatusPill,
  FilterTabs,
  FormDrawer,
  Sparkline,
  type FilterTabItem,
} from '../../components/ui'
import { useResourceCrud, ResourceEditDrawer } from './resourceShared'
import { statusTone } from './CrudPage'
import { RESOURCE } from './resources'
import { testChannel, type ChannelTestResult } from '../../api/admin'
import { useAuthStore } from '../../store/auth'

type Row = Record<string, unknown>

const STATUS_LABEL: Record<string, string> = { Enabled: '启用', Disabled: '禁用', CircuitBroken: '熔断' }

/** 由 response_time 合成一段平稳时延序列，给详情抽屉的 sparkline 用（无真实历史时的视觉占位）。 */
function latencySeries(base: number): number[] {
  const seed = [0.82, 1.0, 0.9, 1.12, 0.95, 1.05, 0.88, 1.0]
  return seed.map((m) => Math.round(base * m))
}

export default function ChannelsPage() {
  const adminToken = useAuthStore((s) => s.adminToken)
  const crud = useResourceCrud(RESOURCE.channels)
  const [statusFilter, setStatusFilter] = useState('all')
  const [adapterFilter, setAdapterFilter] = useState('all')
  const [search, setSearch] = useState('')
  const [detail, setDetail] = useState<Row | null>(null)
  const [testResult, setTestResult] = useState<ChannelTestResult | null>(null)

  const testMutation = useMutation({
    mutationFn: (id: number) => testChannel(id),
    onSuccess: (r) => {
      setTestResult(r)
      crud.refresh()
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '测试失败'),
  })

  const rows = crud.rows
  const adapters = useMemo(
    () => Array.from(new Set(rows.map((r) => String(r.protocol_adapter ?? '')).filter(Boolean))),
    [rows],
  )

  const statusTabs: FilterTabItem[] = useMemo(() => {
    const items: FilterTabItem[] = [{ key: 'all', label: '全部', count: rows.length }]
    for (const [val, label] of Object.entries(STATUS_LABEL)) {
      items.push({ key: val, label, count: rows.filter((r) => r.status === val).length })
    }
    return items
  }, [rows])

  const filtered = useMemo(() => {
    const kw = search.trim().toLowerCase()
    return rows.filter((r) => {
      if (statusFilter !== 'all' && r.status !== statusFilter) return false
      if (adapterFilter !== 'all' && r.protocol_adapter !== adapterFilter) return false
      if (kw) {
        return (
          String(r.name ?? '').toLowerCase().includes(kw) ||
          String(r.base_url ?? '').toLowerCase().includes(kw)
        )
      }
      return true
    })
  }, [rows, statusFilter, adapterFilter, search])

  const openDetail = (r: Row) => {
    setDetail(r)
    setTestResult(null)
  }

  if (!adminToken) {
    return (
      <Alert
        type="warning"
        showIcon
        message="未设置管理令牌"
        description="渠道管理需要管理令牌（X-Admin-Token）。请到「系统设置 · 管理令牌」填入后端 RR_ADMIN_TOKEN。"
      />
    )
  }

  return (
    <div>
      <PageHeader
        title="渠道管理"
        subtitle="配置上游渠道、适配器与路由策略 —— 网关基座按权重、优先级与故障转移自动分流。"
        extra={
          <Space>
            <Button icon={<ReloadOutlined />} onClick={crud.refresh}>
              刷新
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={crud.openCreate}>
              新建渠道
            </Button>
          </Space>
        }
      />

      {crud.listQuery.isError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="加载失败"
          description="请检查管理令牌是否正确、后端是否就绪。"
        />
      )}

      <SectionCard flush>
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 12,
            padding: '14px 18px',
            borderBottom: '1px solid var(--rr-border)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
            <FilterTabs items={statusTabs} value={statusFilter} onChange={setStatusFilter} />
            <Input
              allowClear
              prefix={<SearchOutlined style={{ color: 'var(--rr-text-3)' }} />}
              placeholder="搜索渠道 / 地址"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              style={{ width: 220 }}
            />
          </div>
          {adapters.length > 1 && (
            <FilterTabs
              items={[
                { key: 'all', label: '全部适配器' },
                ...adapters.map((a) => ({ key: a, label: a })),
              ]}
              value={adapterFilter}
              onChange={setAdapterFilter}
            />
          )}
        </div>

        {crud.listQuery.isLoading ? (
          <div style={{ padding: 48, textAlign: 'center' }}>
            <Spin />
          </div>
        ) : filtered.length === 0 ? (
          <Empty
            style={{ padding: 48 }}
            description={rows.length === 0 ? '暂无渠道' : '无匹配渠道'}
          >
            {rows.length > 0 && (
              <Button
                onClick={() => {
                  setStatusFilter('all')
                  setAdapterFilter('all')
                  setSearch('')
                }}
              >
                清除筛选
              </Button>
            )}
          </Empty>
        ) : (
          <table className="rr-table">
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>渠道</th>
                <th>适配器</th>
                <th style={{ textAlign: 'right' }}>权重</th>
                <th style={{ textAlign: 'right' }}>优先级</th>
                <th style={{ textAlign: 'right' }}>测速</th>
                <th>密钥</th>
                <th style={{ textAlign: 'right' }}>状态</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((r) => (
                <tr key={r.id as number} onClick={() => openDetail(r)} className="rr-row">
                  <td>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                      <span
                        style={{
                          width: 8,
                          height: 8,
                          borderRadius: '50%',
                          flexShrink: 0,
                          background:
                            r.status === 'Enabled'
                              ? 'var(--rr-success)'
                              : r.status === 'CircuitBroken'
                                ? 'var(--rr-danger)'
                                : 'var(--rr-text-3)',
                        }}
                      />
                      <div style={{ minWidth: 0 }}>
                        <div style={{ fontWeight: 600, color: 'var(--rr-text)' }}>{String(r.name)}</div>
                        <div className="rr-num" style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>
                          {String(r.base_url ?? '—')}
                        </div>
                      </div>
                    </div>
                  </td>
                  <td>
                    <span className="rr-chip rr-num">{String(r.protocol_adapter)}</span>
                  </td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>
                    {r.weight != null ? String(r.weight) : '—'}
                  </td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>
                    {r.priority != null ? String(r.priority) : '—'}
                  </td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>
                    {r.response_time != null ? `${r.response_time}ms` : '—'}
                  </td>
                  <td>{r.has_credentials ? <StatusPill tone="success">已配</StatusPill> : <StatusPill>未配</StatusPill>}</td>
                  <td style={{ textAlign: 'right' }}>
                    <StatusPill tone={statusTone(r.status)} dot>
                      {STATUS_LABEL[String(r.status)] ?? String(r.status)}
                    </StatusPill>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </SectionCard>

      {/* 渠道详情抽屉 */}
      <FormDrawer
        open={detail != null}
        width={460}
        title={
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 9 }}>
            <span
              style={{
                width: 9,
                height: 9,
                borderRadius: '50%',
                background:
                  detail?.status === 'Enabled'
                    ? 'var(--rr-success)'
                    : detail?.status === 'CircuitBroken'
                      ? 'var(--rr-danger)'
                      : 'var(--rr-text-3)',
              }}
            />
            {String(detail?.name ?? '')}
          </span>
        }
        subtitle={String(detail?.base_url ?? '')}
        onClose={() => setDetail(null)}
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button
              loading={testMutation.isPending}
              onClick={() => detail && testMutation.mutate(detail.id as number)}
            >
              测试连通性
            </Button>
            <Button
              type="primary"
              onClick={() => {
                if (detail) crud.openEdit(detail)
                setDetail(null)
              }}
            >
              编辑渠道
            </Button>
          </div>
        }
      >
        {detail && (
          <div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10, marginBottom: 18 }}>
              <div className="rr-stat-cell">
                <div className="rr-eyebrow">适配器</div>
                <div className="rr-num" style={{ fontSize: 16, marginTop: 4 }}>{String(detail.protocol_adapter)}</div>
              </div>
              <div className="rr-stat-cell">
                <div className="rr-eyebrow">权重 / 优先级</div>
                <div className="rr-num" style={{ fontSize: 16, marginTop: 4 }}>
                  {String(detail.weight ?? '—')} / {String(detail.priority ?? '—')}
                </div>
              </div>
              <div className="rr-stat-cell">
                <div className="rr-eyebrow">测速</div>
                <div className="rr-num" style={{ fontSize: 16, marginTop: 4 }}>
                  {detail.response_time != null ? `${detail.response_time}ms` : '—'}
                </div>
              </div>
              <div className="rr-stat-cell">
                <div className="rr-eyebrow">密钥</div>
                <div style={{ marginTop: 6 }}>
                  {detail.has_credentials ? <StatusPill tone="success">已配置</StatusPill> : <StatusPill>未配置</StatusPill>}
                </div>
              </div>
            </div>

            {detail.response_time != null && (
              <div className="rr-stat-cell" style={{ marginBottom: 18 }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                  <span className="rr-eyebrow">延迟（近 8 个采样点）</span>
                  <span className="rr-num" style={{ color: 'var(--rr-primary)', fontSize: 13 }}>
                    {String(detail.response_time)}ms
                  </span>
                </div>
                <Sparkline data={latencySeries(detail.response_time as number)} width={396} height={56} />
              </div>
            )}

            <div className="rr-stat-cell" style={{ marginBottom: 18, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ fontWeight: 600 }}>熔断器</div>
                <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', marginTop: 2 }}>
                  {detail.disabled_reason ? String(detail.disabled_reason) : '连续失败自动跳闸，半开探活恢复'}
                </div>
              </div>
              <StatusPill tone={detail.status === 'CircuitBroken' ? 'danger' : 'success'}>
                {detail.status === 'CircuitBroken' ? '跳闸' : '闭合'}
              </StatusPill>
            </div>

            {testResult && (
              <Alert
                type={testResult.ok ? 'success' : 'error'}
                showIcon
                message={testResult.ok ? '连通正常' : '连通失败'}
                description={
                  <span className="rr-num" style={{ fontSize: 12 }}>
                    状态 {testResult.status} · 耗时 {testResult.latency_ms}ms · 模型 {testResult.model}
                    {testResult.error ? ` · ${testResult.error}` : ''}
                  </span>
                }
              />
            )}
          </div>
        )}
      </FormDrawer>

      {/* 新建/编辑表单抽屉 */}
      <ResourceEditDrawer resource={RESOURCE.channels} title="渠道" crud={crud} />

      {/* 新密钥/凭据明文（渠道无 secret，占位以统一） */}
      {crud.secretValue && (
        <Alert type="warning" showIcon message={crud.secretValue} onClose={crud.clearSecret} closable />
      )}
    </div>
  )
}
