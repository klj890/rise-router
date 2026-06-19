import { api } from './client'

/**
 * CRM 与销售域 API（M3 片D）。对接后端 `crates/crm` 端点。
 *
 * 数据域隔离在后端 `require_scoped`：销售（crm.write 无 crm.read.all）仅见/操作自己名下客户，
 * 越域返回 404（不泄露存在性）。前端无需也无法绕过——本层只做类型化封装。
 *
 * 注：金额字段（balance/amount...）后端用 rust_decimal 无 serde-float，序列化为**字符串**（如
 * "100.00000000"）；枚举（org_type/status/realname_status/order status）序列化为**变体名字符串**。
 */

export type OrgType = 'Individual' | 'Enterprise'
export type OrgStatus = 'Active' | 'Suspended'
export type RealnameStatus = 'Unverified' | 'IndividualVerified' | 'EnterpriseVerified'
export type OrderStatus = 'Pending' | 'Paid' | 'Failed' | 'Refunded'

/** 客户档案视图：组织字段（flatten）+ 钱包余额/授信/冻结快照。 */
export interface Customer {
  id: number
  name: string
  org_type: OrgType
  group_id: number | null
  status: OrgStatus
  realname_status: RealnameStatus
  owner_sales_id: number | null
  balance: string
  credit_limit: string
  frozen: string
}

export interface CustomerNote {
  id: number
  org_id: number
  author_id: number | null
  content: string
  created_at: string
}

export interface Assignment {
  id: number
  org_id: number
  sales_id: number
  assigned_at: string
  active: boolean
}

export interface Order {
  id: number
  org_id: number
  created_by_sales_id: number | null
  amount: string
  pay_channel: string
  trade_no: string | null
  status: OrderStatus
  memo: string | null
  created_at: string
  paid_at: string | null
}

export interface ListCustomersParams {
  /** 仅全量访问者（管理员/财务/超管令牌）生效；受限销售强制本人名下，忽略此值 */
  owner_sales_id?: number
  limit?: number
  /** 游标：上一页最后一条 id；返回 id > cursor（id 升序） */
  cursor?: number
}

export async function listCustomers(params: ListCustomersParams = {}): Promise<Customer[]> {
  const { data } = await api.get<Customer[]>('/api/crm/customers', { params })
  return data
}

export async function getCustomer(orgId: number): Promise<Customer> {
  const { data } = await api.get<Customer>(`/api/crm/customers/${orgId}`)
  return data
}

export interface OnboardReq {
  phone: string
  name: string
  org_type?: OrgType
  nickname?: string
  /** 仅全量访问者可指定；销售本人忽略此字段强制归己 */
  owner_sales_id?: number
}

export interface OnboardResp {
  org: Customer
  user_id: number
  owner_sales_id: number
}

export async function onboardCustomer(req: OnboardReq): Promise<OnboardResp> {
  const { data } = await api.post<OnboardResp>('/api/crm/customers', req)
  return data
}

export async function listNotes(
  orgId: number,
  params: { limit?: number; cursor?: number } = {},
): Promise<CustomerNote[]> {
  const { data } = await api.get<CustomerNote[]>(`/api/crm/customers/${orgId}/notes`, { params })
  return data
}

export async function createNote(orgId: number, content: string): Promise<CustomerNote> {
  const { data } = await api.post<CustomerNote>(`/api/crm/customers/${orgId}/notes`, { content })
  return data
}

export async function listAssignments(orgId: number): Promise<Assignment[]> {
  const { data } = await api.get<Assignment[]>(`/api/crm/customers/${orgId}/assignments`)
  return data
}

/** 改派客户归属（crm.assign，管理员级）；返回更新后的组织。 */
export async function assignCustomer(orgId: number, salesId: number): Promise<Customer> {
  const { data } = await api.post<Customer>(`/api/crm/customers/${orgId}/assign`, {
    sales_id: salesId,
  })
  return data
}

export interface RechargeReq {
  amount: string
  pay_channel?: string
  memo?: string
}

export interface RechargeResp {
  order: Order
  balance: string
}

export async function rechargeCustomer(orgId: number, req: RechargeReq): Promise<RechargeResp> {
  const { data } = await api.post<RechargeResp>(`/api/crm/customers/${orgId}/recharge`, req)
  return data
}
