# Rise Router 功能清单与开发路线图

> 版本：v0.1 · 2026-06-13 · 配套文档：[architecture.md](./architecture.md)（系统架构）、[data-model.md](./data-model.md)（数据模型）

## 1. 功能清单（按域，与数据模型十大域一一对应）

**① 身份与组织**：手机号 + 短信验证码注册（国情主通道）、密码/微信登录、组织（企业/个人 org-of-one）、成员管理、实名认证（个人/企业）、会话管理。

**② RBAC 与认证**：角色（customer/sales/finance/ops/admin）、权限点（App 声明注入）、角色授权、平台 OIDC Provider（供第三方 SSO）、数据域 scope。

**③ App 注册（平台可插拔）**：App Manifest（auth/permissions/api_routes/frontend 四要素）、内部模块与第三方统一注册、manifest→派生表幂等落地、虚拟密钥（预算/模型白名单/过期）。

**④ 网关与路由**：协议族适配器、新厂商配置化接入（零代码）、模型能力目录（modality × invocation）、渠道（成本/售价分离/多 key/熔断）、model↔channel 路由表（优先级/权重/故障转移）、OpenAI 兼容入口。

**⑤ 定价（五要素解耦）**：模型目录、渠道成本、显式价格表（元/百万 token · 元/张 · 元/秒 · 元/次，分组维度）、用户分组、独立折扣（按客户/分组/模型/时段，叠加规则可见）、价格预览（所见即所得）、价格版本化。

**⑥ 计费与财务**：组织钱包（余额/授信/冻结）、调用计费流水、资金流水、充值/订阅订单、国内支付（微信/支付宝/对公）、发票（普票/专票）、对账、毛利（售价 − 成本）。

**⑦ 多模态任务**：统一 `/v1/tasks`（提交/查询/取消）、任务状态机、轮询 + webhook 回调、artifacts 对象存储、按量纲计费。

**⑧ CRM 与销售**：客户档案、销售归属与变更历史、销售代客开户/充值、跟进记录、业绩归因（基于流水聚合）。

**⑨ 监控报表**：策展数据集（不开放原始库）、指标/维度、RLS 行级隔离（四角色各见其所关心）、定制报表（JSON 定义 + AntD 渲染）、定时导出/订阅、运维监控（QPS/延迟/渠道健康）。

**⑩ 客服**：工单（创建/分派/状态/优先级）、会话消息、与组织/客户关联。

## 2. 里程碑

> 顺序：MVP（网关+定价）→ 财务计费 → CRM 销售 → 监控报表 → 前端可插拔+客服 → 合规生产化。

### M0 — 工程脚手架（地基）
- **交付**：Cargo workspace + 空 crate 骨架；SeaORM 连接 PG + migration 框架；Axum 启动 + 健康检查 + 中间件链空壳；前端 Shell 起步（登录页 + 布局 + 路由）；docker-compose（PG/Redis/MinIO）；CI（fmt/clippy/test）。
- **目录**：`backend/crates/core`、`backend/crates/server`、`backend/migration`、`frontend/shell`。
- **退出标准**：`cargo run` 起服务命中健康检查；前端能登录到空白控制台；迁移可建/回滚。

### M1 — 网关 + 定价最小闭环（MVP）★
- **交付**：身份（注册/登录/组织/分组）+ RBAC 最小集（`enforce(sub,dom,obj,act)`，借鉴 pluto_oauth2）+ 虚拟密钥；网关（openai_compatible 适配器 + 渠道 + 模型 + 路由表 + OpenAI 兼容入口 + relay 转发 + 上游弹性，借鉴 `newapi_client.rs`）；定价（价格表 + 折扣 + `resolve_price()` 纯函数 + 价格缓存带主动失效）；计费（钱包预扣 + usage_logs 结算 + 流水）；管理台（渠道/模型/价格/分组 CRUD + 价格预览）。
- **参考实现**：研读 `billing-report/agent-console`（同栈 Rust/axum+React），对照其 DB schema、缓存层、API 路由形态。
- **闭环**：注册 → 建渠道 → 配模型 → 配价 → 拿 key → OpenAI SDK 调用 → 扣费 → 看流水。
- **涉及**：`identity`、`rbac`、`gateway`、`pricing`、`billing`、`frontend/plugins/pricing-admin`。
- **退出标准**：用真实/Mock 上游跑通一次计费调用；改价不联动路由/分组/折扣，价格预览 = 实际扣费；用量与余额一致；计费时 group 取流水快照（规避 riseapi-ops 的 Token/User group 优先级踩坑）。

### M2 — 财务与计费（深化）
- **交付**：充值订单（状态机 + 幂等，借鉴 `apikey_approval`）+ 国内支付（微信/支付宝/对公）对接、订阅/套餐、授信后付费、发票、对账系统（借鉴 `reconcile.rs`/`april_reconcile_report.py`：应收 vs 实收 gap、对账单 draft→locked）、毛利报表、xlsx 导出 + 邮件账单 cron（借鉴 agent-console）。
- **涉及**：`billing` 扩展、`frontend/plugins/billing`、新增 `audit_logs`/`*_events`/`reconciliations` 表。
- **退出标准**：完成一笔充值 → 入账 → 消费 → 对账闭环；专票/普票可开；月度对账单 gap 可解释。

### M3 — CRM 与销售
- **交付**：客户档案、销售归属与变更历史、销售代客开户/充值（`orders.created_by_sales_id`）、跟进记录、业绩聚合。
- **涉及**：`crm`、`identity`（销售角色）、`frontend/plugins/crm`。
- **退出标准**：销售可代客户开通并充值；业绩按归属正确归因。

### M4 — 监控报表
- **交付**：策展数据集 + 指标/维度、RLS 引擎、定制报表（定义 + 渲染）、定时导出/订阅、运维监控大盘。报表引擎借鉴 `billing_report.py` 七章节结构与错误三分类、`hx_report` BFF 聚合 + `?embed=1` 嵌入模式。
- **涉及**：`report`、`frontend/plugins/report`、PG 只读副本读路径。
- **退出标准**：四角色各自登录只见其数据域；自定义一张报表并定时导出；报表可 iframe 嵌入。

### M5 — 前端可插拔 + 客服 + 多模态任务
- **交付**：Shell 的 Module Federation 运行时加载 + iframe 兜底打通；第三方 App 经 Manifest 注册并在 Shell 渲染菜单/页面；统一 `/v1/tasks` + 任务引擎 + artifacts；工单/会话客服。
- **涉及**：`core`（App Registry 完整化）、`task`、`support`、`frontend/shell`（MF host）、`frontend/plugins/support`。
- **退出标准**：一个第三方 App（异构栈）经标准接入完成 SSO + 菜单 + 路由 + 计费；一个视频任务从提交到产物落对象存储。

### M6 — 合规与生产化
- **交付**：ICP/公安备案展示、实名认证流程、限流/熔断完善、审计日志、密钥加密轮换、备份、压测、私有化部署包。
- **退出标准**：生产部署清单通过；安全/合规自查通过。

## 3. 持续约束（贯穿所有里程碑）

- **五要素解耦不可破**：任何改动不得让模型/渠道/价格/分组/折扣重新耦合；价格解析与路由解析各自封装为独立纯函数，管理台与热路径复用同一实现。
- **狗粮原则**：内部模块（定价/财务/CRM/报表/客服）必须走与第三方相同的 App 注册标准。
- **避免过度设计**：报表数据先单库 PG + 只读副本，量级上去再引入 TimescaleDB/OLAP；不提前造二级设施；不做通用 JSON 映射 DSL。
- **跨文件/接口签名/数据结构改动**：先列 A/B 方案再动手（继承工作区规范）。
