# Rise Router 系统架构

> 版本：v0.1 · 2026-06-13 · 配套文档：[data-model.md](./data-model.md)（数据模型）、[roadmap.md](./roadmap.md)（功能与里程碑）

## 1. 定位

Rise Router 是对开源 new-api 的企业级（to-B）重写：摒弃 to-C 概念，保留核心网关能力，把 new-api 散落在 `options` 表 JSON 大字段、靠 `ModelRatio × GroupRatio × CompletionRatio` 链式相乘的繁琐倍率体系，替换为**定价五要素（模型/渠道/价格/分组/折扣）完全解耦**的关系模型。

系统定位为**微内核平台**：核心提供用户/权限/路由/计费四大基座，业务能力（含内部模块与第三方系统）以 App 形式可插拔接入。

## 2. 架构总览

```
┌──────── 前端 Shell (Vite + React 19 + AntD 6, Module Federation Host) ────────┐
│  登录/布局/导航/主题 ← App Manifest 驱动菜单与远程模块加载                      │
│  [定价管理] [财务] [CRM] [报表] [客服] … 内部插件   |   [第三方前端] iframe/MF  │
└───────────────────────────────┬───────────────────────────────────────────────┘
                                │ HTTPS (OpenAI兼容 / /v1/tasks / 管理API / OIDC)
┌──────────────── 后端核心 (Rust/Axum 单二进制, 模块化单体) ─────────────────────┐
│ 中间件链:  认证 → 两层白名单 → RBAC鉴权 → 限流 → 计费预扣 → 审计                │
│ ┌─────────┬─────────┬──────────┬──────────┬─────────┬────────┬──────────────┐ │
│ │identity │  rbac   │ gateway  │ pricing  │ billing │  task  │   report     │ │
│ │用户/组织 │角色权限  │渠道/模型  │价格/折扣  │钱包/流水 │异步任务 │语义层/RLS     │ │
│ └─────────┴─────────┴──────────┴──────────┴─────────┴────────┴──────────────┘ │
│ App Registry (manifest→permissions/routes/menus 幂等落地)                       │
└──────┬───────────────────────────┬──────────────────────┬──────────────────────┘
   PostgreSQL(+只读副本)        Redis(会话/限流/队列)    对象存储(MinIO/OSS, artifacts)
        │                                                          │
   上游厂商(协议族适配: openai兼容/anthropic/gemini/task_*)   第三方系统(独立进程,OIDC接入)
```

## 3. 后端结构（Cargo workspace，按域拆 crate）

crate 切分与[数据模型十大域](./data-model.md)对齐：

```
backend/
  crates/
    core/      微内核: 配置/DB池/Redis/错误/中间件框架/App注册表/OIDC Provider
    identity/  organizations, users, groups, user_identities, 实名认证
    rbac/      roles, permissions, role_permissions, user_roles, enforce()
    gateway/   channels, models, model_channels; adapters/{openai_compatible,anthropic,gemini,task_*}; 路由解析; relay 转发
    pricing/   prices, discounts; resolve_price() 纯函数(查表+折扣叠加)
    billing/   wallets, transactions, usage_logs, orders, invoices; 预扣/结算
    task/      tasks, artifacts; 任务状态机; 轮询+webhook; 对象存储抽象
    report/    datasets, report_definitions; RLS 查询引擎
    crm/       customer_notes, customer_assignments
    support/   tickets, ticket_messages
    server/    axum 装配, 路由挂载, main.rs(单二进制)
  migration/   SeaORM migration（建表，对应 data-model.md 十大域）
```

## 4. 前端结构

```
frontend/
  shell/         Vite+React19+AntD; MF Host; 登录/布局/主题/路由; 按 manifest 拉菜单加载远程模块
  plugins/       pricing-admin, billing, crm, report, support …（MF remote, 共享 AntD/主题 token）
  packages/      shared-ui, api-client(TanStack Query), auth-sdk
```

前端可插拔 = Module Federation 为主（React 插件共享 AntD/主题 token，原生级一致）+ iframe 兜底（异构技术栈第三方）。App Manifest 中 `frontend.entry.type: module | iframe` 二选一。

### 4.1 设计系统与可配置主题

视觉风格 = **克制专业（Restrained Professional，Linear/Vercel 一派）**：低饱和、细边框、极简阴影、强调色克制使用。**暗色优先 + 浅色可切**；signature 主色 **极光青绿 `#2EE6C0`**（浅色加深为 `#0FB89A`）。字体自托管（`@fontsource/inter` + `@fontsource/jetbrains-mono`，不依赖 Google Fonts CDN）；数据列用等宽 + tabular-nums。

主题入口在 `frontend/shell/src/theme/`：
- `tokens.ts` —— 单一 token 源（暗/浅中性色阶、功能色、字体、圆角）。
- `presets.ts` —— 强调色预设（aurora/violet/amber/blue）+ `buildAntdTheme(mode, accent, override)` 合成 AntD `{algorithm, token, components}`（含 Card/Menu/Button/Table 克制质感覆盖）。
- `store.ts` —— Zustand+persist：`mode`(dark/light/system)、`accentId`、`brand`(白标覆盖)。
- `ThemeProvider.tsx` + `applyCssVars.ts` —— 应用 AntD 主题并把 token 写成 `--rr-*` CSS 变量到 `:root`。
- `branding.ts` —— `loadBranding()` 从 `/api/branding` 拉 per-租户白标（后端未就绪静默回落）。

**可配置 = 预设切换 + 白标覆盖**：内置 4 强调色 × 暗浅 = 8 组合，Header 一键切换；「外观设置」页（`/settings/appearance`）支持白标（主色/logo/应用名/圆角）实时换肤、localStorage 持久化，并预留后端下发。

**CSS 变量契约**（供 MF 第三方插件继承主题）：`--rr-color-primary`、`--rr-bg-layout`、`--rr-bg-container`、`--rr-bg-elevated`、`--rr-border`、`--rr-text`/`-secondary`/`-tertiary`、`--rr-fill`、`--rr-font-sans`/`-mono`；`:root[data-theme=dark|light]` 标记当前模式。

## 5. 关键技术组件

| 关注点 | 选型 |
|---|---|
| Web 框架 / 异步 | Axum + Tokio + Tower（中间件洋葱链） |
| ORM / 迁移 | SeaORM 2.0 + sea-orm-migration；热路径下钻 SQLx |
| 鉴权 | JWT + 平台自身 OIDC Provider；密钥仅存哈希 |
| 缓存/队列/限流 | Redis（会话、限流计数、任务队列） |
| 对象存储 | S3 兼容抽象（私有化 MinIO / 云 OSS·COS 可插拔） |
| 配置 | 分层配置 + 环境变量覆盖；密钥字段加密落库 |
| 可观测 | tracing + OpenTelemetry；usage_logs 既是计费又是监控数据源 |
| 部署 | 后端单二进制（前端静态产物可由其托管）；第三方为独立进程 |

## 6. 网关中间件链（洋葱模型）

借鉴 `pluto_oauth2` 与 `zxzq-app-market` 的认证/鉴权设计，请求流经：

```
请求
 → ⓪ Locale 协商(LocaleLayer): URL?lang > 用户偏好 > 组织默认 > Accept-Language > zh-CN，注入 request 上下文
 → ① 认证: Bearer token 校验 + userinfo 反查缓存(sha256 key, 2min TTL, singleflight 去重, 401/503 区分)
 → ② 两层白名单短路: 免认证(健康检查/公开API) | 免授权(仅校验 token, 跳过权限决策)
 → ③ RBAC: enforce(sub, dom, obj, act)，dom=组织/工作空间 → 天然多租户数据隔离
 → ④ 限流: 渠道级 + 密钥级 (Redis 计数)
 → ⑤ 计费预扣: 钱包余额冻结 (命中预算上限返回 429)
 → 业务处理 / relay 转发
 → ⑥ 结算 + 审计: usage_logs 结算, audit_logs 记录 HTTP 级调用
```

### 6.1 国际化（横切）

全栈 i18n（zh-CN 默认 + en-US，可扩展），四套解耦子系统：UI 文案（i18next 命名空间）/ 内容数据（JSONB `*_i18n`）/ API 错误（code+参数，后端不绑定语言）/ 格式化（`Intl`/CLDR）。可插拔：App Manifest `i18n` 块声明命名空间/错误码/模板，内部域与第三方同套注册（狗粮原则）。完整设计见 **[i18n.md](./i18n.md)**。

## 7. 部署拓扑

- **后端**：单二进制（模块化单体），前端静态产物可由其托管；横向扩展靠多实例 + Redis 共享状态。
- **数据**：PostgreSQL 主库 + 只读副本（报表读路径走副本，避免压主库）；Redis 会话/限流/队列；S3 兼容对象存储放 artifacts。
- **第三方系统**：独立进程，经 OIDC SSO + App Manifest + 网关路由挂载接入，任意语言可接。
- **私有化**：MinIO + 自托管 PG/Redis；云上可换 OSS/COS + 云数据库。

## 8. 本地兄弟项目借鉴清单

工作区已有多个高相关项目可借鉴。**`billing-report/agent-console` 是与本项目技术栈完全一致（Rust/axum + React/AntD/Zustand）的参考实现**，作为 M1–M2 的首要范本研读。

| 借鉴项 | 来源 | 应用 |
|---|---|---|
| **参考实现：多角色财务控制台** | `billing-report/agent-console/`（Rust axum + React） | M1–M2 直接范本：DB schema、API 路由形态、多级缓存、对账、xlsx 导出、邮件 cron |
| 多级缓存 + TTL | `agent-console/src/state.rs`（5min TTL, 主动失效） | 价格表/路由表/会话缓存；解决"改价不刷新"踩坑 |
| 上游客户端弹性 | `agent-console/src/newapi_client.rs`（粘性代理+超时+fallback+退避） | gateway relay 转发层容错 |
| 对账系统 | `agent-console/src/billing/reconcile.rs` + `billing-report/april_reconcile_report.py` | 月度对账：应收 vs 实收 gap、对账单锁定后编辑 |
| 账单报表引擎（7 章节） | `billing-report/billing_report.py` | 报表域：总览/模型/用户/Key/日趋势/渠道/错误；JSON 层与渲染层分离 |
| 错误责任三分类 | `billing_report.py::classify_error`（客户/上游/系统） | `usage_logs` 错误分类字段，供报表与告警 |
| OAuth2 授权码流 + OIDC | `pluto_oauth2/src/routes/oauth2.js` | 平台 OIDC Provider 端点实现范本 |
| `enforce(sub,dom,obj,act)` | `pluto_oauth2/src/services/plutoAuthService.js` | RBAC 决策函数；dom=组织 → 多租户隔离 |
| 两层白名单 | `pluto_oauth2`（免认证 + 免授权） | 网关中间件短路逻辑 |
| Bearer + userinfo 反查缓存 | `zxzq-app-market-api/internal/auth/userinfo.go` | 认证中间件 token 校验与去重 |
| 工单状态机 + 幂等 + 行锁 | `apikey_approval/`（7 态、ticket_no 幂等、行锁、过期 cron、差量推送） | 订单/审批/密钥工作流 |
| append-only 事件审计 | `apikey_approval/application_events` + `agent-console/audit_log` | 全域审计：HTTP 级 + 业务事件流 |
| 三档定价策略与踩坑 | `riseapi-ops/docs/pricing-strategy.md` | **反面教材**：Token group 覆盖 User group、缓存 TTL；本设计用五要素解耦+流水快照 group 规避 |
| 前端页面范本 | `openrouter-china/rise-ai-cloud-demo`（Dashboard/Usage/ApiKeys/套餐/demo 账号） | 控制台页面、demo 账号一键登录、用量看板 |
| BFF 报表聚合 + 嵌入模式 | `hx_report`（多上游统一客户端、`?embed=1`、CSV/Excel 导出） | 报表 BFF；iframe 嵌入第三方 |
| 三层参数合并 | `app-market`（Chart Default → Admin Default → User Input） | 模型调用参数治理：厂商默认 → 管理员上限 → 用户输入 |
| CAMP 设计规范 | `approve_demo` / `hx-camp-operation` | 前端 Shell 主题与企业视觉基线 |

## 9. 核心架构原则（贯穿所有实现）

1. **定价五要素解耦不可破**：模型/渠道/价格/分组/折扣互不依赖；价格解析（`resolve_price()`）与路由解析各为独立纯函数，管理台「价格预览」与网关热路径复用同一实现（所见即所得）。
2. **狗粮原则**：内部模块（定价/财务/CRM/报表/客服）必须与第三方走同一套 App 注册标准。
3. **避免过度设计**：报表数据先单库 PG + 只读副本，量级上去再引入 TimescaleDB/OLAP；不提前造二级设施；不做通用 JSON 映射 DSL。
