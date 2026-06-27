import { useQuery } from '@tanstack/react-query'
import { SafetyCertificateOutlined } from '@ant-design/icons'
import { PageHeader, SectionCard, StatusPill } from '../../components/ui'
import { api } from '../../api/client'

interface RoleRow {
  slug?: string
  name?: string
  description?: string
  [k: string]: unknown
}

// 角色卡兜底（接口需 rbac.manage；无权限/未配管理令牌时展示）
const FALLBACK_ROLES: RoleRow[] = [
  { slug: 'super_admin', name: '超级管理员', description: '平台全部权限，含权限授予与系统配置。' },
  { slug: 'ops', name: '运维', description: '渠道、模型、路由与系统健康管理。' },
  { slug: 'finance', name: '财务', description: '营收、对账、成本毛利与发票管理。' },
  { slug: 'tenant_admin', name: '租户管理员', description: '本组织成员、密钥与用量管理。' },
]

// 成员表（mock：无 users 列表端点）
const MEMBERS = [
  { name: '林安平', role: '超级管理员', tone: 'primary' as const, group: '内部', active: '在线', online: true },
  { name: '周慕云', role: '财务', tone: 'success' as const, group: '内部', active: '12 分钟前', online: false },
  { name: '吴桐', role: '运维', tone: 'warning' as const, group: '内部', active: '在线', online: true },
  { name: '陈砚', role: '租户管理员', tone: 'purple' as const, group: 'Acme 智能科技', active: '1 小时前', online: false },
]

export default function Rbac() {
  const rolesQuery = useQuery({
    queryKey: ['identity-roles'],
    queryFn: async () => (await api.get<RoleRow[]>('/api/identity/roles')).data,
    retry: false,
  })
  const permsQuery = useQuery({
    queryKey: ['identity-permissions'],
    queryFn: async () => (await api.get<unknown[]>('/api/identity/permissions')).data,
    retry: false,
  })

  const roles = rolesQuery.data && rolesQuery.data.length > 0 ? rolesQuery.data : FALLBACK_ROLES
  const permCount = permsQuery.data?.length

  return (
    <div>
      <PageHeader
        title="用户 & 权限"
        subtitle="角色与成员管理 —— 权限点由应用注册时声明，复用核心 RBAC。"
      />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: 16, marginBottom: 16 }}>
        {roles.map((r, i) => (
          <div key={r.slug ?? i} className="rr-card" style={{ padding: 18 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
              <span
                style={{
                  width: 36,
                  height: 36,
                  borderRadius: 10,
                  background: 'var(--rr-primary-weak)',
                  color: 'var(--rr-primary)',
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: 17,
                }}
              >
                <SafetyCertificateOutlined />
              </span>
              <div>
                <div style={{ fontWeight: 700, color: 'var(--rr-text)' }}>{r.name ?? r.slug}</div>
                {r.slug && <div className="rr-num" style={{ fontSize: 11.5, color: 'var(--rr-text-3)' }}>{r.slug}</div>}
              </div>
            </div>
            <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', lineHeight: 1.6, minHeight: 38 }}>
              {r.description ?? '—'}
            </div>
          </div>
        ))}
      </div>

      <SectionCard
        title="成员"
        extra={
          permCount != null ? (
            <span style={{ fontSize: 12.5, color: 'var(--rr-text-3)' }}>系统已注册 {permCount} 个权限点</span>
          ) : null
        }
        flush
      >
        <table className="rr-table">
          <thead>
            <tr>
              <th style={{ textAlign: 'left' }}>成员</th>
              <th style={{ textAlign: 'left' }}>角色</th>
              <th style={{ textAlign: 'left' }}>归属</th>
              <th style={{ textAlign: 'right' }}>状态</th>
            </tr>
          </thead>
          <tbody>
            {MEMBERS.map((m) => (
              <tr key={m.name}>
                <td>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <span className="rr-avatar" style={{ width: 30, height: 30, fontSize: 13, borderRadius: 9 }}>
                      {m.name.charAt(0)}
                    </span>
                    <span style={{ fontWeight: 500 }}>{m.name}</span>
                  </div>
                </td>
                <td>
                  <StatusPill tone={m.tone}>{m.role}</StatusPill>
                </td>
                <td style={{ color: 'var(--rr-text-2)' }}>{m.group}</td>
                <td style={{ textAlign: 'right' }}>
                  <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, color: m.online ? 'var(--rr-success)' : 'var(--rr-text-3)' }}>
                    <span style={{ width: 6, height: 6, borderRadius: '50%', background: m.online ? 'var(--rr-success)' : 'var(--rr-text-3)' }} />
                    {m.active}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </SectionCard>
    </div>
  )
}
