import { useState } from 'react'
import { Button, Input, message } from 'antd'
import { SendOutlined } from '@ant-design/icons'
import { PageHeader, SectionCard, StatusPill, KpiCard } from '../../components/ui'
import type { PillTone } from '../../components/ui'

type TicketStatus = 'open' | 'pending' | 'resolved'
const STATUS: Record<TicketStatus, { label: string; tone: PillTone }> = {
  open: { label: '待响应', tone: 'danger' },
  pending: { label: '处理中', tone: 'warning' },
  resolved: { label: '已解决', tone: 'success' },
}

interface Msg {
  from: 'user' | 'agent'
  text: string
  time: string
}
interface Ticket {
  id: string
  subject: string
  org: string
  status: TicketStatus
  updated: string
  msgs: Msg[]
}

const TICKETS: Ticket[] = [
  {
    id: 'T-2041',
    subject: 'gpt-4o 渠道间歇 429',
    org: 'Acme 智能科技',
    status: 'open',
    updated: '10 分钟前',
    msgs: [
      { from: 'user', text: '我们的生产密钥从今早开始间歇性返回 429，能帮忙看下吗？', time: '09:12' },
      { from: 'agent', text: '您好，正在排查上游 OpenAI 官方渠道的限流，预计 10 分钟内回复。', time: '09:15' },
    ],
  },
  {
    id: 'T-2038',
    subject: '增值税专票抬头变更',
    org: '云帆数据',
    status: 'pending',
    updated: '1 小时前',
    msgs: [
      { from: 'user', text: '公司名称变更了，需要更新开票抬头。', time: '昨天 16:40' },
      { from: 'agent', text: '已收到，请在「组织与认证」上传新的营业执照，我们审核后更新。', time: '昨天 16:52' },
    ],
  },
  {
    id: 'T-2030',
    subject: '申请提升 QPS 限额',
    org: '星河科技',
    status: 'resolved',
    updated: '2 天前',
    msgs: [
      { from: 'user', text: '希望把 QPS 从 100 提到 500。', time: '06-25 10:00' },
      { from: 'agent', text: '已为贵司分组上调至 500 QPS，请验证。', time: '06-25 11:20' },
      { from: 'user', text: '已确认，感谢！', time: '06-25 11:35' },
    ],
  },
]

export default function Support() {
  const [activeId, setActiveId] = useState(TICKETS[0].id)
  const [reply, setReply] = useState('')
  const active = TICKETS.find((t) => t.id === activeId)!

  const counts = {
    open: TICKETS.filter((t) => t.status === 'open').length,
    pending: TICKETS.filter((t) => t.status === 'pending').length,
    resolved: TICKETS.filter((t) => t.status === 'resolved').length,
  }

  return (
    <div>
      <PageHeader title="客服工单" subtitle="完整的工单 / 会话客服能力 —— 状态流转与对话记录。" />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label="待响应" value={counts.open} accent />
        <KpiCard label="处理中" value={counts.pending} />
        <KpiCard label="已解决" value={counts.resolved} />
        <KpiCard label="平均响应" value="14" suffix="分钟" />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '340px 1fr', gap: 16 }}>
        {/* 工单列表 */}
        <SectionCard flush>
          {TICKETS.map((t) => {
            const on = t.id === activeId
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setActiveId(t.id)}
                style={{
                  display: 'block',
                  width: '100%',
                  textAlign: 'left',
                  padding: '14px 16px',
                  border: 'none',
                  borderLeft: `2px solid ${on ? 'var(--rr-primary)' : 'transparent'}`,
                  borderBottom: '1px solid var(--rr-border-secondary)',
                  background: on ? 'var(--rr-primary-weak)' : 'transparent',
                  cursor: 'pointer',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
                  <span className="rr-num" style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>#{t.id}</span>
                  <StatusPill tone={STATUS[t.status].tone} dot>{STATUS[t.status].label}</StatusPill>
                </div>
                <div style={{ fontWeight: 600, color: 'var(--rr-text)', margin: '4px 0 2px' }}>{t.subject}</div>
                <div style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>{t.org} · {t.updated}</div>
              </button>
            )
          })}
        </SectionCard>

        {/* 工单对话 */}
        <SectionCard
          title={
            <span>
              <span className="rr-num" style={{ color: 'var(--rr-text-3)', marginRight: 8 }}>#{active.id}</span>
              {active.subject}
            </span>
          }
          extra={<StatusPill tone={STATUS[active.status].tone} dot>{STATUS[active.status].label}</StatusPill>}
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14, minHeight: 280, marginBottom: 16 }}>
            {active.msgs.map((m, i) => (
              <div
                key={i}
                style={{ display: 'flex', justifyContent: m.from === 'agent' ? 'flex-end' : 'flex-start' }}
              >
                <div style={{ maxWidth: '72%' }}>
                  <div
                    style={{
                      padding: '10px 14px',
                      borderRadius: 12,
                      fontSize: 13.5,
                      lineHeight: 1.6,
                      background: m.from === 'agent' ? 'var(--rr-primary)' : 'var(--rr-surface-2)',
                      color: m.from === 'agent' ? '#fff' : 'var(--rr-text)',
                      border: m.from === 'agent' ? 'none' : '1px solid var(--rr-border)',
                    }}
                  >
                    {m.text}
                  </div>
                  <div style={{ fontSize: 11, color: 'var(--rr-text-3)', marginTop: 3, textAlign: m.from === 'agent' ? 'right' : 'left' }}>
                    {m.from === 'agent' ? '客服' : active.org} · {m.time}
                  </div>
                </div>
              </div>
            ))}
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <Input.TextArea
              rows={2}
              value={reply}
              onChange={(e) => setReply(e.target.value)}
              placeholder="输入回复…"
              style={{ flex: 1 }}
            />
            <Button
              type="primary"
              icon={<SendOutlined />}
              style={{ height: 'auto' }}
              disabled={!reply.trim()}
              onClick={() => {
                message.success('回复已发送（演示）')
                setReply('')
              }}
            >
              发送
            </Button>
          </div>
        </SectionCard>
      </div>
    </div>
  )
}
