import type { ReactNode } from 'react'
import { createBrowserRouter, Navigate } from 'react-router-dom'
import LoginPage from './pages/Login'
import AppLayout from './layout/AppLayout'
import DashboardPage from './pages/Dashboard'
import AppearancePage from './pages/settings/Appearance'
import CrudPage from './pages/admin/CrudPage'
import { ADMIN_RESOURCES } from './pages/admin/resources'
import ChannelsPage from './pages/admin/ChannelsPage'
import ApiKeysPage from './pages/admin/ApiKeysPage'
import AdminTokenSettings from './pages/admin/AdminTokenSettings'
import CustomerList from './pages/crm/CustomerList'
import CustomerDetail from './pages/crm/CustomerDetail'
import ReportBuilder from './pages/report/ReportBuilder'
import PricingFive from './pages/pricing/PricingFive'
import Billing from './pages/billing/Billing'
import Tasks from './pages/tasks/Tasks'
import OrgAuth from './pages/org/OrgAuth'
import Rbac from './pages/rbac/Rbac'
import AppMarket from './pages/apps/AppMarket'
import Support from './pages/support/Support'
import { useAuthStore } from './store/auth'

function RequireAuth({ children }: { children: ReactNode }) {
  const token = useAuthStore((s) => s.token)
  if (!token) return <Navigate to="/login" replace />
  return <>{children}</>
}

// channels / api-keys 走 bespoke 页（详情抽屉、预算条），其余资源走通用 CrudPage。
const BESPOKE = new Set(['channels', 'api-keys'])
const GENERIC_ADMIN = ADMIN_RESOURCES.filter((r) => !BESPOKE.has(r.key))

export const router = createBrowserRouter([
  { path: '/login', element: <LoginPage /> },
  {
    path: '/',
    element: (
      <RequireAuth>
        <AppLayout />
      </RequireAuth>
    ),
    children: [
      { index: true, element: <Navigate to="/dashboard" replace /> },
      { path: 'dashboard', element: <DashboardPage /> },
      // 网关
      { path: 'admin/channels', element: <ChannelsPage /> },
      { path: 'admin/api-keys', element: <ApiKeysPage /> },
      { path: 'tasks', element: <Tasks /> },
      // 定价与计费
      { path: 'pricing', element: <PricingFive /> },
      { path: 'billing', element: <Billing /> },
      // 增长
      { path: 'crm', element: <CustomerList /> },
      { path: 'crm/:orgId', element: <CustomerDetail /> },
      { path: 'report', element: <ReportBuilder /> },
      // 平台
      { path: 'org', element: <OrgAuth /> },
      { path: 'rbac', element: <Rbac /> },
      { path: 'apps', element: <AppMarket /> },
      { path: 'support', element: <Support /> },
      { path: 'settings/appearance', element: <AppearancePage /> },
      { path: 'settings/admin-token', element: <AdminTokenSettings /> },
      // 管理台 CRUD（按资源描述符动态挂载）
      ...GENERIC_ADMIN.map((r) => ({
        path: `admin/${r.key}`,
        element: <CrudPage resource={r.def} title={r.title} />,
      })),
    ],
  },
])
