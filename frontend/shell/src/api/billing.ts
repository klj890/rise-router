import { api } from './client'

/**
 * 计费 API。
 * - 充值（recharge）：管理动作，`billing.manage`（X-Admin-Token 超管通道或财务/管理员 JWT）。
 * - 财务控制台只读视图（margin / admin-* / reconciliations）：`billing.read`，跨租户聚合。
 *   控制台用户 JWT 若具 finance/admin 角色即可读；否则配管理令牌走超管通道（与渠道/价格 CRUD 同款）。
 */

export interface RechargeResp {
  org_id: number
  balance: string
}

/** 手动充值入账（billing.manage）。 */
export async function recharge(orgId: number, amount: number, memo?: string): Promise<RechargeResp> {
  const { data } = await api.post<RechargeResp>('/api/billing/recharge', {
    org_id: orgId,
    amount,
    memo: memo?.trim() || undefined,
  })
  return data
}

// —— 跨租户只读视图（billing.read）——

export interface MarginCell {
  dim: string | null
  revenue: string
  cost: string
  gross_profit: string
  margin_rate: string | null
  total_calls: number
  cost_covered_calls: number
}
export interface MarginResp {
  period: string
  cost_complete: boolean
  rows: MarginCell[]
}

/** 毛利总览（period=YYYY-MM，缺省当月）。rows[0] 为总览单元格。 */
export async function getMargin(period?: string): Promise<MarginResp> {
  const { data } = await api.get<MarginResp>('/api/billing/margin', { params: { period } })
  return data
}

export interface TenantRow {
  org_id: number
  org_name: string
  calls: number
  charged: string
  balance: string
}

/** 租户用量总览（本周期调用/消费 + 钱包余额，消费倒序）。 */
export async function listTenants(period?: string): Promise<TenantRow[]> {
  const { data } = await api.get<TenantRow[]>('/api/billing/admin/tenants', { params: { period } })
  return data
}

export interface OrderRow {
  id: number
  org_id: number
  org_name: string
  amount: string
  pay_channel: string
  status: string
  memo: string | null
  created_at: string
  paid_at: string | null
}

/** 跨租户充值订单（倒序）。 */
export async function listOrders(limit = 100): Promise<OrderRow[]> {
  const { data } = await api.get<OrderRow[]>('/api/billing/admin/orders', { params: { limit } })
  return data
}

export interface InvoiceRow {
  id: number
  org_id: number
  org_name: string
  invoice_type: string
  title: string
  tax_no: string | null
  amount: string
  status: string
  created_at: string
  issued_at: string | null
}

/** 跨租户发票（倒序）。 */
export async function listInvoices(limit = 100): Promise<InvoiceRow[]> {
  const { data } = await api.get<InvoiceRow[]>('/api/billing/admin/invoices', { params: { limit } })
  return data
}

export interface Reconciliation {
  id: number
  period: string
  status: string
  total_revenue: string
  total_calls: number
  upstream_cost: string | null
  gap: string | null
  generated_at: string
  locked_at: string | null
}

/** 对账单列表（按周期倒序）。 */
export async function listReconciliations(): Promise<Reconciliation[]> {
  const { data } = await api.get<Reconciliation[]>('/api/billing/reconciliations')
  return data
}
