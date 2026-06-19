import type { OrgType, OrgStatus, RealnameStatus, OrderStatus } from '../../api/crm'

/** CRM 枚举的中文展示映射（后端序列化为变体名字符串）。集中一处，列表/详情复用。 */

export const ORG_TYPE_LABEL: Record<OrgType, string> = {
  Individual: '个人',
  Enterprise: '企业',
}

export const ORG_STATUS_LABEL: Record<OrgStatus, string> = {
  Active: '活跃',
  Suspended: '停用',
}

export const ORG_STATUS_COLOR: Record<OrgStatus, string> = {
  Active: 'green',
  Suspended: 'red',
}

export const REALNAME_LABEL: Record<RealnameStatus, string> = {
  Unverified: '未认证',
  IndividualVerified: '个人已认证',
  EnterpriseVerified: '企业已认证',
}

export const ORDER_STATUS_LABEL: Record<OrderStatus, string> = {
  Pending: '待支付',
  Paid: '已支付',
  Failed: '失败',
  Refunded: '已退款',
}
