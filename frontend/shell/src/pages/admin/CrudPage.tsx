import { useMemo, useState } from 'react'
import {
  Table,
  Button,
  Modal,
  Form,
  Input,
  InputNumber,
  Select,
  Switch,
  Popconfirm,
  Space,
  Typography,
  Alert,
  message,
  DatePicker,
  Tag,
} from 'antd'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { useQuery, useQueries, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnsType } from 'antd/es/table'
import dayjs from 'dayjs'
import { api } from '../../api/client'
import { useAuthStore } from '../../store/auth'

export type FieldType =
  | 'text'
  | 'number'
  | 'textarea'
  | 'json'
  | 'select'
  | 'switch'
  | 'datetime'

export interface FieldOption {
  label: string
  value: string | number
}

export interface FieldDef {
  name: string
  label: string
  type: FieldType
  required?: boolean
  options?: FieldOption[]
  optionsLoader?: () => Promise<FieldOption[]>
  /** 默认 true；身份字段（如 slug 一旦建立）可设 false 使其不可编辑 */
  inEdit?: boolean
  /** 默认 true；服务端生成或只读字段可设 false 不出现在创建表单 */
  inCreate?: boolean
  /** 是否作为表格列展示（默认 false） */
  inTable?: boolean
  help?: string
  placeholder?: string
}

/** 行级自定义操作（如渠道连通性测试）：调 `run(id)`，结果弹窗展示并刷新列表。 */
export interface RowActionDef {
  label: string
  run: (id: number) => Promise<unknown>
}

export interface ResourceDef {
  base: string
  fields: FieldDef[]
  /** 创建响应里携带的一次性密钥（如 api key 明文） */
  secret?: { field: string; entityField: string; label: string }
  /** 操作列附加的行级动作 */
  rowActions?: RowActionDef[]
}

type Row = Record<string, unknown>

/** 把表单值转换为后端载荷：json 串→对象、dayjs→ISO、空值显式置 null（以支持"清空"）。 */
function toPayload(fields: FieldDef[], values: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const f of fields) {
    const v = values[f.name]
    if (v === undefined || v === null || v === '') {
      // 显式置 null（而非剔除键）：后端可清空字段（如 org.group_id 走 double_option）据此清除；
      // 仅支持 None=不变 的字段会把 null 当 None 安全忽略。required 字段已被表单校验拦截，不会到此。
      out[f.name] = null
      continue
    }
    if (f.type === 'json') {
      out[f.name] = JSON.parse(v as string) // 解析失败由调用方 catch → 提示
    } else if (f.type === 'datetime') {
      out[f.name] = (v as dayjs.Dayjs).toISOString()
    } else {
      out[f.name] = v
    }
  }
  return out
}

/** 把记录转为表单初值：json→美化串、datetime→dayjs。 */
function toFormValues(fields: FieldDef[], record: Row): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const f of fields) {
    const v = record[f.name]
    if (v === undefined || v === null) continue
    if (f.type === 'json') out[f.name] = JSON.stringify(v, null, 2)
    else if (f.type === 'datetime') out[f.name] = dayjs(v as string)
    else out[f.name] = v
  }
  return out
}

function renderCell(f: FieldDef, value: unknown, optionMap: Map<string | number, string>): string {
  if (value === null || value === undefined) return '—'
  if (f.type === 'json') {
    const s = JSON.stringify(value)
    return s.length > 60 ? `${s.slice(0, 60)}…` : s
  }
  if (f.type === 'datetime') return dayjs(value as string).format('YYYY-MM-DD HH:mm')
  if (f.type === 'switch') return value ? '✓' : '✗'
  if (f.type === 'select') return optionMap.get(value as string | number) ?? String(value)
  return String(value)
}

/** 行级动作结果展示：对含 ok 的结果（如渠道测试）友好渲染，否则退化为 JSON。 */
function ActionResult({ result }: { result: unknown }) {
  const r = (result ?? {}) as Record<string, unknown>
  if (typeof r.ok === 'boolean') {
    return (
      <div>
        <Space style={{ marginBottom: 8 }}>
          <Tag color={r.ok ? 'success' : 'error'}>{r.ok ? '连通正常' : '连通失败'}</Tag>
          <Typography.Text type="secondary">
            状态 {String(r.status)} · 耗时 {String(r.latency_ms)}ms · 模型 {String(r.model)}
          </Typography.Text>
        </Space>
        {r.error ? (
          <Alert type="error" showIcon message={String(r.error)} style={{ marginBottom: 8 }} />
        ) : null}
        {r.usage != null ? (
          <Typography.Paragraph code style={{ marginBottom: 0 }}>
            usage: {JSON.stringify(r.usage)}
          </Typography.Paragraph>
        ) : null}
      </div>
    )
  }
  return (
    <pre style={{ maxHeight: 300, overflow: 'auto', margin: 0 }}>
      {JSON.stringify(result, null, 2)}
    </pre>
  )
}

export default function CrudPage({ resource, title }: { resource: ResourceDef; title: string }) {
  const qc = useQueryClient()
  const adminToken = useAuthStore((s) => s.adminToken)
  const [form] = Form.useForm()
  const [open, setOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [secretValue, setSecretValue] = useState<string | null>(null)
  const [actionModal, setActionModal] = useState<{ title: string; result: unknown } | null>(null)

  const listQuery = useQuery({
    queryKey: ['admin', resource.base],
    queryFn: async () => (await api.get<Row[]>(resource.base)).data,
    enabled: !!adminToken,
  })

  // 动态加载所有带 optionsLoader 字段的下拉项（FK 选择）。
  const loaderFields = useMemo(
    () => resource.fields.filter((f) => f.optionsLoader),
    [resource.fields],
  )
  const optionQueries = useQueries({
    queries: loaderFields.map((f) => ({
      queryKey: ['admin-options', resource.base, f.name],
      queryFn: f.optionsLoader!,
      staleTime: 30_000,
      enabled: !!adminToken,
    })),
  })
  const optionsByField = useMemo(() => {
    const m: Record<string, FieldOption[]> = {}
    loaderFields.forEach((f, i) => {
      m[f.name] = (optionQueries[i].data as FieldOption[] | undefined) ?? f.options ?? []
    })
    resource.fields
      .filter((f) => f.options && !f.optionsLoader)
      .forEach((f) => {
        m[f.name] = f.options!
      })
    return m
    // 依赖用 dataUpdatedAt（数据刷新时变化的时间戳）而非 .data：对象 join 会被字符串化成
    // '[object Object]'，条数不变时即便内容变了也不重算，导致 FK 选项/标签陈旧。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaderFields, optionQueries.map((q) => q.dataUpdatedAt).join(','), resource.fields])

  const saveMutation = useMutation({
    mutationFn: async (payload: Record<string, unknown>) => {
      if (editingId == null) return (await api.post(resource.base, payload)).data
      return (await api.put(`${resource.base}/${editingId}`, payload)).data
    },
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['admin', resource.base] })
      setOpen(false)
      if (editingId == null && resource.secret) {
        const sv = (data as Row)?.[resource.secret.field]
        if (typeof sv === 'string') setSecretValue(sv)
      } else {
        message.success('已保存')
      }
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '保存失败'),
  })

  const deleteMutation = useMutation({
    mutationFn: async (id: number) => {
      await api.delete(`${resource.base}/${id}`)
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['admin', resource.base] })
      message.success('已删除')
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '删除失败'),
  })

  const actionMutation = useMutation({
    mutationFn: ({ action, id }: { action: RowActionDef; id: number }) => action.run(id),
    onSuccess: (result, { action }) => {
      // 行级动作可能改变记录（如渠道测试写回测速/触发熔断）→ 刷新列表
      qc.invalidateQueries({ queryKey: ['admin', resource.base] })
      setActionModal({ title: action.label, result })
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '操作失败'),
  })

  const openCreate = () => {
    setEditingId(null)
    form.resetFields()
    setOpen(true)
  }
  const openEdit = (record: Row) => {
    setEditingId(record.id as number)
    form.resetFields()
    form.setFieldsValue(toFormValues(resource.fields, record))
    setOpen(true)
  }

  const onSubmit = async () => {
    let values: Record<string, unknown>
    try {
      values = await form.validateFields()
    } catch {
      return
    }
    const mode = editingId == null ? 'create' : 'edit'
    const usable = resource.fields.filter((f) =>
      mode === 'create' ? f.inCreate !== false : f.inEdit !== false,
    )
    let payload: Record<string, unknown>
    try {
      payload = toPayload(usable, values)
    } catch {
      message.error('JSON 字段格式不合法')
      return
    }
    saveMutation.mutate(payload)
  }

  const optionMaps = useMemo(() => {
    const maps: Record<string, Map<string | number, string>> = {}
    for (const f of resource.fields) {
      maps[f.name] = new Map((optionsByField[f.name] ?? []).map((o) => [o.value, o.label]))
    }
    return maps
  }, [optionsByField, resource.fields])

  const columns: ColumnsType<Row> = [
    ...resource.fields
      .filter((f) => f.inTable)
      .map((f) => ({
        title: f.label,
        dataIndex: f.name,
        key: f.name,
        render: (v: unknown) => renderCell(f, v, optionMaps[f.name]),
      })),
    {
      title: '操作',
      key: '__actions',
      width: 160 + (resource.rowActions?.length ?? 0) * 60,
      render: (_: unknown, record: Row) => (
        <Space>
          {(resource.rowActions ?? []).map((a) => (
            <Button
              key={a.label}
              size="small"
              loading={actionMutation.isPending}
              onClick={() => actionMutation.mutate({ action: a, id: record.id as number })}
            >
              {a.label}
            </Button>
          ))}
          <Button size="small" onClick={() => openEdit(record)}>
            编辑
          </Button>
          <Popconfirm
            title="确认删除？"
            onConfirm={() => deleteMutation.mutate(record.id as number)}
          >
            <Button size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  const mode = editingId == null ? 'create' : 'edit'
  const formFields = resource.fields.filter((f) =>
    mode === 'create' ? f.inCreate !== false : f.inEdit !== false,
  )

  if (!adminToken) {
    return (
      <Alert
        type="warning"
        showIcon
        message="未设置管理令牌"
        description="管理台 CRUD 需要管理令牌（X-Admin-Token）。请到「系统设置 · 管理令牌」填入后端 RR_ADMIN_TOKEN。"
      />
    )
  }

  return (
    <div>
      <div
        style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16, alignItems: 'center' }}
      >
        <Typography.Title level={4} style={{ margin: 0 }}>
          {title}
        </Typography.Title>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => qc.invalidateQueries({ queryKey: ['admin', resource.base] })}
          >
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            新建
          </Button>
        </Space>
      </div>

      {listQuery.isError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="加载失败"
          description={
            (listQuery.error as { localizedMessage?: string })?.localizedMessage ??
            '请检查管理令牌是否正确、后端是否就绪。'
          }
        />
      )}

      <Table<Row>
        rowKey="id"
        loading={listQuery.isLoading}
        columns={columns}
        dataSource={listQuery.data ?? []}
        size="middle"
        pagination={{ pageSize: 20, showSizeChanger: true }}
        scroll={{ x: true }}
      />

      <Modal
        title={`${mode === 'create' ? '新建' : '编辑'} · ${title}`}
        open={open}
        onOk={onSubmit}
        confirmLoading={saveMutation.isPending}
        onCancel={() => setOpen(false)}
        destroyOnClose
        width={640}
      >
        <Form form={form} layout="vertical" preserve={false}>
          {formFields.map((f) => (
            <Form.Item
              key={f.name}
              name={f.name}
              label={f.label}
              extra={f.help}
              valuePropName={f.type === 'switch' ? 'checked' : 'value'}
              rules={f.required ? [{ required: true, message: `请填写${f.label}` }] : undefined}
            >
              {f.type === 'text' && <Input placeholder={f.placeholder} />}
              {f.type === 'number' && <InputNumber style={{ width: '100%' }} />}
              {f.type === 'textarea' && <Input.TextArea rows={2} placeholder={f.placeholder} />}
              {f.type === 'json' && (
                <Input.TextArea rows={5} placeholder={f.placeholder ?? '{ }'} style={{ fontFamily: 'monospace' }} />
              )}
              {f.type === 'switch' && <Switch />}
              {f.type === 'datetime' && <DatePicker showTime style={{ width: '100%' }} />}
              {f.type === 'select' && (
                <Select
                  allowClear
                  showSearch
                  optionFilterProp="label"
                  options={optionsByField[f.name] ?? []}
                  placeholder={f.placeholder}
                />
              )}
            </Form.Item>
          ))}
        </Form>
      </Modal>

      <Modal
        title={resource.secret?.label ?? '密钥'}
        open={secretValue != null}
        onOk={() => setSecretValue(null)}
        onCancel={() => setSecretValue(null)}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 12 }}
          message="请立即复制保存：明文仅此一次展示，关闭后无法再次获取。"
        />
        <Typography.Paragraph copyable code style={{ wordBreak: 'break-all' }}>
          {secretValue}
        </Typography.Paragraph>
      </Modal>

      <Modal
        title={actionModal?.title ?? '结果'}
        open={actionModal != null}
        onOk={() => setActionModal(null)}
        onCancel={() => setActionModal(null)}
        cancelButtonProps={{ style: { display: 'none' } }}
        width={560}
      >
        {actionModal && <ActionResult result={actionModal.result} />}
      </Modal>
    </div>
  )
}
