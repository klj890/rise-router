import { useMemo, useState } from 'react'
import {
  Card,
  Select,
  Space,
  Button,
  DatePicker,
  InputNumber,
  Radio,
  Table,
  Tag,
  Typography,
  Alert,
  Modal,
  Form,
  Input,
  Drawer,
  List,
  Popconfirm,
  Empty,
  Spin,
  message,
} from 'antd'
import {
  BarChartOutlined,
  SaveOutlined,
  FolderOpenOutlined,
  PlayCircleOutlined,
} from '@ant-design/icons'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnsType } from 'antd/es/table'
import dayjs from 'dayjs'
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
} from 'recharts'
import {
  listDatasets,
  queryDataset,
  listReports,
  createReport,
  deleteReport,
  type Dataset,
  type QueryResp,
  type ChartType,
  type ReportConfig,
  type ReportDefinition,
  type MetricDef,
  type ResultRow,
} from '../../api/report'

const { RangePicker } = DatePicker
// 极光主题取色：青绿主色 + 几个区分色，足够多系列时循环。
const PALETTE = ['#2EE6C0', '#5B8DEF', '#F5A623', '#E0529C', '#9B6DFF', '#3FB950']
const LIMIT_DEFAULT = 1000

/** 图表 Tooltip 数值格式化：与表格千分位一致。 */
const tooltipFmt = (value: unknown) => {
  if (value == null) return '—'
  const num = Number(value)
  return Number.isNaN(num)
    ? String(value)
    : num.toLocaleString('zh-CN', { maximumFractionDigits: 2 })
}

export default function ReportBuilder() {
  const qc = useQueryClient()
  const [slug, setSlug] = useState<string | undefined>(undefined)
  const [metrics, setMetrics] = useState<string[]>([])
  const [dimensions, setDimensions] = useState<string[]>([])
  // 元素可空：AntD RangePicker 清除单边时会产生 [dayjs,null]/[null,dayjs]；后端 query 支持单边时间窗。
  const [range, setRange] = useState<[dayjs.Dayjs | null, dayjs.Dayjs | null] | null>(null)
  // 允许为 null（可清空输入框重输）；查询/保存时用 ?? LIMIT_DEFAULT 兜底。
  const [limit, setLimit] = useState<number | null>(LIMIT_DEFAULT)
  const [chartType, setChartType] = useState<ChartType>('table')
  const [result, setResult] = useState<QueryResp | null>(null)
  const [saveOpen, setSaveOpen] = useState(false)
  const [savedOpen, setSavedOpen] = useState(false)
  const [saveForm] = Form.useForm<{ name: string; visibility: 'private' | 'role' | 'org' }>()

  const datasetsQuery = useQuery({ queryKey: ['report-datasets'], queryFn: listDatasets })
  const datasets = datasetsQuery.data ?? []
  const dataset: Dataset | undefined = useMemo(
    () => datasets.find((d) => d.slug === slug),
    [datasets, slug],
  )

  const savedQuery = useQuery({
    queryKey: ['report-reports'],
    queryFn: listReports,
    enabled: savedOpen,
  })

  const runMutation = useMutation({
    mutationFn: () =>
      queryDataset(slug!, {
        metrics,
        dimensions,
        from: range?.[0]?.toISOString(),
        to: range?.[1]?.toISOString(),
        limit: limit ?? LIMIT_DEFAULT,
      }),
    onSuccess: (resp) => setResult(resp),
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '查询失败'),
  })

  const saveMutation = useMutation({
    mutationFn: (v: { name: string; visibility: 'private' | 'role' | 'org' }) => {
      const config: ReportConfig = {
        metrics,
        dimensions,
        from: range?.[0]?.toISOString(),
        to: range?.[1]?.toISOString(),
        limit: limit ?? LIMIT_DEFAULT,
        chart_type: chartType,
      }
      return createReport({ dataset_slug: slug!, name: v.name.trim(), visibility: v.visibility, config })
    },
    onSuccess: () => {
      message.success('报表已保存')
      setSaveOpen(false)
      saveForm.resetFields()
      qc.invalidateQueries({ queryKey: ['report-reports'] })
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '保存失败'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteReport(id),
    onSuccess: () => {
      message.success('已删除')
      qc.invalidateQueries({ queryKey: ['report-reports'] })
    },
    onError: (e: { localizedMessage?: string }) => message.error(e.localizedMessage ?? '删除失败'),
  })

  const onSelectDataset = (s: string) => {
    setSlug(s)
    setMetrics([])
    setDimensions([])
    setResult(null)
  }

  const loadReport = (r: ReportDefinition) => {
    const ds = datasets.find((d) => d.id === r.dataset_id)
    if (!ds) {
      message.error('报表所属数据集不可见或不存在')
      return
    }
    const c = r.config
    setSlug(ds.slug)
    setMetrics(c.metrics ?? [])
    setDimensions(c.dimensions ?? [])
    setRange(
      c.from || c.to ? [c.from ? dayjs(c.from) : null, c.to ? dayjs(c.to) : null] : null,
    )
    setLimit(c.limit ?? LIMIT_DEFAULT)
    setChartType(c.chart_type ?? 'table')
    setResult(null)
    setSavedOpen(false)
    message.info(`已载入报表「${r.name}」，点击「查询」运行`)
  }

  const canRun = !!slug && metrics.length > 0
  // 图表需至少一个维度作 X 轴；整体聚合（无维度）时退化为表格展示。
  const canChart = result != null && chartType !== 'table' && result.dimensions.length > 0

  // 表格列：维度列（原值）+ 指标列（右对齐 + 千分位）；label 取数据集声明，缺则回落 key。
  const tableColumns: ColumnsType<ResultRow> = useMemo(() => {
    if (!result) return []
    const labelOf = (key: string, defs: MetricDef[]) =>
      defs.find((d) => d.key === key)?.label ?? key
    const dimCols: ColumnsType<ResultRow> = result.dimensions.map((d) => ({
      title: labelOf(d, dataset?.dimensions ?? []),
      dataIndex: d,
      key: d,
    }))
    const metCols: ColumnsType<ResultRow> = result.metrics.map((m) => ({
      title: labelOf(m, dataset?.metrics ?? []),
      dataIndex: m,
      key: m,
      align: 'right' as const,
      render: (v: unknown) => tooltipFmt(v),
    }))
    return [...dimCols, ...metCols]
  }, [result, dataset])

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
        <Typography.Title level={4} style={{ margin: 0 }}>
          报表构建器
        </Typography.Title>
        <Space>
          <Button icon={<FolderOpenOutlined />} onClick={() => setSavedOpen(true)}>
            已存报表
          </Button>
          <Button
            type="primary"
            icon={<SaveOutlined />}
            disabled={!result}
            onClick={() => setSaveOpen(true)}
          >
            保存为报表
          </Button>
        </Space>
      </div>

      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="策展数据集 + 行级隔离：只能基于管理员策展的数据集搭建报表，查询按你的角色强制行级过滤（如客户仅本组织、销售仅本人名下），不开放原始库。"
      />

      {datasetsQuery.isError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="加载数据集失败"
          description={
            (datasetsQuery.error as { localizedMessage?: string })?.localizedMessage ??
            '请刷新页面重试'
          }
        />
      )}

      <Card style={{ marginBottom: 16 }}>
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space wrap size="middle" align="start">
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>数据集</div>
              <Select
                style={{ width: 220 }}
                placeholder="选择数据集"
                loading={datasetsQuery.isLoading}
                value={slug}
                onChange={onSelectDataset}
                options={datasets.map((d) => ({ value: d.slug, label: d.name }))}
                showSearch
                optionFilterProp="label"
              />
            </div>
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>指标（必选）</div>
              <Select
                mode="multiple"
                style={{ minWidth: 240 }}
                placeholder="选择指标"
                value={metrics}
                onChange={(v) => {
                  setMetrics(v)
                  setResult(null) // 参数变更 → 作废旧结果，强制重查后才能保存（避免保存配置与展示不一致）
                }}
                disabled={!dataset}
                options={(dataset?.metrics ?? []).map((m) => ({ value: m.key, label: m.label }))}
              />
            </div>
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>维度（可空=整体聚合）</div>
              <Select
                mode="multiple"
                style={{ minWidth: 240 }}
                placeholder="选择维度"
                value={dimensions}
                onChange={(v) => {
                  setDimensions(v)
                  setResult(null)
                }}
                disabled={!dataset}
                options={(dataset?.dimensions ?? []).map((d) => ({ value: d.key, label: d.label }))}
              />
            </div>
          </Space>
          <Space wrap size="middle" align="start">
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>时间窗（可选）</div>
              <RangePicker
                showTime
                value={range}
                onChange={(v) => {
                  setRange(v as [dayjs.Dayjs | null, dayjs.Dayjs | null] | null)
                  setResult(null)
                }}
              />
            </div>
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>行数上限</div>
              <InputNumber
                min={1}
                max={10000}
                precision={0}
                placeholder={String(LIMIT_DEFAULT)}
                value={limit ?? undefined}
                onChange={(v) => {
                  setLimit(v)
                  setResult(null)
                }}
              />
            </div>
            <div>
              <div style={{ marginBottom: 4, fontSize: 12, opacity: 0.7 }}>展示</div>
              <Radio.Group
                value={chartType}
                onChange={(e) => setChartType(e.target.value)}
                optionType="button"
                buttonStyle="solid"
                options={[
                  { value: 'table', label: '表格' },
                  { value: 'bar', label: '柱状' },
                  { value: 'line', label: '折线' },
                ]}
              />
            </div>
            <div style={{ alignSelf: 'flex-end' }}>
              <Button
                type="primary"
                icon={<PlayCircleOutlined />}
                disabled={!canRun}
                loading={runMutation.isPending}
                onClick={() => runMutation.mutate()}
              >
                查询
              </Button>
            </div>
          </Space>
        </Space>
      </Card>

      {result && (
        <Card
          title={
            <Space>
              <BarChartOutlined />
              <span>结果</span>
              <Tag>角色：{result.role}</Tag>
              {result.rls_filtered ? (
                <Tag color="orange">行级隔离生效</Tag>
              ) : (
                <Tag color="green">全量</Tag>
              )}
              <Typography.Text type="secondary">{result.rows.length} 行</Typography.Text>
            </Space>
          }
        >
          {result.rows.length === 0 ? (
            <Empty description="无数据（检查时间窗与过滤条件）" />
          ) : (
            <>
              {chartType !== 'table' && !canChart && (
                <Alert
                  type="warning"
                  showIcon
                  style={{ marginBottom: 16 }}
                  message="图表需要至少一个维度作为 X 轴；当前为整体聚合，已退化为表格展示。"
                />
              )}
              {canChart && (
                <div style={{ width: '100%', height: 360, marginBottom: 16 }}>
                  <ResponsiveContainer width="100%" height="100%">
                    {chartType === 'bar' ? (
                      <BarChart data={result.rows}>
                        <CartesianGrid strokeDasharray="3 3" opacity={0.2} />
                        <XAxis dataKey={result.dimensions[0]} />
                        <YAxis />
                        <Tooltip formatter={tooltipFmt} />
                        <Legend />
                        {result.metrics.map((m, i) => (
                          <Bar
                            key={m}
                            dataKey={m}
                            name={dataset?.metrics.find((x) => x.key === m)?.label ?? m}
                            fill={PALETTE[i % PALETTE.length]}
                          />
                        ))}
                      </BarChart>
                    ) : (
                      <LineChart data={result.rows}>
                        <CartesianGrid strokeDasharray="3 3" opacity={0.2} />
                        <XAxis dataKey={result.dimensions[0]} />
                        <YAxis />
                        <Tooltip formatter={tooltipFmt} />
                        <Legend />
                        {result.metrics.map((m, i) => (
                          <Line
                            key={m}
                            type="monotone"
                            dataKey={m}
                            name={dataset?.metrics.find((x) => x.key === m)?.label ?? m}
                            stroke={PALETTE[i % PALETTE.length]}
                            dot={false}
                          />
                        ))}
                      </LineChart>
                    )}
                  </ResponsiveContainer>
                </div>
              )}
              <Table
                rowKey={(_, i) => String(i)}
                columns={tableColumns}
                dataSource={result.rows}
                size="middle"
                pagination={{ pageSize: 20, showSizeChanger: true }}
                scroll={{ x: true }}
              />
            </>
          )}
        </Card>
      )}

      <Modal
        title="保存为报表"
        open={saveOpen}
        onOk={async () => {
          let v: { name: string; visibility: 'private' | 'role' | 'org' }
          try {
            v = await saveForm.validateFields()
          } catch {
            return
          }
          saveMutation.mutate(v)
        }}
        confirmLoading={saveMutation.isPending}
        onCancel={() => setSaveOpen(false)}
        destroyOnClose
      >
        <Form form={saveForm} layout="vertical" preserve={false} initialValues={{ visibility: 'private' }}>
          <Form.Item
            name="name"
            label="报表名称"
            rules={[
              { required: true, message: '请填写报表名称' },
              { whitespace: true, message: '名称不能全为空格' },
            ]}
          >
            <Input placeholder="如：本月销售业绩" maxLength={128} />
          </Form.Item>
          <Form.Item name="visibility" label="可见性" extra="private 仅自己；role/org 对持 report.read 者可见（片A 简化）">
            <Radio.Group
              options={[
                { value: 'private', label: '私有' },
                { value: 'role', label: '按角色' },
                { value: 'org', label: '按组织' },
              ]}
            />
          </Form.Item>
        </Form>
      </Modal>

      <Drawer
        title="已存报表"
        open={savedOpen}
        onClose={() => setSavedOpen(false)}
        width={420}
      >
        {savedQuery.isLoading ? (
          <div style={{ textAlign: 'center', padding: 48 }}>
            <Spin />
          </div>
        ) : savedQuery.isError ? (
          <Alert
            type="error"
            showIcon
            message="加载失败"
            description={
              (savedQuery.error as { localizedMessage?: string })?.localizedMessage ?? '请稍后重试'
            }
          />
        ) : (savedQuery.data?.length ?? 0) === 0 ? (
          <Empty description="暂无已存报表" />
        ) : (
          <List
            dataSource={savedQuery.data ?? []}
            renderItem={(r) => (
              <List.Item
                key={r.id}
                actions={[
                  <Button
                    key="load"
                    type="link"
                    disabled={deleteMutation.isPending}
                    onClick={() => loadReport(r)}
                  >
                    载入
                  </Button>,
                  <Popconfirm
                    key="del"
                    title="确认删除该报表？"
                    onConfirm={() => deleteMutation.mutate(r.id)}
                    okButtonProps={{ loading: deleteMutation.isPending }}
                  >
                    <Button
                      type="link"
                      danger
                      loading={deleteMutation.isPending && deleteMutation.variables === r.id}
                      disabled={deleteMutation.isPending && deleteMutation.variables !== r.id}
                    >
                      删除
                    </Button>
                  </Popconfirm>,
                ]}
              >
                <List.Item.Meta
                  title={r.name}
                  description={
                    <Space size="small">
                      <Tag>{datasets.find((d) => d.id === r.dataset_id)?.name ?? `数据集 #${r.dataset_id}`}</Tag>
                      <Tag>{r.config.chart_type}</Tag>
                      <Typography.Text type="secondary">{r.visibility}</Typography.Text>
                    </Space>
                  }
                />
              </List.Item>
            )}
          />
        )}
      </Drawer>
    </div>
  )
}
