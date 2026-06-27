import { useEffect, useMemo, useRef, useState } from 'react'
import { Layout, Dropdown, Input, Badge, Tooltip, Empty, type InputRef } from 'antd'
import {
  AppstoreOutlined,
  ApiOutlined,
  DeploymentUnitOutlined,
  NodeIndexOutlined,
  KeyOutlined,
  ThunderboltOutlined,
  DollarOutlined,
  AccountBookOutlined,
  TeamOutlined,
  BarChartOutlined,
  SafetyCertificateOutlined,
  UsergroupAddOutlined,
  ShopOutlined,
  CustomerServiceOutlined,
  BgColorsOutlined,
  ControlOutlined,
  SearchOutlined,
  BellOutlined,
  DownOutlined,
  LogoutOutlined,
} from '@ant-design/icons'
import type { ReactNode } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'
import ThemeControls from '../components/ThemeControls'
import LocaleSwitcher from '../components/LocaleSwitcher'

const { Sider, Content } = Layout

interface NavLeaf {
  key: string
  label: string
  icon: ReactNode
}
interface NavGroup {
  group: string
  items: NavLeaf[]
}

const NAV: NavGroup[] = [
  { group: '概览', items: [{ key: '/dashboard', label: '总览', icon: <AppstoreOutlined /> }] },
  {
    group: '网关',
    items: [
      { key: '/admin/channels', label: '渠道管理', icon: <ApiOutlined /> },
      { key: '/admin/models', label: '模型管理', icon: <DeploymentUnitOutlined /> },
      { key: '/admin/model-channels', label: '路由管理', icon: <NodeIndexOutlined /> },
      { key: '/admin/api-keys', label: 'API 密钥', icon: <KeyOutlined /> },
      { key: '/tasks', label: '多模态任务', icon: <ThunderboltOutlined /> },
    ],
  },
  {
    group: '定价与计费',
    items: [
      { key: '/pricing', label: '定价五要素', icon: <DollarOutlined /> },
      { key: '/billing', label: '计费与账单', icon: <AccountBookOutlined /> },
    ],
  },
  {
    group: '增长',
    items: [
      { key: '/crm', label: '客户与销售', icon: <TeamOutlined /> },
      { key: '/report', label: '监控报表', icon: <BarChartOutlined /> },
    ],
  },
  {
    group: '平台',
    items: [
      { key: '/org', label: '组织与认证', icon: <SafetyCertificateOutlined /> },
      { key: '/rbac', label: '用户 & 权限', icon: <UsergroupAddOutlined /> },
      { key: '/apps', label: 'App 插件市场', icon: <ShopOutlined /> },
      { key: '/support', label: '客服工单', icon: <CustomerServiceOutlined /> },
      { key: '/settings/appearance', label: '外观设置', icon: <BgColorsOutlined /> },
      { key: '/settings/admin-token', label: '管理令牌', icon: <ControlOutlined /> },
    ],
  },
]

const MOCK_NOTIFS = [
  { id: 1, title: '渠道「Google Vertex」错误率升至 2.3%', time: '5 分钟前', tone: 'var(--rr-warning)' },
  { id: 2, title: 'Acme 智能科技 完成对公转账充值 ¥50,000', time: '1 小时前', tone: 'var(--rr-success)' },
  { id: 3, title: '新工单 #T-2041 等待响应', time: '2 小时前', tone: 'var(--rr-primary)' },
]

export default function AppLayout() {
  const navigate = useNavigate()
  const location = useLocation()
  const { t } = useTranslation()
  const { username, logout } = useAuthStore()
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? t('common:brand')
  const [search, setSearch] = useState('')
  const searchRef = useRef<InputRef>(null)

  // ⌘K / Ctrl+K 聚焦顶栏搜索框（与 suffix 提示一致）。
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        searchRef.current?.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const isActive = (key: string) =>
    location.pathname === key || location.pathname.startsWith(key + '/')

  const initial = (username ?? 'U').trim().charAt(0).toUpperCase()

  const orgMenu = useMemo(
    () => ({
      items: [
        { key: 'acme', label: 'Acme 智能科技 · 生产环境' },
        { key: 'acme-staging', label: 'Acme 智能科技 · 预发环境' },
        { key: 'yunfan', label: '云帆数据 · 生产环境' },
      ],
    }),
    [],
  )

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider
        width={248}
        theme="light"
        style={{
          background: 'var(--rr-surface)',
          borderRight: '1px solid var(--rr-border)',
          height: '100vh',
          position: 'sticky',
          top: 0,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* logo 头 */}
        <div
          style={{
            height: 60,
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '0 20px',
            flexShrink: 0,
          }}
        >
          {brand.logoUrl ? (
            <img src={brand.logoUrl} alt="logo" style={{ height: 30 }} />
          ) : (
            <span className="rr-avatar" style={{ width: 34, height: 34, fontSize: 16, borderRadius: 9 }}>
              R
            </span>
          )}
          <div style={{ lineHeight: 1.15, minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 15.5, color: 'var(--rr-text)' }}>{appName}</div>
            <div className="rr-eyebrow" style={{ fontSize: 9.5 }}>
              CONTROL PLANE
            </div>
          </div>
        </div>

        {/* 滚动导航 */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '6px 12px 12px' }}>
          {NAV.map((g) => (
            <div key={g.group} style={{ marginTop: 14 }}>
              <div className="rr-eyebrow" style={{ padding: '0 10px 8px' }}>
                {g.group}
              </div>
              {g.items.map((it) => {
                const active = isActive(it.key)
                return (
                  <button
                    key={it.key}
                    type="button"
                    onClick={() => navigate(it.key)}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 11,
                      width: '100%',
                      height: 38,
                      padding: '0 10px',
                      marginBottom: 2,
                      border: 'none',
                      borderRadius: 9,
                      textAlign: 'left',
                      cursor: 'pointer',
                      fontSize: 13.5,
                      fontWeight: active ? 600 : 500,
                      color: active ? 'var(--rr-primary)' : 'var(--rr-text-2)',
                      background: active ? 'var(--rr-primary-weak)' : 'transparent',
                      transition: 'background .12s ease, color .12s ease',
                    }}
                    onMouseEnter={(e) => {
                      if (!active) e.currentTarget.style.background = 'var(--rr-fill)'
                    }}
                    onMouseLeave={(e) => {
                      if (!active) e.currentTarget.style.background = 'transparent'
                    }}
                  >
                    <span style={{ fontSize: 16, display: 'inline-flex' }}>{it.icon}</span>
                    {it.label}
                  </button>
                )
              })}
            </div>
          ))}
        </div>

        {/* 底部用户行 */}
        <div
          style={{
            flexShrink: 0,
            borderTop: '1px solid var(--rr-border)',
            padding: '10px 14px',
            display: 'flex',
            alignItems: 'center',
            gap: 10,
          }}
        >
          <span className="rr-avatar" style={{ width: 34, height: 34, fontSize: 14 }}>
            {initial}
          </span>
          <div style={{ flex: 1, minWidth: 0, lineHeight: 1.25 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: 'var(--rr-text)',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
            >
              {username ?? t('common:user.notLoggedIn')}
            </div>
            <div style={{ fontSize: 11.5, color: 'var(--rr-text-3)' }}>平台超级管理员</div>
          </div>
          <Tooltip title={t('common:action.logout')}>
            <button
              type="button"
              onClick={() => logout()}
              style={{
                border: 'none',
                background: 'transparent',
                color: 'var(--rr-text-3)',
                cursor: 'pointer',
                fontSize: 16,
                display: 'inline-flex',
                padding: 6,
              }}
            >
              <LogoutOutlined />
            </button>
          </Tooltip>
        </div>
      </Sider>

      <Layout style={{ background: 'var(--rr-bg-layout)' }}>
        {/* 顶栏 */}
        <div
          style={{
            height: 60,
            flexShrink: 0,
            background: 'var(--rr-surface)',
            borderBottom: '1px solid var(--rr-border)',
            display: 'flex',
            alignItems: 'center',
            gap: 14,
            padding: '0 22px',
            position: 'sticky',
            top: 0,
            zIndex: 10,
          }}
        >
          {/* 组织/环境切换 */}
          <Dropdown menu={orgMenu} trigger={['click']}>
            <button
              type="button"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                height: 40,
                padding: '0 12px',
                border: '1px solid var(--rr-border)',
                borderRadius: 10,
                background: 'transparent',
                cursor: 'pointer',
              }}
            >
              <span className="rr-avatar" style={{ width: 26, height: 26, fontSize: 12, borderRadius: 7 }}>
                A
              </span>
              <span style={{ lineHeight: 1.15, textAlign: 'left' }}>
                <span style={{ display: 'block', fontSize: 13, fontWeight: 600, color: 'var(--rr-text)' }}>
                  Acme 智能科技
                </span>
                <span style={{ display: 'block', fontSize: 11, color: 'var(--rr-text-3)' }}>生产环境</span>
              </span>
              <DownOutlined style={{ fontSize: 10, color: 'var(--rr-text-3)' }} />
            </button>
          </Dropdown>

          {/* ⌘K 搜索 */}
          <Input
            ref={searchRef}
            prefix={<SearchOutlined style={{ color: 'var(--rr-text-3)' }} />}
            suffix={<span className="rr-num rr-chip" style={{ fontSize: 11 }}>⌘K</span>}
            placeholder="搜索模型、渠道、租户…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            allowClear
            style={{ maxWidth: 360, flex: 1 }}
          />

          <div style={{ flex: 1 }} />

          <LocaleSwitcher />
          <ThemeControls />
          <Dropdown
            trigger={['click']}
            popupRender={() => (
              <div
                className="rr-card"
                style={{ width: 320, padding: 0, boxShadow: 'var(--rr-shadow-lg)' }}
              >
                <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--rr-border)', fontWeight: 600 }}>
                  通知中心
                </div>
                {MOCK_NOTIFS.length === 0 ? (
                  <Empty style={{ padding: 24 }} image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无通知" />
                ) : (
                  MOCK_NOTIFS.map((n) => (
                    <div
                      key={n.id}
                      style={{ display: 'flex', gap: 10, padding: '12px 16px', borderBottom: '1px solid var(--rr-border-secondary)' }}
                    >
                      <span style={{ width: 7, height: 7, borderRadius: '50%', background: n.tone, marginTop: 6, flexShrink: 0 }} />
                      <div>
                        <div style={{ fontSize: 13, color: 'var(--rr-text)' }}>{n.title}</div>
                        <div style={{ fontSize: 11.5, color: 'var(--rr-text-3)', marginTop: 2 }}>{n.time}</div>
                      </div>
                    </div>
                  ))
                )}
              </div>
            )}
          >
            <button
              type="button"
              style={{ border: 'none', background: 'transparent', cursor: 'pointer', padding: 6, color: 'var(--rr-text-2)' }}
            >
              <Badge dot offset={[-2, 2]}>
                <BellOutlined style={{ fontSize: 17 }} />
              </Badge>
            </button>
          </Dropdown>
        </div>

        <Content style={{ padding: '26px 30px 44px', overflow: 'auto' }}>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  )
}
