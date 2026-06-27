import { useMemo, useState } from 'react'
import { Button, Empty, Spin, Alert, Tooltip, message } from 'antd'
import { ReloadOutlined, InfoCircleOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import dayjs from 'dayjs'
import {
  PageHeader,
  SectionCard,
  StatusPill,
  FilterTabs,
  KpiCard,
  FormDrawer,
} from '../../components/ui'
import type { PillTone } from '../../components/ui'
import { listTasks, cancelTask, type Task, type TaskStatus } from '../../api/tasks'

const STATUS: Record<TaskStatus, { label: string; tone: PillTone }> = {
  queued: { label: '排队中', tone: 'warning' },
  running: { label: '运行中', tone: 'primary' },
  succeeded: { label: '成功', tone: 'success' },
  failed: { label: '失败', tone: 'danger' },
  cancelled: { label: '已取消', tone: 'neutral' },
}
const MODALITY_LABEL: Record<string, string> = { video: '视频', image: '图像', audio: '语音', text: '文本' }

/** type 形如 video.generation → 取模态前缀。 */
const modalityOf = (type: string) => type.split('.')[0]

/** 计费量纲数量 → 友好串：{second:7}→7 秒 / {image:4}→4 张 / {call:1}→1 次。 */
function fmtUsage(usage: Record<string, number> | null): string {
  if (!usage) return '—'
  const unit: Record<string, string> = { second: '秒', image: '张', call: '次', token: 'tok' }
  return (
    Object.entries(usage)
      .map(([k, v]) => `${v} ${unit[k] ?? k}`)
      .join(' · ') || '—'
  )
}

function fmtElapsed(t: Task): string {
  const start = t.started_at ?? t.created_at
  const end = t.finished_at
  if (!end) return t.status === 'running' ? '进行中' : '—'
  const s = Math.max(0, dayjs(end).diff(dayjs(start), 'second'))
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
}

const yuan = (v: string | null) => {
  const n = Number(v)
  return v == null || Number.isNaN(n) ? '—' : `¥${n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 4 })}`
}

export default function Tasks() {
  const qc = useQueryClient()
  const [typeFilter, setTypeFilter] = useState('all')
  const [detail, setDetail] = useState<Task | null>(null)

  const tasksQuery = useQuery({
    queryKey: ['admin-tasks'],
    queryFn: () => listTasks(200),
    refetchInterval: 5000, // 实时监控
    retry: false,
  })
  const denied = (() => {
    const code = (tasksQuery.error as { response?: { status?: number } } | null)?.response?.status
    return code === 401 || code === 403
  })()
  const rows = tasksQuery.data ?? []

  const cancelMutation = useMutation({
    mutationFn: (id: number) => cancelTask(id),
    onSuccess: () => {
      message.success('已取消')
      qc.invalidateQueries({ queryKey: ['admin-tasks'] })
    },
    onError: (e) => message.error((e as { localizedMessage?: string }).localizedMessage ?? '取消失败'),
  })

  const counts = useMemo(() => {
    const c: Record<string, number> = { queued: 0, running: 0, succeeded: 0, failed: 0, cancelled: 0 }
    rows.forEach((t) => (c[t.status] = (c[t.status] ?? 0) + 1))
    return c
  }, [rows])

  const filtered = typeFilter === 'all' ? rows : rows.filter((t) => modalityOf(t.type) === typeFilter)

  return (
    <div>
      <PageHeader
        title="多模态任务"
        subtitle="统一 /v1/tasks 提交、查询与取消 —— 文本 / 图像 / 语音 / 视频共用状态机，产物落对象存储，按量纲计费。"
        extra={
          <Button icon={<ReloadOutlined />} onClick={() => qc.invalidateQueries({ queryKey: ['admin-tasks'] })}>
            刷新
          </Button>
        }
      />

      <Alert
        type="info"
        showIcon
        icon={<InfoCircleOutlined />}
        style={{ marginBottom: 16 }}
        message="控制台为任务监控视图（每 5 秒自动刷新）。任务由 API 消费方用其密钥经 POST /v1/tasks 提交。"
      />

      {denied && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message="未设置管理令牌"
          description="任务监控需要管理令牌（X-Admin-Token）。请到「系统设置 · 管理令牌」填入后端 RR_ADMIN_TOKEN。"
        />
      )}

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label="运行中" value={counts.running} accent />
        <KpiCard label="排队中" value={counts.queued} />
        <KpiCard label="成功" value={counts.succeeded} hintTone="positive" />
        <KpiCard label="失败" value={counts.failed} hint={counts.cancelled ? `已取消 ${counts.cancelled}` : undefined} hintTone="muted" />
      </div>

      <SectionCard flush>
        <div style={{ padding: '14px 18px', borderBottom: '1px solid var(--rr-border)' }}>
          <FilterTabs
            items={[
              { key: 'all', label: '全部', count: rows.length },
              { key: 'text', label: '文本' },
              { key: 'image', label: '图像' },
              { key: 'audio', label: '语音' },
              { key: 'video', label: '视频' },
            ]}
            value={typeFilter}
            onChange={setTypeFilter}
          />
        </div>

        {tasksQuery.isLoading ? (
          <div style={{ padding: 48, textAlign: 'center' }}>
            <Spin />
          </div>
        ) : filtered.length === 0 ? (
          <Empty style={{ padding: 48 }} description={denied ? '无权限' : '暂无任务'} />
        ) : (
          <table className="rr-table">
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>任务 ID</th>
                <th style={{ textAlign: 'left' }}>类型</th>
                <th style={{ textAlign: 'left' }}>模型</th>
                <th style={{ textAlign: 'left' }}>租户</th>
                <th>状态</th>
                <th style={{ textAlign: 'right' }}>耗时</th>
                <th style={{ textAlign: 'right' }}>用量</th>
                <th style={{ textAlign: 'right' }}>计费</th>
                <th style={{ textAlign: 'right' }}>操作</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((t) => (
                <tr key={t.id} className="rr-row" onClick={() => setDetail(t)}>
                  <td className="rr-num" style={{ fontWeight: 600 }}>#{t.id}</td>
                  <td><span className="rr-chip">{MODALITY_LABEL[modalityOf(t.type)] ?? t.type}</span></td>
                  <td className="rr-num" style={{ color: 'var(--rr-text-2)' }}>{t.model_slug}</td>
                  <td style={{ color: 'var(--rr-text-2)' }}>{t.org_name}</td>
                  <td><StatusPill tone={STATUS[t.status]?.tone ?? 'neutral'} dot>{STATUS[t.status]?.label ?? t.status}</StatusPill></td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{fmtElapsed(t)}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{fmtUsage(t.usage)}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{yuan(t.charged_amount)}</td>
                  <td style={{ textAlign: 'right' }} onClick={(e) => e.stopPropagation()}>
                    {(t.status === 'queued' || t.status === 'running') ? (
                      <Button
                        size="small"
                        danger
                        loading={cancelMutation.isPending && cancelMutation.variables === t.id}
                        onClick={() => cancelMutation.mutate(t.id)}
                      >
                        取消
                      </Button>
                    ) : (
                      <Tooltip title="查看详情"><Button size="small" type="text" onClick={() => setDetail(t)}>详情</Button></Tooltip>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </SectionCard>

      {/* 任务详情抽屉 */}
      <FormDrawer
        open={detail != null}
        width={460}
        title={detail ? `任务 #${detail.id}` : ''}
        subtitle={detail?.type}
        onClose={() => setDetail(null)}
        showFooter={false}
      >
        {detail && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
              <Field label="状态">
                <StatusPill tone={STATUS[detail.status]?.tone ?? 'neutral'} dot>{STATUS[detail.status]?.label ?? detail.status}</StatusPill>
              </Field>
              <Field label="租户">{detail.org_name}</Field>
              <Field label="模型">{detail.model_slug}</Field>
              <Field label="耗时">{fmtElapsed(detail)}</Field>
              <Field label="用量">{fmtUsage(detail.usage)}</Field>
              <Field label="计费">{yuan(detail.charged_amount)}</Field>
            </div>
            <Field label="上游任务 ID">{detail.vendor_task_id ?? '—'}</Field>
            <Field label="创建 / 完成">
              {dayjs(detail.created_at).format('YYYY-MM-DD HH:mm:ss')}
              {detail.finished_at ? ` → ${dayjs(detail.finished_at).format('HH:mm:ss')}` : ''}
            </Field>
            {detail.webhook_url && (
              <Field label="Webhook">
                {detail.webhook_state ? <StatusPill tone={detail.webhook_state === 'delivered' ? 'success' : detail.webhook_state === 'blocked' ? 'danger' : 'warning'}>{detail.webhook_state}</StatusPill> : '—'}
              </Field>
            )}
            {detail.error && <Alert type="error" showIcon message={detail.error} />}
            <div>
              <div className="rr-eyebrow" style={{ marginBottom: 4 }}>输入</div>
              <pre className="rr-stat-cell rr-num" style={{ fontSize: 12, margin: 0, overflow: 'auto', maxHeight: 160 }}>
                {JSON.stringify(detail.input, null, 2)}
              </pre>
            </div>
          </div>
        )}
      </FormDrawer>
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="rr-eyebrow">{label}</div>
      <div style={{ fontSize: 13.5, color: 'var(--rr-text)', marginTop: 3 }}>{children}</div>
    </div>
  )
}
