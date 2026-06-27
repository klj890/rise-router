import type { ResourceDef, FieldOption } from './CrudPage'
import {
  loadModelOptions,
  loadChannelOptions,
  loadGroupOptions,
  loadOrgOptions,
  testChannel,
} from '../../api/admin'

// —— 枚举选项（value 用后端 serde 变体名 / 词表值）——
const opt = (v: string, label = v): FieldOption => ({ label, value: v })

const CHANNEL_STATUS = [opt('Enabled', '启用'), opt('Disabled', '禁用'), opt('CircuitBroken', '熔断')]
const MODEL_STATUS = [opt('Listed', '上架'), opt('Delisted', '下架')]
const MODALITY = ['chat', 'embedding', 'image', 'video', 'audio', 'rerank'].map((v) => opt(v))
const INVOCATION = [opt('sync_stream', '同步流 sync_stream'), opt('async_task', '异步任务 async_task')]
const BILLING_UNIT = ['token', 'image', 'second', 'call'].map((v) => opt(v))
const PROTOCOL = [opt('openai_compatible'), opt('anthropic'), opt('gemini')]
// org 暂不开放：解析器尚不处理 org 维度折扣（详见后端 discount.rs KNOWN_SCOPES）
const SCOPE = ['global', 'model', 'group', 'model_group'].map((v) => opt(v))
const KIND = [opt('percentage', '按比例 percentage'), opt('fixed', '减额 fixed')]
const ORG_TYPE = [opt('Individual', '个人'), opt('Enterprise', '企业')]
const ORG_STATUS = [opt('Active', '活跃'), opt('Suspended', '停用')]
const REALNAME = [
  opt('Unverified', '未认证'),
  opt('IndividualVerified', '个人已认证'),
  opt('EnterpriseVerified', '企业已认证'),
]
const KEY_STATUS = [opt('Enabled', '启用'), opt('Disabled', '禁用')]

const channels: ResourceDef = {
  base: '/api/gateway/channels',
  fields: [
    { name: 'name', label: '名称', type: 'text', required: true, inTable: true },
    { name: 'protocol_adapter', label: '协议族', type: 'select', options: PROTOCOL, required: true, inTable: true },
    { name: 'base_url', label: '上游地址', type: 'text', required: true, inTable: true, placeholder: 'https://...' },
    { name: 'credentials', label: '凭据(JSON)', type: 'json', placeholder: '{"key":"sk-..."}', help: '密钥配置；列表不回显' },
    { name: 'has_credentials', label: '已配密钥', type: 'switch', inTable: true, inCreate: false, inEdit: false },
    { name: 'adapter_config', label: '适配器配置(JSON)', type: 'json', help: 'anthropic: {"anthropic_version","default_max_tokens"}；gemini: {"path_template"}；openai 可留空' },
    { name: 'priority', label: '优先级', type: 'number', inTable: true },
    { name: 'weight', label: '权重', type: 'number', inTable: true },
    { name: 'status', label: '状态', type: 'select', options: CHANNEL_STATUS, inTable: true },
    // 健康管理
    { name: 'test_model', label: '测试模型', type: 'text', help: '渠道测试默认上游模型；留空取首条路由' },
    { name: 'auto_ban', label: '允许自动禁用', type: 'switch', inTable: true, help: '失败/超时时是否自动熔断（留空默认开）' },
    { name: 'response_time', label: '测速(ms)', type: 'number', inTable: true, inCreate: false, inEdit: false },
    { name: 'test_time', label: '最近测试', type: 'datetime', inTable: true, inCreate: false, inEdit: false },
    { name: 'disabled_reason', label: '熔断原因', type: 'text', inTable: true, inCreate: false, inEdit: false },
  ],
  rowActions: [{ label: '测试', run: testChannel }],
}

const models: ResourceDef = {
  base: '/api/gateway/models',
  fields: [
    { name: 'slug', label: 'slug', type: 'text', required: true, inTable: true },
    { name: 'display_name_i18n', label: '显示名(JSON)', type: 'json', required: true, placeholder: '{"zh-CN":"通义千问","en-US":"Qwen"}' },
    { name: 'modality', label: '模态', type: 'select', options: MODALITY, required: true, inTable: true },
    { name: 'invocation', label: '调用模式', type: 'select', options: INVOCATION, required: true, inTable: true },
    { name: 'billing_unit', label: '计费量纲', type: 'select', options: BILLING_UNIT, required: true, inTable: true },
    { name: 'capabilities', label: '能力标签(JSON)', type: 'json' },
    { name: 'status', label: '状态', type: 'select', options: MODEL_STATUS, inTable: true },
  ],
}

const modelChannels: ResourceDef = {
  base: '/api/gateway/model-channels',
  fields: [
    { name: 'model_id', label: '模型', type: 'select', optionsLoader: loadModelOptions, required: true, inEdit: false, inTable: true },
    { name: 'channel_id', label: '渠道', type: 'select', optionsLoader: loadChannelOptions, required: true, inEdit: false, inTable: true },
    { name: 'upstream_model_name', label: '上游模型名', type: 'text', required: true, inTable: true },
    { name: 'enabled', label: '启用', type: 'switch', inTable: true },
    { name: 'priority', label: '优先级', type: 'number', inTable: true, help: '留空=继承渠道' },
    { name: 'weight', label: '权重', type: 'number', inTable: true, help: '留空=继承渠道' },
    { name: 'cost_price', label: '成本价(JSON)', type: 'json' },
  ],
}

const groups: ResourceDef = {
  base: '/api/pricing/groups',
  fields: [
    { name: 'slug', label: 'slug', type: 'text', required: true, inTable: true },
    { name: 'name', label: '名称', type: 'text', required: true, inTable: true },
    { name: 'description', label: '描述', type: 'textarea', inTable: true },
  ],
}

const prices: ResourceDef = {
  base: '/api/pricing/prices',
  fields: [
    { name: 'model_id', label: '模型', type: 'select', optionsLoader: loadModelOptions, required: true, inEdit: false, inTable: true },
    { name: 'group_id', label: '分组', type: 'select', optionsLoader: loadGroupOptions, inEdit: false, inTable: true, help: '留空=默认价' },
    { name: 'billing_unit', label: '计费量纲', type: 'text', inCreate: false, inEdit: false, inTable: true },
    { name: 'currency', label: '币种', type: 'text', inTable: true, placeholder: 'CNY' },
    { name: 'unit_prices', label: '单价(JSON)', type: 'json', required: true, placeholder: '{"input":1.5,"output":6.0}' },
    { name: 'valid_from', label: '生效时间', type: 'datetime', inTable: true },
    { name: 'valid_to', label: '失效时间', type: 'datetime', inTable: true },
    { name: 'version', label: '版本', type: 'number', inCreate: false, inEdit: false, inTable: true },
  ],
}

const discounts: ResourceDef = {
  base: '/api/pricing/discounts',
  fields: [
    { name: 'name', label: '名称', type: 'text', required: true, inTable: true },
    { name: 'scope', label: '作用域', type: 'select', options: SCOPE, required: true, inEdit: false, inTable: true },
    { name: 'kind', label: '类型', type: 'select', options: KIND, required: true, inEdit: false, inTable: true },
    { name: 'value', label: '值', type: 'number', required: true, inTable: true, help: 'percentage 因子∈(0,1]（如 0.9）；fixed 减额>0' },
    { name: 'target_model_id', label: '目标模型', type: 'select', optionsLoader: loadModelOptions, inEdit: false, help: 'scope=model/model_group 必填' },
    { name: 'target_group_id', label: '目标分组', type: 'select', optionsLoader: loadGroupOptions, inEdit: false, help: 'scope=group/model_group 必填' },
    { name: 'stackable', label: '可叠加', type: 'switch', inTable: true },
    { name: 'priority', label: '优先级', type: 'number', inTable: true },
    { name: 'valid_from', label: '生效时间', type: 'datetime' },
    { name: 'valid_to', label: '失效时间', type: 'datetime' },
  ],
}

const organizations: ResourceDef = {
  base: '/api/identity/organizations',
  fields: [
    { name: 'name', label: '名称', type: 'text', required: true, inTable: true },
    { name: 'org_type', label: '类型', type: 'select', options: ORG_TYPE, required: true, inTable: true },
    { name: 'group_id', label: '商业分组', type: 'select', optionsLoader: loadGroupOptions, inTable: true, help: '留空=默认价' },
    { name: 'status', label: '状态', type: 'select', options: ORG_STATUS, inTable: true },
    { name: 'realname_status', label: '实名', type: 'select', options: REALNAME, inTable: true },
    { name: 'owner_sales_id', label: '归属销售ID', type: 'number', inTable: true },
  ],
}

const apiKeys: ResourceDef = {
  base: '/api/identity/api-keys',
  secret: { field: 'key', entityField: 'api_key', label: '新密钥（明文仅此一次）' },
  fields: [
    { name: 'org_id', label: '组织', type: 'select', optionsLoader: loadOrgOptions, required: true, inEdit: false, inTable: true },
    { name: 'name', label: '名称', type: 'text', required: true, inTable: true },
    { name: 'allowed_models', label: '模型白名单(JSON)', type: 'json', placeholder: '["gpt-4o","qwen-max"]', help: '模型 slug 数组；留空=不限' },
    { name: 'budget_limit', label: '预算上限(元)', type: 'number', inTable: true, help: '留空=不限' },
    { name: 'expires_at', label: '过期时间', type: 'datetime', inTable: true, help: '留空=永不过期' },
    { name: 'budget_used', label: '已用预算', type: 'number', inCreate: false, inEdit: false, inTable: true },
    { name: 'status', label: '状态', type: 'select', options: KEY_STATUS, inCreate: false, inTable: true },
  ],
}

export interface AdminResourceEntry {
  /** 路由段 + i18n key */
  key: string
  title: string
  /** 所属分区：gateway / pricing / identity */
  section: 'gateway' | 'pricing' | 'identity'
  def: ResourceDef
}

export const ADMIN_RESOURCES: AdminResourceEntry[] = [
  { key: 'channels', title: '渠道', section: 'gateway', def: channels },
  { key: 'models', title: '模型', section: 'gateway', def: models },
  { key: 'model-channels', title: '路由线', section: 'gateway', def: modelChannels },
  { key: 'groups', title: '分组', section: 'pricing', def: groups },
  { key: 'prices', title: '价格', section: 'pricing', def: prices },
  { key: 'discounts', title: '折扣', section: 'pricing', def: discounts },
  { key: 'organizations', title: '组织', section: 'identity', def: organizations },
  { key: 'api-keys', title: '密钥', section: 'identity', def: apiKeys },
]

// 单独导出供 bespoke 页（渠道详情抽屉、密钥预算条）复用同一份字段定义。
export const RESOURCE = { channels, models, modelChannels, groups, prices, discounts, organizations, apiKeys }
