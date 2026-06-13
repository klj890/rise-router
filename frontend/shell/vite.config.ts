import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// 端口 5273：避开 5173（openrouter-china 占用）与 8080（微信冲突，全局禁用）。
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5273,
    proxy: {
      // 开发期把 /api、/healthz、/readyz 代理到后端（rise-server :8088）
      '/api': { target: 'http://localhost:8088', changeOrigin: true },
      '/healthz': { target: 'http://localhost:8088', changeOrigin: true },
      '/readyz': { target: 'http://localhost:8088', changeOrigin: true },
    },
  },
})
