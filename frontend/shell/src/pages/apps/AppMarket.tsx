import { useState } from 'react'
import { Button, message } from 'antd'
import { AppstoreOutlined } from '@ant-design/icons'
import { PageHeader, FilterTabs, StatusPill } from '../../components/ui'

interface AppItem {
  name: string
  desc: string
  mount: string
  category: string
  installed: boolean
  internal: boolean
  icon: string
}

const APPS: AppItem[] = [
  { name: 'CRM 销售系统', desc: '客户档案、销售归属、代客开户与业绩统计。', mount: '/api/crm', category: '内部', installed: true, internal: true, icon: '🤝' },
  { name: '财务对账', desc: '充值、流水、发票与对账，企业财务一体化。', mount: '/api/billing', category: '内部', installed: true, internal: true, icon: '💰' },
  { name: '监控报表', desc: '策展数据集 + 行级隔离的定制报表引擎。', mount: '/api/report', category: '内部', installed: true, internal: true, icon: '📊' },
  { name: '客服工单', desc: '工单与会话客服，状态流转与对话记录。', mount: '/api/support', category: '内部', installed: true, internal: true, icon: '🎧' },
  { name: 'AI 视频工坊', desc: '第三方视频生成应用，按任务量纲计费。', mount: '/api/apps/video-studio', category: '第三方', installed: false, internal: false, icon: '🎬' },
  { name: 'AI 教育助手', desc: '面向 K12 的智能题库与讲解，多模态接入。', mount: '/api/apps/edu', category: '第三方', installed: false, internal: false, icon: '🎓' },
  { name: '智能客服机器人', desc: '基于平台模型的对话机器人，可嵌入官网。', mount: '/api/apps/chatbot', category: '第三方', installed: true, internal: false, icon: '🤖' },
  { name: '数据标注平台', desc: '人机协同标注，回流微调数据集。', mount: '/api/apps/labeling', category: '第三方', installed: false, internal: false, icon: '🏷️' },
]

export default function AppMarket() {
  const [cat, setCat] = useState('all')
  const filtered = cat === 'all' ? APPS : APPS.filter((a) => a.category === cat)

  return (
    <div>
      <PageHeader
        title="App 插件市场"
        subtitle="内部一等模块与第三方 App 同一套注册标准接入（OIDC + Manifest + 网关路由）。"
      />

      {/* 精选 banner */}
      <div
        className="rr-card"
        style={{
          padding: 28,
          marginBottom: 16,
          color: '#fff',
          background: 'linear-gradient(135deg, #4f46e5, #7c4ddb)',
          border: 'none',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 20,
        }}
      >
        <div>
          <div style={{ fontSize: 12, letterSpacing: '.08em', opacity: 0.85, textTransform: 'uppercase' }}>精选 App</div>
          <div style={{ fontSize: 22, fontWeight: 700, margin: '6px 0' }}>开发者平台 · 一次接入，三端打通</div>
          <div style={{ fontSize: 13.5, opacity: 0.9, maxWidth: 560, lineHeight: 1.7 }}>
            第三方 App 既是身份/权限接入方（App Manifest），又是 AI 能力消费方（per-app API Key + 配额 + 用量看板 + webhook），账单挂 App 与客户档案。
          </div>
        </div>
        <Button size="large" ghost onClick={() => message.info('开发者文档即将开放')}>
          接入指南
        </Button>
      </div>

      <div style={{ marginBottom: 16 }}>
        <FilterTabs
          items={[
            { key: 'all', label: '全部', count: APPS.length },
            { key: '内部', label: '内部模块', count: APPS.filter((a) => a.internal).length },
            { key: '第三方', label: '第三方', count: APPS.filter((a) => !a.internal).length },
          ]}
          value={cat}
          onChange={setCat}
        />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 16 }}>
        {filtered.map((a) => (
          <div key={a.name} className="rr-card rr-card-hover" style={{ padding: 18 }}>
            <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 10 }}>
              <span
                style={{
                  width: 44,
                  height: 44,
                  borderRadius: 12,
                  background: 'var(--rr-surface-2)',
                  border: '1px solid var(--rr-border)',
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: 22,
                }}
              >
                {a.icon}
              </span>
              <StatusPill tone={a.internal ? 'primary' : 'purple'}>{a.internal ? '内部模块' : '第三方'}</StatusPill>
            </div>
            <div style={{ fontWeight: 700, fontSize: 15, color: 'var(--rr-text)', marginTop: 12 }}>{a.name}</div>
            <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', marginTop: 4, lineHeight: 1.6, minHeight: 40 }}>{a.desc}</div>
            <div className="rr-num rr-chip" style={{ marginTop: 10, fontSize: 11.5 }}>
              <AppstoreOutlined /> {a.mount}
            </div>
            <div style={{ marginTop: 14 }}>
              {a.installed ? (
                <Button block disabled>
                  已安装
                </Button>
              ) : (
                <Button block type="primary" onClick={() => message.success(`已安装 ${a.name}（演示）`)}>
                  安装
                </Button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
