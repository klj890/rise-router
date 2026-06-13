import React from 'react'
import ReactDOM from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from 'react-router-dom'
// 自托管字体（不依赖 Google Fonts CDN）
import '@fontsource/inter/400.css'
import '@fontsource/inter/500.css'
import '@fontsource/inter/600.css'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/500.css'
import './styles/tokens.css'
import './styles/global.css'
import './i18n' // 初始化 i18next（须在渲染前）
import { ThemeProvider } from './theme/ThemeProvider'
import { LocaleProvider } from './i18n/LocaleProvider'
import { loadBranding } from './theme/branding'
import { router } from './router'

const queryClient = new QueryClient()

// 异步拉取 per-租户白标，不阻塞首屏（后端未就绪时静默回落）
void loadBranding()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ThemeProvider>
      <LocaleProvider>
        <QueryClientProvider client={queryClient}>
          <RouterProvider router={router} />
        </QueryClientProvider>
      </LocaleProvider>
    </ThemeProvider>
  </React.StrictMode>,
)
