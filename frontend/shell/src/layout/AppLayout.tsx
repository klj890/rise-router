import { Layout, Menu, Dropdown, Avatar, Typography } from 'antd'
import {
  DashboardOutlined,
  DollarOutlined,
  ApiOutlined,
  AccountBookOutlined,
  TeamOutlined,
  BarChartOutlined,
  CustomerServiceOutlined,
  UserOutlined,
} from '@ant-design/icons'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useAuthStore } from '../store/auth'

const { Header, Sider, Content } = Layout

// 导航对应数据模型十大域；M0 仅 Dashboard 可达，其余为后续里程碑占位。
const menuItems = [
  { key: '/dashboard', icon: <DashboardOutlined />, label: '概览' },
  { key: '/gateway', icon: <ApiOutlined />, label: '网关与渠道' },
  { key: '/pricing', icon: <DollarOutlined />, label: '定价管理' },
  { key: '/billing', icon: <AccountBookOutlined />, label: '财务与计费' },
  { key: '/crm', icon: <TeamOutlined />, label: 'CRM 与销售' },
  { key: '/report', icon: <BarChartOutlined />, label: '监控报表' },
  { key: '/support', icon: <CustomerServiceOutlined />, label: '客服工单' },
]

export default function AppLayout() {
  const navigate = useNavigate()
  const location = useLocation()
  const { username, logout } = useAuthStore()

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider theme="light" breakpoint="lg" collapsible>
        <div
          style={{
            height: 48,
            margin: 16,
            color: '#1a3a6e',
            fontWeight: 700,
            fontSize: 18,
            display: 'flex',
            alignItems: 'center',
          }}
        >
          Rise Router
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={({ key }) => navigate(key)}
        />
      </Sider>
      <Layout>
        <Header
          style={{
            background: '#1a3a6e',
            display: 'flex',
            justifyContent: 'flex-end',
            alignItems: 'center',
            paddingInline: 24,
          }}
        >
          <Dropdown
            menu={{
              items: [{ key: 'logout', label: '退出登录', onClick: () => logout() }],
            }}
          >
            <span style={{ color: '#fff', cursor: 'pointer' }}>
              <Avatar size="small" icon={<UserOutlined />} style={{ marginRight: 8 }} />
              <Typography.Text style={{ color: '#fff' }}>{username ?? '未登录'}</Typography.Text>
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
