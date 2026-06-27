import { useMemo, useState } from 'react'
import { useQueries, useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Form, Input, InputNumber, Select, Switch, DatePicker, message } from 'antd'
import dayjs from 'dayjs'
import type { FieldDef, FieldOption, ResourceDef } from './CrudPage'
import { useAuthStore } from '../../store/auth'
import { api } from '../../api/client'
import { FormDrawer } from '../../components/ui'

type Row = Record<string, unknown>

/**
 * 资源表单的共享逻辑：payload 转换 + FK 选项加载 + 表单项渲染。
 * 由通用 CrudPage 与渠道/密钥等 bespoke 页共用，避免 null/json/datetime 处理重复。
 */

/** 表单值 → 后端载荷：json 串→对象、dayjs→ISO、空值显式置 null（以支持「清空」）。 */
export function toPayload(fields: FieldDef[], values: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const f of fields) {
    const v = values[f.name]
    if (v === undefined || v === null || v === '') {
      out[f.name] = null
      continue
    }
    if (f.type === 'json') {
      out[f.name] = JSON.parse(v as string)
    } else if (f.type === 'datetime') {
      out[f.name] = (v as dayjs.Dayjs).toISOString()
    } else {
      out[f.name] = v
    }
  }
  return out
}

/** 记录 → 表单初值：json→美化串、datetime→dayjs。 */
export function toFormValues(fields: FieldDef[], record: Record<string, unknown>): Record<string, unknown> {
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

/** 加载资源所有 FK 字段的下拉项（含静态 options），返回 fieldName → options。 */
export function useResourceOptions(resource: ResourceDef): Record<string, FieldOption[]> {
  const adminToken = useAuthStore((s) => s.adminToken)
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
  return useMemo(() => {
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
    // 依赖用 dataUpdatedAt（刷新时变化的时间戳）而非 .data：对象 join 会被字符串化成
    // '[object Object]'，条数不变时即便内容变了也不重算，导致 FK 选项/标签陈旧。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaderFields, optionQueries.map((q) => q.dataUpdatedAt).join(','), resource.fields])
}

/**
 * 资源增删改 + 列表查询的复用钩子（供渠道/密钥等 bespoke 页用）。
 * 列表展示由各页自定义；本钩子只管数据流与编辑抽屉状态。
 */
export function useResourceCrud(resource: ResourceDef) {
  const qc = useQueryClient()
  const adminToken = useAuthStore((s) => s.adminToken)
  const [form] = Form.useForm()
  const [open, setOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [secretValue, setSecretValue] = useState<string | null>(null)
  const optionsByField = useResourceOptions(resource)

  const listQuery = useQuery({
    queryKey: ['admin', resource.base],
    queryFn: async () => (await api.get<Row[]>(resource.base)).data,
    enabled: !!adminToken,
  })

  const refresh = () => qc.invalidateQueries({ queryKey: ['admin', resource.base] })

  const saveMutation = useMutation({
    mutationFn: async (payload: Record<string, unknown>) => {
      if (editingId == null) return (await api.post(resource.base, payload)).data
      return (await api.put(`${resource.base}/${editingId}`, payload)).data
    },
    onSuccess: (data) => {
      refresh()
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
      refresh()
      message.success('已删除')
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '删除失败'),
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
  const submit = async () => {
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

  return {
    form,
    open,
    setOpen,
    editingId,
    mode: (editingId == null ? 'create' : 'edit') as 'create' | 'edit',
    optionsByField,
    listQuery,
    rows: (listQuery.data ?? []) as Row[],
    refresh,
    openCreate,
    openEdit,
    submit,
    saving: saveMutation.isPending,
    deleteRow: (id: number) => deleteMutation.mutate(id),
    deleting: deleteMutation.isPending,
    secretValue,
    clearSecret: () => setSecretValue(null),
  }
}

export type ResourceCrud = ReturnType<typeof useResourceCrud>

/** 编辑抽屉：接 useResourceCrud 返回值，自动按 create/edit 过滤字段并渲染表单项。 */
export function ResourceEditDrawer({
  resource,
  title,
  crud,
  width = 600,
}: {
  resource: ResourceDef
  title: string
  crud: ResourceCrud
  width?: number
}) {
  const formFields = resource.fields.filter((f) =>
    crud.mode === 'create' ? f.inCreate !== false : f.inEdit !== false,
  )
  return (
    <FormDrawer
      open={crud.open}
      title={`${crud.mode === 'create' ? '新建' : '编辑'} · ${title}`}
      onClose={() => crud.setOpen(false)}
      onOk={crud.submit}
      okLoading={crud.saving}
      width={width}
    >
      <Form form={crud.form} layout="vertical" preserve={false}>
        <ResourceFormItems fields={formFields} optionsByField={crud.optionsByField} />
      </Form>
    </FormDrawer>
  )
}

/** 按字段定义渲染 AntD 表单项列表（受 Form 上下文控制）。 */
export function ResourceFormItems({
  fields,
  optionsByField,
}: {
  fields: FieldDef[]
  optionsByField: Record<string, FieldOption[]>
}) {
  return (
    <>
      {fields.map((f) => (
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
    </>
  )
}
