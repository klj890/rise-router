import { Spin, Alert } from 'antd'
import { CheckCircleOutlined, SafetyCertificateOutlined } from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { PageHeader, SectionCard, StatusPill } from '../../components/ui'
import { api } from '../../api/client'

interface MeResp {
  user: { id: number; phone?: string | null; nickname?: string | null; status?: string }
  org: {
    id: number
    name: string
    org_type: string
    status: string
    realname_status: string
    created_at?: string
    group_id?: number | null
    owner_sales_id?: number | null
  }
}

const REALNAME_LABEL: Record<string, { text: string; tone: 'success' | 'warning' | 'neutral' }> = {
  Unverified: { text: '未认证', tone: 'warning' },
  IndividualVerified: { text: '个人已认证', tone: 'success' },
  EnterpriseVerified: { text: '企业已实名', tone: 'success' },
}

const ORG_TYPE_LABEL: Record<string, string> = { Individual: '个人', Enterprise: '企业' }

// 会话管理（mock：会话表接口就绪后替换）
const SESSIONS = [
  { device: 'MacBook Pro · Chrome', loc: '上海 · 中国电信', active: '当前', current: true },
  { device: 'iPhone 15 · Safari', loc: '上海 · 中国移动', active: '2 小时前', current: false },
  { device: 'Windows · Edge', loc: '北京 · 中国联通', active: '3 天前', current: false },
]

export default function OrgAuth() {
  const me = useQuery({
    queryKey: ['identity-me'],
    queryFn: async () => (await api.get<MeResp>('/api/identity/me')).data,
    retry: false,
  })

  const org = me.data?.org
  const user = me.data?.user
  const realname = REALNAME_LABEL[org?.realname_status ?? 'Unverified'] ?? REALNAME_LABEL.Unverified
  const verified = org?.realname_status !== 'Unverified'

  return (
    <div>
      <PageHeader
        title="组织与认证"
        subtitle="组织信息、实名认证、登录方式与会话管理 —— 身份基座统一治理。"
      />

      {me.isError && (
        <Alert type="error" showIcon style={{ marginBottom: 16 }} message="加载组织信息失败" description="请检查登录态是否有效。" />
      )}

      {me.isLoading ? (
        <div style={{ padding: 60, textAlign: 'center' }}>
          <Spin />
        </div>
      ) : (
        <>
          <div style={{ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 16, marginBottom: 16 }}>
            {/* 组织信息 */}
            <SectionCard>
              <div style={{ display: 'flex', alignItems: 'center', gap: 14, marginBottom: 18 }}>
                <span className="rr-avatar" style={{ width: 52, height: 52, fontSize: 22, borderRadius: 13 }}>
                  {(org?.name ?? 'R').charAt(0)}
                </span>
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{ fontSize: 18, fontWeight: 700, color: 'var(--rr-text)' }}>{org?.name ?? '—'}</span>
                    {verified && <StatusPill tone="success">已实名</StatusPill>}
                  </div>
                  <div className="rr-num" style={{ fontSize: 12.5, color: 'var(--rr-text-3)', marginTop: 2 }}>
                    组织 ID · #{org?.id ?? '—'} · 创建于 {org?.created_at ? dayjs(org.created_at).format('YYYY-MM-DD') : '—'}
                  </div>
                </div>
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14 }}>
                <Field label="组织类型" value={ORG_TYPE_LABEL[org?.org_type ?? ''] ?? org?.org_type ?? '—'} />
                <Field label="组织状态" value={org?.status === 'Active' ? '活跃' : org?.status ?? '—'} />
                <Field label="商业分组" value={org?.group_id != null ? `#${org.group_id}` : '默认价'} />
                <Field label="归属销售" value={org?.owner_sales_id != null ? `#${org.owner_sales_id}` : '自主注册'} />
              </div>
            </SectionCard>

            {/* 实名认证 */}
            <SectionCard style={{ borderColor: verified ? 'var(--rr-success)' : undefined }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12 }}>
                <span
                  style={{
                    width: 40,
                    height: 40,
                    borderRadius: 11,
                    background: 'var(--rr-success-weak)',
                    color: 'var(--rr-success)',
                    display: 'inline-flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: 19,
                  }}
                >
                  <SafetyCertificateOutlined />
                </span>
                <div>
                  <div style={{ fontWeight: 700, fontSize: 15, color: 'var(--rr-text)' }}>
                    实名认证{verified ? '已通过' : '待完成'}
                  </div>
                  <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)' }}>{realname.text}</div>
                </div>
              </div>
              <div style={{ fontSize: 13, color: 'var(--rr-text-2)', lineHeight: 1.7, marginBottom: 12 }}>
                {verified
                  ? '已完成主体与对公账户验证，可开具增值税专用发票、申请授信后付费与私有化部署。'
                  : '完成企业实名后可开通对公账户、专票开具与授信后付费。'}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                {['营业执照', '对公账户', '法人身份'].map((c) => (
                  <span key={c} className="rr-chip" style={{ color: verified ? 'var(--rr-success)' : 'var(--rr-text-3)' }}>
                    <CheckCircleOutlined /> {c}
                  </span>
                ))}
              </div>
            </SectionCard>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1.4fr', gap: 16 }}>
            {/* 登录与安全 */}
            <SectionCard title="登录与安全">
              <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                <Bind label="手机号" value={user?.phone ? maskPhone(user.phone) : '未绑定'} bound={!!user?.phone} />
                <Bind label="微信" value="未绑定" bound={false} />
                <Bind label="登录密码" value="未设置" bound={false} />
              </div>
            </SectionCard>

            {/* 会话管理 */}
            <SectionCard title="会话管理" extra={<a style={{ color: 'var(--rr-danger)' }}>注销其他会话</a>} flush>
              <table className="rr-table">
                <thead>
                  <tr>
                    <th style={{ textAlign: 'left' }}>设备</th>
                    <th style={{ textAlign: 'left' }}>登录地</th>
                    <th style={{ textAlign: 'right' }}>活跃</th>
                  </tr>
                </thead>
                <tbody>
                  {SESSIONS.map((s) => (
                    <tr key={s.device}>
                      <td style={{ fontWeight: 500 }}>{s.device}</td>
                      <td style={{ color: 'var(--rr-text-2)' }}>{s.loc}</td>
                      <td style={{ textAlign: 'right' }}>
                        {s.current ? <StatusPill tone="primary">当前</StatusPill> : <span style={{ color: 'var(--rr-text-3)' }}>{s.active}</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </SectionCard>
          </div>
        </>
      )}
    </div>
  )
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="rr-eyebrow">{label}</div>
      <div style={{ fontSize: 14, color: 'var(--rr-text)', marginTop: 3 }}>{value}</div>
    </div>
  )
}

function Bind({ label, value, bound }: { label: string; value: string; bound: boolean }) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '11px 14px',
        borderRadius: 10,
        border: '1px solid var(--rr-border)',
      }}
    >
      <div>
        <div style={{ fontSize: 13.5, fontWeight: 500, color: 'var(--rr-text)' }}>{label}</div>
        <div className="rr-num" style={{ fontSize: 12, color: 'var(--rr-text-3)', marginTop: 2 }}>{value}</div>
      </div>
      <StatusPill tone={bound ? 'success' : 'neutral'}>{bound ? '已绑定' : '去绑定'}</StatusPill>
    </div>
  )
}

function maskPhone(p: string): string {
  return p.length === 11 ? `${p.slice(0, 3)} **** ${p.slice(7)}` : p
}
