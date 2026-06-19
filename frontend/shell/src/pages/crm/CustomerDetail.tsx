import { useState } from 'react'
import {
  Button,
  Space,
  Typography,
  Tag,
  Descriptions,
  Card,
  Statistic,
  Row,
  Col,
  Tabs,
  Table,
  Input,
  InputNumber,
  Modal,
  Form,
  List,
  Alert,
  Spin,
  Empty,
  message,
} from 'antd'
import { ArrowLeftOutlined, DollarOutlined, SwapOutlined, ReloadOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnsType } from 'antd/es/table'
import { useNavigate, useParams } from 'react-router-dom'
import dayjs from 'dayjs'
import {
  getCustomer,
  listNotes,
  createNote,
  listAssignments,
  assignCustomer,
  rechargeCustomer,
  type Customer,
  type Assignment,
} from '../../api/crm'
import { ORG_TYPE_LABEL, ORG_STATUS_LABEL, ORG_STATUS_COLOR, REALNAME_LABEL } from './labels'

function fmtMoney(v: string | null | undefined): string {
  const num = Number(v)
  if (v == null || v === '' || Number.isNaN(num)) return '0.00'
  return num.toLocaleString('zh-CN', { minimumFractionDigits: 2 })
}

export default function CustomerDetail() {
  const { orgId: orgIdParam } = useParams<{ orgId: string }>()
  const orgId = Number(orgIdParam)
  const navigate = useNavigate()
  const qc = useQueryClient()

  const [rechargeOpen, setRechargeOpen] = useState(false)
  const [assignOpen, setAssignOpen] = useState(false)
  const [noteContent, setNoteContent] = useState('')
  const [rechargeForm] = Form.useForm<{ amount: number; pay_channel?: string; memo?: string }>()
  const [assignForm] = Form.useForm<{ sales_id: number }>()

  const custQuery = useQuery({
    queryKey: ['crm-customer', orgId],
    queryFn: () => getCustomer(orgId),
    enabled: Number.isFinite(orgId),
  })
  const notesQuery = useQuery({
    queryKey: ['crm-notes', orgId],
    queryFn: () => listNotes(orgId, { limit: 100 }),
    enabled: Number.isFinite(orgId),
  })
  const assignQuery = useQuery({
    queryKey: ['crm-assignments', orgId],
    queryFn: () => listAssignments(orgId),
    enabled: Number.isFinite(orgId),
  })

  const rechargeMutation = useMutation({
    mutationFn: (v: { amount: number; pay_channel?: string; memo?: string }) =>
      rechargeCustomer(orgId, {
        // 与表单 precision={2} 对齐：toFixed(2) 规范化为十进制串，避免浮点/科学计数法污染上送（后端 rust_decimal 解析）
        amount: v.amount.toFixed(2),
        pay_channel: v.pay_channel?.trim() || undefined,
        memo: v.memo?.trim() || undefined,
      }),
    onSuccess: (resp) => {
      message.success(`充值成功，余额 ${fmtMoney(resp.balance)}（订单 #${resp.order.id}）`)
      setRechargeOpen(false)
      rechargeForm.resetFields()
      qc.invalidateQueries({ queryKey: ['crm-customer', orgId] })
      qc.invalidateQueries({ queryKey: ['crm-customers'] }) // 列表余额随之刷新
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '充值失败'),
  })

  const noteMutation = useMutation({
    mutationFn: (content: string) => createNote(orgId, content),
    onSuccess: () => {
      setNoteContent('')
      qc.invalidateQueries({ queryKey: ['crm-notes', orgId] })
      message.success('已记录跟进')
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '记录失败'),
  })

  const assignMutation = useMutation({
    mutationFn: (salesId: number) => assignCustomer(orgId, salesId),
    onSuccess: () => {
      setAssignOpen(false)
      assignForm.resetFields()
      qc.invalidateQueries({ queryKey: ['crm-customer', orgId] })
      qc.invalidateQueries({ queryKey: ['crm-assignments', orgId] })
      qc.invalidateQueries({ queryKey: ['crm-customers'] }) // 列表归属销售随之刷新
      message.success('已改派归属')
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '改派失败'),
  })

  if (custQuery.isLoading) {
    return (
      <div style={{ textAlign: 'center', padding: 64 }}>
        <Spin />
      </div>
    )
  }
  if (custQuery.isError || !custQuery.data) {
    return (
      <div>
        <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/crm')} style={{ marginBottom: 16 }}>
          返回
        </Button>
        <Alert
          type="error"
          showIcon
          message="无法加载客户"
          description={
            (custQuery.error as { localizedMessage?: string })?.localizedMessage ??
            '客户不存在或不在你的数据域内（越域访问返回 404）。'
          }
        />
      </div>
    )
  }

  const c: Customer = custQuery.data

  const assignColumns: ColumnsType<Assignment> = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 72 },
    { title: '销售 id', dataIndex: 'sales_id', key: 'sales_id', render: (v: number) => `#${v}` },
    {
      title: '归属时间',
      dataIndex: 'assigned_at',
      key: 'assigned_at',
      render: (v: string) => dayjs(v).format('YYYY-MM-DD HH:mm'),
    },
    {
      title: '当前',
      dataIndex: 'active',
      key: 'active',
      width: 88,
      render: (v: boolean) => (v ? <Tag color="green">生效中</Tag> : <Tag>历史</Tag>),
    },
  ]

  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 16,
        }}
      >
        <Space>
          <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/crm')}>
            返回
          </Button>
          <Typography.Title level={4} style={{ margin: 0 }}>
            {c.name}
            <Typography.Text type="secondary" style={{ fontSize: 14, marginLeft: 8 }}>
              org #{c.id}
            </Typography.Text>
          </Typography.Title>
        </Space>
        <Space>
          <Button icon={<SwapOutlined />} onClick={() => setAssignOpen(true)}>
            改派归属
          </Button>
          <Button type="primary" icon={<DollarOutlined />} onClick={() => setRechargeOpen(true)}>
            代客充值
          </Button>
        </Space>
      </div>

      <Card style={{ marginBottom: 16 }}>
        <Row gutter={16} style={{ marginBottom: 8 }}>
          <Col span={8}>
            <Statistic title="余额" value={fmtMoney(c.balance)} />
          </Col>
          <Col span={8}>
            <Statistic title="授信额度" value={fmtMoney(c.credit_limit)} />
          </Col>
          <Col span={8}>
            <Statistic title="冻结" value={fmtMoney(c.frozen)} />
          </Col>
        </Row>
        <Descriptions column={3} size="small" style={{ marginTop: 8 }}>
          <Descriptions.Item label="类型">{ORG_TYPE_LABEL[c.org_type] ?? c.org_type}</Descriptions.Item>
          <Descriptions.Item label="状态">
            <Tag color={ORG_STATUS_COLOR[c.status]}>{ORG_STATUS_LABEL[c.status] ?? c.status}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="实名">
            {REALNAME_LABEL[c.realname_status] ?? c.realname_status}
          </Descriptions.Item>
          <Descriptions.Item label="归属销售">
            {c.owner_sales_id == null ? '—' : `#${c.owner_sales_id}`}
          </Descriptions.Item>
          <Descriptions.Item label="商业分组">
            {c.group_id == null ? '默认价' : `#${c.group_id}`}
          </Descriptions.Item>
        </Descriptions>
      </Card>

      <Tabs
        defaultActiveKey="notes"
        items={[
          {
            key: 'notes',
            label: '跟进记录',
            children: (
              <div>
                <Space.Compact style={{ width: '100%', marginBottom: 16 }}>
                  <Input.TextArea
                    rows={2}
                    value={noteContent}
                    onChange={(e) => setNoteContent(e.target.value)}
                    placeholder="记录一条跟进（最多 2000 字）"
                    maxLength={2000}
                  />
                  <Button
                    type="primary"
                    style={{ height: 'auto' }}
                    loading={noteMutation.isPending}
                    disabled={!noteContent.trim()}
                    onClick={() => noteMutation.mutate(noteContent.trim())}
                  >
                    提交
                  </Button>
                </Space.Compact>
                {notesQuery.isError ? (
                  <Alert type="error" showIcon message="跟进记录加载失败" />
                ) : (notesQuery.data?.length ?? 0) === 0 && !notesQuery.isLoading ? (
                  <Empty description="暂无跟进记录" />
                ) : (
                  <List
                    loading={notesQuery.isLoading}
                    dataSource={notesQuery.data ?? []}
                    renderItem={(n) => (
                      <List.Item key={n.id}>
                        <List.Item.Meta
                          title={
                            <Space>
                              <span>{n.author_id == null ? '系统' : `销售 #${n.author_id}`}</span>
                              <Typography.Text type="secondary" style={{ fontWeight: 400 }}>
                                {dayjs(n.created_at).format('YYYY-MM-DD HH:mm')}
                              </Typography.Text>
                            </Space>
                          }
                          description={<span style={{ whiteSpace: 'pre-wrap' }}>{n.content}</span>}
                        />
                      </List.Item>
                    )}
                  />
                )}
              </div>
            ),
          },
          {
            key: 'assignments',
            label: '归属历史',
            children: (
              <div>
                <div style={{ marginBottom: 12, textAlign: 'right' }}>
                  <Button
                    size="small"
                    icon={<ReloadOutlined />}
                    onClick={() => qc.invalidateQueries({ queryKey: ['crm-assignments', orgId] })}
                  >
                    刷新
                  </Button>
                </div>
                <Table<Assignment>
                  rowKey="id"
                  loading={assignQuery.isLoading}
                  columns={assignColumns}
                  dataSource={assignQuery.data ?? []}
                  size="middle"
                  pagination={false}
                />
              </div>
            ),
          },
        ]}
      />

      <Modal
        title={`代客充值 · ${c.name}`}
        open={rechargeOpen}
        onOk={async () => {
          let v: { amount: number; pay_channel?: string; memo?: string }
          try {
            v = await rechargeForm.validateFields()
          } catch {
            return
          }
          rechargeMutation.mutate(v)
        }}
        confirmLoading={rechargeMutation.isPending}
        onCancel={() => setRechargeOpen(false)}
        destroyOnClose
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="线下/对公已收款后一步入账：建 Paid 订单 + 钱包入账，原子提交，计入销售业绩。"
        />
        <Form form={rechargeForm} layout="vertical" preserve={false} initialValues={{ pay_channel: 'transfer' }}>
          <Form.Item
            name="amount"
            label="充值金额（元）"
            rules={[{ required: true, message: '请填写金额' }]}
          >
            <InputNumber style={{ width: '100%' }} min={0.01} precision={2} step={100} placeholder="正数（元，到分）" />
          </Form.Item>
          <Form.Item name="pay_channel" label="支付渠道">
            <Input placeholder="transfer（对公）/ alipay / wechat" maxLength={32} />
          </Form.Item>
          <Form.Item name="memo" label="备注">
            <Input.TextArea rows={2} placeholder="可选（最多 255 字）" maxLength={255} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title={`改派归属 · ${c.name}`}
        open={assignOpen}
        onOk={async () => {
          let v: { sales_id: number }
          try {
            v = await assignForm.validateFields()
          } catch {
            return
          }
          assignMutation.mutate(v.sales_id)
        }}
        confirmLoading={assignMutation.isPending}
        onCancel={() => setAssignOpen(false)}
        destroyOnClose
      >
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message={`改派为管理动作（需 crm.assign 权限）。当前归属：${
            c.owner_sales_id == null ? '无' : `销售 #${c.owner_sales_id}`
          }`}
        />
        <Form form={assignForm} layout="vertical" preserve={false}>
          <Form.Item
            name="sales_id"
            label="改派到销售 id"
            rules={[{ required: true, message: '请填写目标销售 id' }]}
          >
            <InputNumber style={{ width: '100%' }} min={1} precision={0} placeholder="users.id（须为 sales 角色）" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
