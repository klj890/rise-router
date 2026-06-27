import { useState } from 'react'
import {
  Table,
  Button,
  Space,
  Input,
  InputNumber,
  Modal,
  Form,
  Select,
  Alert,
  message,
} from 'antd'
import { PlusOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnsType } from 'antd/es/table'
import { useNavigate } from 'react-router-dom'
import {
  listCustomers,
  onboardCustomer,
  type Customer,
  type OnboardReq,
  type OrgType,
} from '../../api/crm'
import { ORG_TYPE_LABEL, ORG_STATUS_LABEL, REALNAME_LABEL } from './labels'
import { PageHeader, KpiCard, SectionCard, StatusPill } from '../../components/ui'
import type { PillTone } from '../../components/ui'

const STATUS_TONE: Record<string, PillTone> = { Active: 'success', Suspended: 'neutral' }

// 销售业绩（mock：接口就绪后替换为 CRM 归属聚合）
const SALES_PERF = [
  { name: '周慕云', amount: 742 },
  { name: '吴桐', amount: 386 },
  { name: '何雨桐', amount: 154 },
]

const PAGE = 50

export default function CustomerList() {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [ownerInput, setOwnerInput] = useState('')
  const [ownerFilter, setOwnerFilter] = useState<number | undefined>(undefined)
  // 游标栈：[] = 首页；每入栈一个 cursor（上一页末条 id）进入下一页，出栈回上一页。
  const [stack, setStack] = useState<number[]>([])
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm<OnboardReq & { owner_sales_id?: number }>()

  const cursor = stack.length ? stack[stack.length - 1] : undefined

  // 多取一条（PAGE+1）探测是否还有下一页：满 PAGE 但无更多时不再误显「下一页」（避免点进空白页）。
  const listQuery = useQuery({
    queryKey: ['crm-customers', ownerFilter, cursor],
    queryFn: () => listCustomers({ owner_sales_id: ownerFilter, limit: PAGE + 1, cursor }),
  })
  const rawRows = listQuery.data ?? []
  const hasNext = rawRows.length > PAGE
  const rows = hasNext ? rawRows.slice(0, PAGE) : rawRows

  const onboardMutation = useMutation({
    mutationFn: (req: OnboardReq) => onboardCustomer(req),
    onSuccess: (resp) => {
      message.success(`已开户：${resp.org.name}（org #${resp.org.id}）`)
      setOpen(false)
      form.resetFields()
      setStack([])
      qc.invalidateQueries({ queryKey: ['crm-customers'] })
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '开户失败'),
  })

  const applyFilter = () => {
    const v = ownerInput.trim()
    const num = Number(v)
    // 非正整数（字母/符号/负数/0/小数）→ 传后端 owner_sales_id 会 400；前端先拦。
    if (v && (!Number.isInteger(num) || num <= 0)) {
      message.error('请输入有效的销售 id（正整数）')
      return
    }
    setOwnerFilter(v ? num : undefined)
    setStack([])
  }

  const submitOnboard = async () => {
    let values: OnboardReq & { owner_sales_id?: number }
    try {
      values = await form.validateFields()
    } catch {
      return
    }
    onboardMutation.mutate({
      phone: values.phone.trim(),
      name: values.name.trim(),
      org_type: values.org_type,
      nickname: values.nickname?.trim() || undefined,
      owner_sales_id: values.owner_sales_id || undefined,
    })
  }

  const columns: ColumnsType<Customer> = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 72 },
    { title: '名称', dataIndex: 'name', key: 'name' },
    {
      title: '类型',
      dataIndex: 'org_type',
      key: 'org_type',
      width: 80,
      render: (v: OrgType) => ORG_TYPE_LABEL[v] ?? v,
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 80,
      render: (v: Customer['status']) => (
        <StatusPill tone={STATUS_TONE[v] ?? 'neutral'} dot>
          {ORG_STATUS_LABEL[v] ?? v}
        </StatusPill>
      ),
    },
    {
      title: '实名',
      dataIndex: 'realname_status',
      key: 'realname_status',
      width: 110,
      render: (v: Customer['realname_status']) => REALNAME_LABEL[v] ?? v,
    },
    {
      title: '归属销售',
      dataIndex: 'owner_sales_id',
      key: 'owner_sales_id',
      width: 100,
      render: (v: number | null) => (v == null ? '—' : `#${v}`),
    },
    {
      title: '余额',
      dataIndex: 'balance',
      key: 'balance',
      align: 'right',
      render: (v: string) => (Number(v) || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 }),
    },
    {
      title: '授信',
      dataIndex: 'credit_limit',
      key: 'credit_limit',
      align: 'right',
      render: (v: string) => (Number(v) || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 }),
    },
    {
      title: '操作',
      key: '__actions',
      width: 88,
      render: (_: unknown, r: Customer) => (
        <Button size="small" type="link" onClick={() => navigate(`/crm/${r.id}`)}>
          详情
        </Button>
      ),
    },
  ]

  const maxPerf = Math.max(...SALES_PERF.map((s) => s.amount))

  return (
    <div>
      <PageHeader
        title="客户与销售"
        subtitle="客户档案、销售归属与跟进 —— 业绩基于计费流水自动归因。"
        extra={
          <Space>
            <Input
              placeholder="按归属销售 id 过滤"
              value={ownerInput}
              onChange={(e) => setOwnerInput(e.target.value)}
              onPressEnter={applyFilter}
              allowClear
              onClear={() => {
                // 不能复用 applyFilter：其读到的是清除前的旧 ownerInput 闭包值（异步 setState 未生效）。
                setOwnerInput('')
                setOwnerFilter(undefined)
                setStack([])
              }}
              style={{ width: 180 }}
              suffix={<SearchOutlined onClick={applyFilter} style={{ cursor: 'pointer' }} />}
            />
            <Button
              icon={<ReloadOutlined />}
              onClick={() => qc.invalidateQueries({ queryKey: ['crm-customers'] })}
            >
              刷新
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
              代客开户
            </Button>
          </Space>
        }
      />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label="客户总数" value={listQuery.isLoading ? '…' : rows.length} accent hint="本月新签 26" />
        <KpiCard label="本月销售额" value="¥1.28M" hint="环比 +18.2%" hintTone="positive" />
        <KpiCard label="客单价" value="¥3,742" hint="环比 +4.1%" hintTone="positive" />
        <KpiCard label="续约率" value="91.4%" hint="近 12 个月" hintTone="muted" />
      </div>

      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="数据域：销售仅见自己名下客户；管理员/财务可按归属销售 id 过滤查看全部。"
      />

      {listQuery.isError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="加载失败"
          description={
            (listQuery.error as { localizedMessage?: string })?.localizedMessage ??
            '请检查登录态与 CRM 权限（crm.read）。'
          }
        />
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr', gap: 16 }}>
        <SectionCard title="客户档案" flush>
          <Table<Customer>
            rowKey="id"
            loading={listQuery.isLoading}
            columns={columns}
            dataSource={rows}
            size="middle"
            pagination={false}
            scroll={{ x: true }}
          />
        </SectionCard>

        <SectionCard title="销售业绩">
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            {SALES_PERF.map((s) => (
              <div key={s.name}>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 5 }}>
                  <span style={{ color: 'var(--rr-text)' }}>{s.name}</span>
                  <span className="rr-num" style={{ color: 'var(--rr-text-2)' }}>¥{s.amount}K</span>
                </div>
                <div style={{ height: 7, borderRadius: 4, background: 'var(--rr-surface-2)', overflow: 'hidden' }}>
                  <div style={{ width: `${(s.amount / maxPerf) * 100}%`, height: '100%', background: 'var(--rr-primary)' }} />
                </div>
              </div>
            ))}
          </div>
        </SectionCard>
      </div>

      <div style={{ marginTop: 16, display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button disabled={stack.length === 0} onClick={() => setStack(stack.slice(0, -1))}>
          上一页
        </Button>
        <Button
          disabled={!hasNext}
          onClick={() => rows.length > 0 && setStack([...stack, rows[rows.length - 1].id])}
        >
          下一页
        </Button>
      </div>

      <Modal
        title="代客开户"
        open={open}
        onOk={submitOnboard}
        confirmLoading={onboardMutation.isPending}
        onCancel={() => setOpen(false)}
        destroyOnClose
        width={520}
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="为新客户创建组织 + 登录账号 + 首条归属，原子提交。客户后续用手机号 + 短信验证码登录。"
        />
        <Form form={form} layout="vertical" preserve={false} initialValues={{ org_type: 'Enterprise' as OrgType }}>
          <Form.Item
            name="phone"
            label="客户手机号"
            rules={[
              { required: true, message: '请填写手机号' },
              // 与后端 valid_phone 对齐：11 位、首位 1、次位 3-9、全数字
              { pattern: /^1[3-9]\d{9}$/, message: '请输入有效的 11 位手机号' },
            ]}
          >
            <Input placeholder="11 位手机号（客户登录主通道）" maxLength={11} />
          </Form.Item>
          <Form.Item
            name="name"
            label="组织名称"
            rules={[
              { required: true, message: '请填写组织名称' },
              { whitespace: true, message: '组织名称不能全为空格' },
            ]}
          >
            <Input placeholder="企业/客户名称" maxLength={128} />
          </Form.Item>
          <Form.Item name="org_type" label="组织类型">
            <Select
              options={[
                { value: 'Enterprise', label: '企业' },
                { value: 'Individual', label: '个人' },
              ]}
            />
          </Form.Item>
          <Form.Item name="nickname" label="客户昵称">
            <Input placeholder="可选" maxLength={64} />
          </Form.Item>
          <Form.Item
            name="owner_sales_id"
            label="归属销售 id"
            extra="仅管理员/财务可指定；销售本人留空自动归己。"
          >
            <InputNumber
              style={{ width: '100%' }}
              min={1}
              precision={0}
              placeholder="可选（管理员代任意销售开户）"
            />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
