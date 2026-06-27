import { api } from './client'

/** 管理 API 辅助：FK 下拉 option 加载器 + 价格预览。CRUD 主体由 CrudPage 直接走 `api`。 */

interface IdRow {
  id: number
  [k: string]: unknown
}

async function optionsFrom(
  base: string,
  labelKey: string,
): Promise<{ label: string; value: number }[]> {
  const { data } = await api.get<IdRow[]>(base)
  return data.map((r) => ({
    label: `${(r[labelKey] as string) ?? r.id} (#${r.id})`,
    value: r.id,
  }))
}

export const loadModelOptions = () => optionsFrom('/api/gateway/models', 'slug')
export const loadChannelOptions = () => optionsFrom('/api/gateway/channels', 'name')
export const loadGroupOptions = () => optionsFrom('/api/pricing/groups', 'slug')
export const loadOrgOptions = () => optionsFrom('/api/identity/organizations', 'name')

/** 渠道连通性测试：真打上游一次，返回 {ok,status,latency_ms,model,error?,usage?}。 */
export interface ChannelTestResult {
  ok: boolean
  status: number
  latency_ms: number
  model: string
  error?: string
  usage?: unknown
}
export const testChannel = (id: number): Promise<ChannelTestResult> =>
  api.post<ChannelTestResult>(`/api/gateway/channels/${id}/test`, {}).then((r) => r.data)

export interface PricePreview {
  model_id: number
  model_slug: string
  group_slug: string | null
  billing_unit: string
  currency: string
  base_unit_prices: unknown
  final_unit_prices: unknown
  discount_factor: number
  applied_discounts: {
    id: number
    name: string
    kind: string
    value: number
    applied: boolean
  }[]
  price_version: number
}

export async function pricePreview(model: string, group?: string): Promise<PricePreview> {
  const { data } = await api.get<PricePreview>('/api/pricing/preview', {
    params: { model, group: group?.trim() || undefined },
  })
  return data
}
