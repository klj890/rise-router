import { useMemo, useState } from 'react'
import { Button, Space, Alert, Empty, Spin, Modal, Typography } from 'antd'
import { PlusOutlined, ReloadOutlined, DollarOutlined, SafetyOutlined, ClockCircleOutlined, CopyOutlined, CheckOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'
import { PageHeader, SectionCard, StatusPill } from '../../components/ui'
import { useResourceCrud, ResourceEditDrawer } from './resourceShared'
import { statusTone } from './CrudPage'
import { RESOURCE } from './resources'
import { useAuthStore } from '../../store/auth'

const KEY_STATUS_LABEL: Record<string, string> = { Enabled: '启用', Disabled: '禁用' }

const FEATURES = [
  { icon: <DollarOutlined />, tone: 'var(--rr-primary)', title: '预算上限', desc: '按 token 或金额封顶，超限自动熔断。' },
  { icon: <SafetyOutlined />, tone: 'var(--rr-success)', title: '模型白名单', desc: '限定密钥可调用的模型集合。' },
  { icon: <ClockCircleOutlined />, tone: 'var(--rr-warning)', title: '过期与轮换', desc: '到期自动失效，支持一键轮换。' },
]

function maskToken(t: unknown): string {
  const s = String(t ?? '')
  if (!s) return 'sk-••••••••'
  if (s.length <= 12) return s
  return `${s.slice(0, 7)}••••••${s.slice(-4)}`
}

export default function ApiKeysPage() {
  const adminToken = useAuthStore((s) => s.adminToken)
  const crud = useResourceCrud(RESOURCE.apiKeys)
  const [copiedId, setCopiedId] = useState<number | null>(null)

  const orgMap = useMemo(() => {
    const m = new Map<string | number, string>()
    for (const o of crud.optionsByField['org_id'] ?? []) m.set(o.value, o.label)
    return m
  }, [crud.optionsByField])

  const copy = (id: number, value: string) => {
    navigator.clipboard?.writeText(value)
    setCopiedId(id)
    window.setTimeout(() => setCopiedId((c) => (c === id ? null : c)), 1600)
  }

  if (!adminToken) {
    return (
      <Alert
        type="warning"
        showIcon
        message="未设置管理令牌"
        description="API 密钥管理需要管理令牌（X-Admin-Token）。请到「系统设置 · 管理令牌」填入后端 RR_ADMIN_TOKEN。"
      />
    )
  }

  const rows = crud.rows

  return (
    <div>
      <PageHeader
        title="API 密钥"
        subtitle="为每个应用签发独立的虚拟密钥 —— 绑定预算上限、模型白名单与过期时间，超限自动熔断。"
        extra={
          <Space>
            <Button icon={<ReloadOutlined />} onClick={crud.refresh}>
              刷新
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={crud.openCreate}>
              创建密钥
            </Button>
          </Space>
        }
      />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16, marginBottom: 16 }}>
        {FEATURES.map((f) => (
          <div key={f.title} className="rr-card" style={{ padding: 18, display: 'flex', gap: 12 }}>
            <span
              style={{
                width: 38,
                height: 38,
                borderRadius: 10,
                background: 'var(--rr-primary-weak)',
                color: f.tone,
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: 18,
                flexShrink: 0,
              }}
            >
              {f.icon}
            </span>
            <div>
              <div style={{ fontWeight: 600, color: 'var(--rr-text)' }}>{f.title}</div>
              <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', marginTop: 3, lineHeight: 1.6 }}>{f.desc}</div>
            </div>
          </div>
        ))}
      </div>

      {crud.listQuery.isError && (
        <Alert type="error" showIcon style={{ marginBottom: 16 }} message="加载失败" description="请检查管理令牌与后端就绪状态。" />
      )}

      <SectionCard flush>
        {crud.listQuery.isLoading ? (
          <div style={{ padding: 48, textAlign: 'center' }}>
            <Spin />
          </div>
        ) : rows.length === 0 ? (
          <Empty style={{ padding: 48 }} description="暂无密钥" />
        ) : (
          <table className="rr-table">
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>密钥</th>
                <th style={{ textAlign: 'left' }}>组织</th>
                <th style={{ textAlign: 'left' }}>预算</th>
                <th style={{ textAlign: 'left' }}>模型白名单</th>
                <th>过期</th>
                <th style={{ textAlign: 'right' }}>状态</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => {
                const id = r.id as number
                const token = maskToken(r.api_key ?? r.key_prefix)
                const limit = Number(r.budget_limit) || 0
                const used = Number(r.budget_used) || 0
                const pct = limit > 0 ? Math.min(1, used / limit) : 0
                const barColor = pct >= 1 ? 'var(--rr-danger)' : pct >= 0.8 ? 'var(--rr-warning)' : 'var(--rr-primary)'
                const allowed = r.allowed_models as string[] | null | undefined
                return (
                  <tr key={id} className="rr-row" onClick={() => crud.openEdit(r)}>
                    <td>
                      <div style={{ fontWeight: 600, color: 'var(--rr-text)' }}>{String(r.name)}</div>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 2 }}>
                        <span className="rr-num" style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>{token}</span>
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation()
                            copy(id, String(r.api_key ?? r.key_prefix ?? ''))
                          }}
                          style={{ border: 'none', background: 'transparent', cursor: 'pointer', color: copiedId === id ? 'var(--rr-success)' : 'var(--rr-text-3)', fontSize: 12, display: 'inline-flex', alignItems: 'center', gap: 3 }}
                        >
                          {copiedId === id ? <><CheckOutlined /> 已复制</> : <CopyOutlined />}
                        </button>
                      </div>
                    </td>
                    <td style={{ color: 'var(--rr-text-2)' }}>{orgMap.get(r.org_id as number) ?? `#${String(r.org_id)}`}</td>
                    <td>
                      {limit > 0 ? (
                        <div style={{ minWidth: 130 }}>
                          <div className="rr-num" style={{ fontSize: 12.5, marginBottom: 4 }}>
                            {used.toLocaleString()} / {limit.toLocaleString()}
                          </div>
                          <div style={{ height: 5, borderRadius: 3, background: 'var(--rr-surface-2)', overflow: 'hidden' }}>
                            <div style={{ width: `${pct * 100}%`, height: '100%', background: barColor }} />
                          </div>
                        </div>
                      ) : (
                        <span style={{ color: 'var(--rr-text-3)' }}>不限</span>
                      )}
                    </td>
                    <td style={{ color: 'var(--rr-text-2)' }}>
                      {!allowed || allowed.length === 0 ? '全部模型' : allowed.length <= 2 ? allowed.join('、') : `${allowed.slice(0, 2).join('、')} +${allowed.length - 2}`}
                    </td>
                    <td className="rr-num" style={{ textAlign: 'center', color: 'var(--rr-text-2)' }}>
                      {r.expires_at ? dayjs(r.expires_at as string).format('YYYY-MM-DD') : '长期'}
                    </td>
                    <td style={{ textAlign: 'right' }}>
                      <StatusPill tone={statusTone(r.status)} dot>
                        {KEY_STATUS_LABEL[String(r.status)] ?? String(r.status)}
                      </StatusPill>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </SectionCard>

      <ResourceEditDrawer resource={RESOURCE.apiKeys} title="密钥" crud={crud} />

      {/* 新密钥明文（仅此一次） */}
      <Modal
        title="新密钥（明文仅此一次）"
        open={crud.secretValue != null}
        onOk={crud.clearSecret}
        onCancel={crud.clearSecret}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 12 }}
          message="请立即复制保存：明文仅此一次展示，关闭后无法再次获取。"
        />
        <Typography.Paragraph copyable code style={{ wordBreak: 'break-all' }}>
          {crud.secretValue}
        </Typography.Paragraph>
      </Modal>
    </div>
  )
}
