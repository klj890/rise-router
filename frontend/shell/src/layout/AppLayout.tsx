import { Layout, Menu, Dropdown, Avatar, Typography, theme } from 'antd'
import {
  DashboardOutlined,
  DollarOutlined,
  ApiOutlined,
  AccountBookOutlined,
  TeamOutlined,
  BarChartOutlined,
  CustomerServiceOutlined,
  UserOutlined,
  BgColorsOutlined,
} from '@ant-design/icons'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'
import { useResolvedMode } from '../theme/useResolvedMode'
import ThemeControls from '../components/ThemeControls'
import LocaleSwitcher from '../components/LocaleSwitcher'

const { Header, Sider, Content } = Layout

export default function AppLayout() {
  const navigate = useNavigate()
  const location = useLocation()
  const { t } = useTranslation()
  const resolved = useResolvedMode()
  const { token } = theme.useToken()
  const { username, logout } = useAuthStore()
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? t('common:brand')

  // 导航对应数据模型十大域；M0 仅「概览」「外观设置」可达，其余为后续里程碑占位。
  const menuItems = [
    { key: '/dashboard', icon: <DashboardOutlined />, label: t('common:nav.overview') },
    { key: '/gateway', icon: <ApiOutlined />, label: t('common:nav.gateway') },
    { key: '/pricing', icon: <DollarOutlined />, label: t('common:nav.pricing') },
    { key: '/billing', icon: <AccountBookOutlined />, label: t('common:nav.billing') },
    { key: '/crm', icon: <TeamOutlined />, label: t('common:nav.crm') },
    { key: '/report', icon: <BarChartOutlined />, label: t('common:nav.report') },
    { key: '/support', icon: <CustomerServiceOutlined />, label: t('common:nav.support') },
    { key: '/settings/appearance', icon: <BgColorsOutlined />, label: t('common:nav.appearance') },
  ]

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider
        theme={resolved}
        breakpoint="lg"
        collapsible
        style={{ borderRight: `1px solid ${token.colorBorderSecondary}` }}
      >
        <div
          style={{
            height: 56,
            paddingInline: 20,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            fontWeight: 600,
            fontSize: 16,
            letterSpacing: 0.2,
          }}
        >
          {brand.logoUrl ? (
            <img src={brand.logoUrl} alt="logo" style={{ height: 22 }} />
          ) : (
            <span
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                background: token.colorPrimary,
                display: 'inline-block',
              }}
            />
          )}
          <span style={{ color: token.colorText }}>{appName}</span>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={({ key }) => navigate(key)}
          style={{ borderInlineEnd: 'none', background: 'transparent' }}
        />
      </Sider>
      <Layout>
        <Header
          style={{
            background: token.colorBgContainer,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            display: 'flex',
            justifyContent: 'flex-end',
            alignItems: 'center',
            gap: 8,
            paddingInline: 20,
          }}
        >
          <LocaleSwitcher />
          <ThemeControls />
          <Dropdown
            menu={{
              items: [
                { key: 'logout', label: t('common:action.logout'), onClick: () => logout() },
              ],
            }}
          >
            <span style={{ cursor: 'pointer', display: 'inline-flex', alignItems: 'center', gap: 8 }}>
              <Avatar size="small" icon={<UserOutlined />} />
              <Typography.Text>{username ?? t('common:user.notLoggedIn')}</Typography.Text>
            </span>
          </Dropdown>
        </Header>
        <Content style={{ margin: 24 }}>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  )
}
