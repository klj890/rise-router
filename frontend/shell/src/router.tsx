import type { ReactNode } from 'react'
import { createBrowserRouter, Navigate } from 'react-router-dom'
import LoginPage from './pages/Login'
import AppLayout from './layout/AppLayout'
import DashboardPage from './pages/Dashboard'
import AppearancePage from './pages/settings/Appearance'
import CrudPage from './pages/admin/CrudPage'
import { ADMIN_RESOURCES } from './pages/admin/resources'
import PricePreviewPage from './pages/admin/PricePreview'
import AdminTokenSettings from './pages/admin/AdminTokenSettings'
import CustomerList from './pages/crm/CustomerList'
import CustomerDetail from './pages/crm/CustomerDetail'
import { useAuthStore } from './store/auth'

function RequireAuth({ children }: { children: ReactNode }) {
  const token = useAuthStore((s) => s.token)
  if (!token) return <Navigate to="/login" replace />
  return <>{children}</>
}

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
      // CRM 与销售控制台（M3 片D）
      { path: 'crm', element: <CustomerList /> },
      { path: 'crm/:orgId', element: <CustomerDetail /> },
      { path: 'settings/appearance', element: <AppearancePage /> },
      { path: 'settings/admin-token', element: <AdminTokenSettings /> },
      // 管理台 CRUD（按资源描述符动态挂载）+ 价格预览
      ...ADMIN_RESOURCES.map((r) => ({
        path: `admin/${r.key}`,
        element: <CrudPage resource={r.def} title={r.title} />,
      })),
      { path: 'admin/price-preview', element: <PricePreviewPage /> },
    ],
  },
])
