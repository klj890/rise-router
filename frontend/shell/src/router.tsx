import type { ReactNode } from 'react'
import { createBrowserRouter, Navigate } from 'react-router-dom'
import LoginPage from './pages/Login'
import AppLayout from './layout/AppLayout'
import DashboardPage from './pages/Dashboard'
import AppearancePage from './pages/settings/Appearance'
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
      { path: 'settings/appearance', element: <AppearancePage /> },
    ],
  },
])
