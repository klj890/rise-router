import { api } from './client'

/**
 * 监控报表域 API（M4 片C）。对接后端 `crates/report`：策展数据集 + RLS 查询引擎 + 报表定义。
 *
 * 安全模型在后端：数据集列表/详情按主体权限点过滤（看不到无权数据集）；查询走 RLS 引擎，
 * 按当前用户角色强制注入行级过滤（客户仅本组织、销售仅本人名下…），前端无法绕过。
 */

export interface MetricDef {
  key: string
  label: string
}
export type DimensionDef = MetricDef

export interface Dataset {
  id: number
  slug: string
  name: string
  source: string
  metrics: MetricDef[]
  dimensions: DimensionDef[]
  rls_rule: Record<string, unknown>
  required_permission: string
  created_at: string
}

export interface QueryReq {
  metrics: string[]
  dimensions?: string[]
  /** RFC3339，含 */
  from?: string
  /** RFC3339，不含 */
  to?: string
  limit?: number
}

/** 查询结果一行：维度值为字符串，指标值为数值（后端 ::float8）。 */
export type ResultRow = Record<string, string | number | null>

export interface QueryResp {
  dataset: string
  /** 实际生效角色 */
  role: string
  /** 是否注入了行级过滤（true=你看到的是经隔离的数据子集） */
  rls_filtered: boolean
  dimensions: string[]
  metrics: string[]
  rows: ResultRow[]
}

export type ChartType = 'table' | 'bar' | 'line'

/** 报表定义 config（存 report_definitions.config jsonb）。 */
export interface ReportConfig {
  metrics: string[]
  dimensions: string[]
  from?: string
  to?: string
  limit?: number
  chart_type: ChartType
}

export interface ReportDefinition {
  id: number
  dataset_id: number
  name: string
  owner_user_id: number | null
  visibility: string
  config: ReportConfig
  created_at: string
}

export interface CreateReportReq {
  dataset_slug: string
  name: string
  visibility?: 'private' | 'role' | 'org'
  config: ReportConfig
}

export async function listDatasets(): Promise<Dataset[]> {
  const { data } = await api.get<Dataset[]>('/api/report/datasets')
  return data
}

export async function getDataset(slug: string): Promise<Dataset> {
  const { data } = await api.get<Dataset>(`/api/report/datasets/${slug}`)
  return data
}

export async function queryDataset(slug: string, req: QueryReq): Promise<QueryResp> {
  const { data } = await api.post<QueryResp>(`/api/report/datasets/${slug}/query`, req)
  return data
}

export async function listReports(): Promise<ReportDefinition[]> {
  const { data } = await api.get<ReportDefinition[]>('/api/report/reports')
  return data
}

export async function createReport(req: CreateReportReq): Promise<ReportDefinition> {
  const { data } = await api.post<ReportDefinition>('/api/report/reports', req)
  return data
}

export async function deleteReport(id: number): Promise<void> {
  await api.delete(`/api/report/reports/${id}`)
}
