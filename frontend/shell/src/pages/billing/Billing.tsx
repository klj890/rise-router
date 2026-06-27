import { useState } from 'react'
import { Button, Tabs, Table, Modal, Form, InputNumber, Radio, Input, Alert, Spin, message } from 'antd'
import { PlusOutlined } from '@ant-design/icons'
import { useMutation, useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import { PageHeader, KpiCard, SectionCard, StatusPill } from '../../components/ui'
import type { PillTone } from '../../components/ui'
import {
  recharge,
  getMargin,
  listTenants,
  listOrders,
  listInvoices,
  listReconciliations,
} from '../../api/billing'
import { useAuthStore } from '../../store/auth'

// —— 状态词表 → 药丸语义（覆盖订单/发票/对账的 serde 变体名）——
function statusMeta(status: string): { label: string; tone: PillTone } {
  const s = status.toLowerCase()
  const map: Record<string, { label: string; tone: PillTone }> = {
    paid: { label: '已入账', tone: 'success' },
    pending: { label: '待支付', tone: 'warning' },
    failed: { label: '失败', tone: 'danger' },
    cancelled: { label: '已取消', tone: 'neutral' },
    canceled: { label: '已取消', tone: 'neutral' },
    issued: { label: '已开具', tone: 'success' },
    draft: { label: '草稿', tone: 'warning' },
    voided: { label: '已作废', tone: 'danger' },
    void: { label: '已作废', tone: 'danger' },
    locked: { label: '已锁定', tone: 'success' },
  }
  return map[s] ?? { label: status, tone: 'neutral' }
}

const PAY_LABEL: Record<string, string> = {
  manual: '手动入账',
  transfer: '对公转账',
  wechat: '微信支付',
  alipay: '支付宝',
}
const INVOICE_TYPE_LABEL: Record<string, string> = {
  Special: '增值税专票',
  Normal: '增值税普票',
  special: '增值税专票',
  normal: '增值税普票',
}

const yuan = (v: string | number | null | undefined) => {
  const n = Number(v)
  if (v == null || Number.isNaN(n)) return '—'
  return `¥${n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

/** 计费与账单：财务跨租户只读视图（billing.read）+ 代客充值（billing.manage）。 */
export default function Billing() {
  const adminToken = useAuthStore((s) => s.adminToken)
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm<{ org_id: number; amount: number; channel: string; memo?: string }>()

  const margin = useQuery({ queryKey: ['billing-margin'], queryFn: () => getMargin(), retry: false })
  const tenants = useQuery({ queryKey: ['billing-tenants'], queryFn: () => listTenants(), retry: false })
  const orders = useQuery({ queryKey: ['billing-orders'], queryFn: () => listOrders(), retry: false })
  const invoices = useQuery({ queryKey: ['billing-invoices'], queryFn: () => listInvoices(), retry: false })
  const recon = useQuery({ queryKey: ['billing-recon'], queryFn: () => listReconciliations(), retry: false })

  // 任一只读查询 403/401 → 提示需财务权限或管理令牌。
  const denied = [margin, tenants, orders, invoices, recon].some((q) => {
    const code = (q.error as { response?: { status?: number } } | null)?.response?.status
    return code === 401 || code === 403
  })

  const rechargeMutation = useMutation({
    mutationFn: (v: { org_id: number; amount: number; memo?: string }) => recharge(v.org_id, v.amount, v.memo),
    onSuccess: (r) => {
      message.success(`充值成功：org #${r.org_id} 余额 ¥${r.balance}`)
      setOpen(false)
      form.resetFields()
      tenants.refetch()
      orders.refetch()
    },
    onError: (e) => message.error((e as { localizedMessage?: string }).localizedMessage ?? '充值失败'),
  })

  const submit = async () => {
    let v: { org_id: number; amount: number; channel: string; memo?: string }
    try {
      v = await form.validateFields()
    } catch {
      return
    }
    rechargeMutation.mutate({ org_id: v.org_id, amount: v.amount, memo: v.memo })
  }

  // 概览 KPI：营收/成本/毛利率取 margin 总览单元格；账户余额合计取 tenants 钱包余额之和。
  const cell = margin.data?.rows?.[0]
  const totalBalance = (tenants.data ?? []).reduce((s, t) => s + Number(t.balance || 0), 0)
  const pendingOrders = (orders.data ?? []).filter((o) => statusMeta(o.status).tone === 'warning').length

  const loadingAny = margin.isLoading || tenants.isLoading

  const overview = (
    <>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label={`本月营收（${margin.data?.period ?? '—'}）`} value={cell ? yuan(cell.revenue) : '—'} accent />
        <KpiCard
          label="渠道成本"
          value={cell ? yuan(cell.cost) : '—'}
          hint={cell?.margin_rate != null ? `毛利率 ${(Number(cell.margin_rate) * 100).toFixed(1)}%` : margin.data ? '毛利率 —' : undefined}
          hintTone="muted"
        />
        <KpiCard label="待支付订单" value={pendingOrders} hint={`共 ${orders.data?.length ?? 0} 笔`} hintTone="muted" />
        <KpiCard label="账户余额合计" value={yuan(totalBalance)} hint={`跨 ${tenants.data?.length ?? 0} 租户`} hintTone="muted" />
      </div>
      {margin.data && !margin.data.cost_complete && (
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="部分调用未配渠道成本价，毛利偏乐观（未配成本按 0 计）。"
        />
      )}
      <SectionCard title="租户用量与余额" flush>
        <Table
          rowKey="org_id"
          size="middle"
          loading={tenants.isLoading}
          pagination={false}
          dataSource={tenants.data ?? []}
          columns={[
            { title: '租户', dataIndex: 'org_name' },
            { title: '调用量', dataIndex: 'calls', align: 'right', render: (v) => <span className="rr-num">{Number(v).toLocaleString()}</span> },
            { title: '本月消费', dataIndex: 'charged', align: 'right', render: (v) => <span className="rr-num">{yuan(v)}</span> },
            { title: '账户余额', dataIndex: 'balance', align: 'right', render: (v) => <span className="rr-num">{yuan(v)}</span> },
          ]}
        />
      </SectionCard>
    </>
  )

  return (
    <div>
      <PageHeader
        title="计费与账单"
        subtitle="充值、消费流水、对账与发票 —— 企业财务维度统一治理。"
        extra={
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
            发起充值
          </Button>
        }
      />

      {denied && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message="无财务读取权限"
          description="计费视图需 billing.read（财务/管理员角色），或在「系统设置 · 管理令牌」填入管理令牌以超管身份查看。"
        />
      )}

      {loadingAny && !denied ? (
        <div style={{ padding: 60, textAlign: 'center' }}>
          <Spin />
        </div>
      ) : (
        <Tabs
          items={[
            { key: 'overview', label: '账单总览', children: overview },
            {
              key: 'orders',
              label: '充值记录',
              children: (
                <SectionCard flush>
                  <Table
                    rowKey="id"
                    size="middle"
                    loading={orders.isLoading}
                    pagination={{ pageSize: 20 }}
                    dataSource={orders.data ?? []}
                    columns={[
                      { title: '订单', dataIndex: 'id', render: (v) => <span className="rr-num">#{v}</span> },
                      { title: '租户', dataIndex: 'org_name' },
                      { title: '金额', dataIndex: 'amount', align: 'right', render: (v) => <span className="rr-num">{yuan(v)}</span> },
                      { title: '支付方式', dataIndex: 'pay_channel', render: (v) => PAY_LABEL[v] ?? v },
                      { title: '时间', dataIndex: 'created_at', render: (v) => <span className="rr-num">{dayjs(v).format('YYYY-MM-DD HH:mm')}</span> },
                      { title: '状态', dataIndex: 'status', align: 'right', render: (v) => { const m = statusMeta(v); return <StatusPill tone={m.tone} dot>{m.label}</StatusPill> } },
                    ]}
                  />
                </SectionCard>
              ),
            },
            {
              key: 'invoices',
              label: '发票管理',
              children: (
                <SectionCard flush>
                  <Table
                    rowKey="id"
                    size="middle"
                    loading={invoices.isLoading}
                    pagination={{ pageSize: 20 }}
                    dataSource={invoices.data ?? []}
                    columns={[
                      { title: '发票', dataIndex: 'id', render: (v) => <span className="rr-num">#{v}</span> },
                      { title: '租户', dataIndex: 'org_name' },
                      { title: '抬头', dataIndex: 'title' },
                      { title: '类型', dataIndex: 'invoice_type', render: (v) => INVOICE_TYPE_LABEL[v] ?? v },
                      { title: '金额', dataIndex: 'amount', align: 'right', render: (v) => <span className="rr-num">{yuan(v)}</span> },
                      { title: '状态', dataIndex: 'status', align: 'right', render: (v) => { const m = statusMeta(v); return <StatusPill tone={m.tone} dot>{m.label}</StatusPill> } },
                    ]}
                  />
                </SectionCard>
              ),
            },
            {
              key: 'recon',
              label: '对账',
              children: (
                <SectionCard flush>
                  <Table
                    rowKey="id"
                    size="middle"
                    loading={recon.isLoading}
                    pagination={false}
                    dataSource={recon.data ?? []}
                    columns={[
                      { title: '账期', dataIndex: 'period', render: (v) => <span className="rr-num">{v}</span> },
                      { title: '营收', dataIndex: 'total_revenue', align: 'right', render: (v) => <span className="rr-num">{yuan(v)}</span> },
                      { title: '调用数', dataIndex: 'total_calls', align: 'right', render: (v) => <span className="rr-num">{Number(v).toLocaleString()}</span> },
                      { title: '上游成本', dataIndex: 'upstream_cost', align: 'right', render: (v) => <span className="rr-num">{v == null ? '—' : yuan(v)}</span> },
                      { title: '差额', dataIndex: 'gap', align: 'right', render: (v) => <span className="rr-num">{v == null ? '—' : yuan(v)}</span> },
                      { title: '对账单', dataIndex: 'status', align: 'right', render: (v) => { const m = statusMeta(v); return <StatusPill tone={m.tone}>{m.label}</StatusPill> } },
                    ]}
                  />
                </SectionCard>
              ),
            },
          ]}
        />
      )}

      <Modal
        title="发起充值"
        open={open}
        onOk={submit}
        confirmLoading={rechargeMutation.isPending}
        onCancel={() => setOpen(false)}
        destroyOnClose
      >
        {!adminToken && (
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 12 }}
            message="充值需要 billing.manage（财务/管理员角色，或管理令牌）。"
          />
        )}
        <Form form={form} layout="vertical" preserve={false} initialValues={{ channel: 'transfer' }}>
          <Form.Item name="org_id" label="租户组织 ID" rules={[{ required: true, message: '请填写组织 ID' }]}>
            <InputNumber style={{ width: '100%' }} min={1} precision={0} placeholder="目标组织的数字 ID" />
          </Form.Item>
          <Form.Item name="amount" label="充值金额（元）" rules={[{ required: true, message: '请填写金额' }]}>
            <InputNumber style={{ width: '100%' }} min={0.01} precision={2} placeholder="如 50000" />
          </Form.Item>
          <Form.Item name="channel" label="支付方式">
            <Radio.Group
              options={[
                { value: 'transfer', label: '对公转账' },
                { value: 'wechat', label: '微信支付' },
                { value: 'alipay', label: '支付宝' },
              ]}
              optionType="button"
            />
          </Form.Item>
          <Form.Item name="memo" label="备注">
            <Input placeholder="可选，如合同号 / 备注" maxLength={128} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
