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
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'
import { useResolvedMode } from '../theme/useResolvedMode'
import ThemeControls from '../components/ThemeControls'

const { Header, Sider, Content } = Layout

// 导航对应数据模型十大域；M0 仅「概览」「外观设置」可达，其余为后续里程碑占位。
const menuItems = [
  { key: '/dashboard', icon: <DashboardOutlined />, label: '概览' },
  { key: '/gateway', icon: <ApiOutlined />, label: '网关与渠道' },
  { key: '/pricing', icon: <DollarOutlined />, label: '定价管理' },
  { key: '/billing', icon: <AccountBookOutlined />, label: '财务与计费' },
  { key: '/crm', icon: <TeamOutlined />, label: 'CRM 与销售' },
  { key: '/report', icon: <BarChartOutlined />, label: '监控报表' },
  { key: '/support', icon: <CustomerServiceOutlined />, label: '客服工单' },
  { key: '/settings/appearance', icon: <BgColorsOutlined />, label: '外观设置' },
]

export default function AppLayout() {
  const navigate = useNavigate()
  const location = useLocation()
  const resolved = useResolvedMode()
  const { token } = theme.useToken()
  const { username, logout } = useAuthStore()
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? 'Rise Router'

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
          <ThemeControls />
          <Dropdown
            menu={{ items: [{ key: 'logout', label: '退出登录', onClick: () => logout() }] }}
          >
            <span style={{ cursor: 'pointer', display: 'inline-flex', alignItems: 'center', gap: 8 }}>
              <Avatar size="small" icon={<UserOutlined />} />
              <Typography.Text>{username ?? '未登录'}</Typography.Text>
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
