# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目状态

**M0 脚手架已落地（2026-06-13）**。设计阶段决策见 `docs/`（README/architecture/roadmap/data-model）。技术选型见「技术选型（已确定）」一节。

### 已落地工程结构（M0）

```
rise-router/
  backend/                 Cargo workspace（Rust）
    Cargo.toml             workspace + workspace.dependencies（含 SeaORM 2.0.0-rc.40）
    crates/
      core/                rise-core：config/db/error/state（AppState 注入各域）
      identity rbac gateway pricing billing task report crm support
                           9 个域 crate，各 lib.rs 暴露 routes()，挂到 server 的 /api/<域>
      server/              rise-server：axum 装配 + /healthz + /readyz + 中间件链骨架(trace/cors)
    migration/             SeaORM 迁移（含初始 groups 表，验证 up/down）
  frontend/shell/          Vite + React 19 + AntD 6 + TanStack Query + Zustand + react-router
                           登录页(手机号占位) + AppLayout(十域导航) + Dashboard(探测 /readyz)
    src/theme/             设计系统与可配置主题：tokens/presets/store/ThemeProvider/applyCssVars/branding
                           克制专业风(Linear 派)·暗色优先·极光青绿#2EE6C0·预设切换+白标(外观设置页)
                           详见 docs/architecture.md §4.1；主题入口改动从 src/theme/ 起
  docker-compose.yml       PG(5432) / Redis(6379) / MinIO(9000+9001)
  Makefile                 infra-up / be-run / migrate / fe-dev 等
  .github/workflows/ci.yml fmt+clippy+test（后端）+ pnpm build（前端）
```

### 端口分配（M0）

| 端口 | 用途 |
|---|---|
| 8088 | rise-server 后端（**不用 8080**：微信冲突）|
| 5273 | 前端 Shell dev（避开 openrouter-china 的 5173）|
| 5432 / 6379 | PostgreSQL / Redis |
| 9000 / 9001 | MinIO S3 API / 控制台 |

### 本地启动

```bash
make infra-up            # 起 PG/Redis/MinIO
cd backend && cp ../.env.example .env   # 或 export 环境变量
make migrate             # 建表（需 PG 就绪）
make be-run              # 后端 :8088（无 DB 也能起，/readyz 报 degraded）
make fe-dev              # 前端 :5273
```

### 注意事项 / 踩坑

- **SeaORM 仍是 2.0.0-rc.40**（本环境 crates.io 无 2.0 stable），workspace 已显式锁定 RC；stable 发布后再升。
- 后端 M0 **容忍数据库未连接**：`/healthz` 恒 200，`/readyz` 检查 DB 连通性返回 ready/degraded。
- 迁移 CLI 读 `DATABASE_URL`（标准名），运行期服务读 `RR_DATABASE_URL`——两者都在 `.env.example` 里。

## 项目定位

**Rise Router** — 企业级（to-B）AI API Router 系统，是对开源项目 new-api 的重新设计与重构（不是 fork，不是打补丁）。

new-api 本质上是一个 to-C 系统，其核心痛点是**倍率体系**（ModelRatio × CompletionRatio × GroupRatio 链式相乘）：配置繁琐、语义隐晦、管理员难以回答"这个客户用这个模型到底多少钱一百万 token"。Rise Router 的核心目标就是消灭这套倍率体系。

## 核心设计原则：定价五要素完全解耦

**模型（Model）、渠道（Channel）、价格（Price）、用户分组（Group）、模型折扣（Discount）必须是相互独立的实体**，通过显式关联组合，而非隐式倍率相乘：

- **模型**：纯能力目录（模型名、上下文、能力标签），不携带价格信息。
- **渠道**：纯上游接入（供应商适配、密钥、成本价），渠道成本与售价分离。
- **价格**：独立价格表，以"元/百万 token"等直观单位显式定价（区分 prompt/completion），可按模型 × 分组维度查询出**确定的最终价格**，不需要心算倍率。
- **用户分组**：纯客户分类（套餐档位、企业客户、销售渠道），分组本身不含价格逻辑。
- **折扣**：独立的折扣/优惠实体（按客户、按模型、按时段），叠加规则显式可见。

判断标准：管理员在任意页面应能直接看到"分组 G 的用户调用模型 M 的最终单价"，且修改任一要素不需要联动改其他四个。

## 功能版图

1. **API 路由网关**：多上游渠道接入与转发（参考 new-api 的 relay 适配器设计）、灵活路由配置（按模型/分组/权重/故障转移）。
2. **企业级模型与定价管理**：模型目录、渠道管理、价格表、折扣管理（按上述解耦原则）。
3. **财务系统**：充值、消费流水、对账、发票/合同维度的企业财务需求。
4. **CRM / 销售系统**：客户档案、销售归属、销售开户与售卖（销售可代客户开通/充值）、业绩统计。**用户自主注册与销售售卖两条获客通道并存**。
5. **客服系统**：完整的工单/会话客服能力。
6. **高度可定制前端**：品牌、主题、页面布局支持深度定制（企业私有化部署场景）。

## 平台化与可插拔架构（已确定，2026-06-13 用户拍板）

系统定位为**微内核平台**：核心提供用户/权限/路由/计费四大基座，业务能力以"应用（App）"形式插拔接入。

**核心基座（编译进主二进制的内核）**：
1. 用户体系（账号/组织/用户分组）
2. 认证与权限：平台自身作为 **OIDC Provider**；RBAC 权限系统，权限点由应用注册时声明
3. API 路由系统：数据驱动的网关规则，按应用 manifest 自动挂载路由并套用鉴权/限流/审计
4. 计费/审计

**后端可插拔 = 标准协议接入型**：第三方系统为独立进程，通过 OIDC SSO + App Manifest + 网关路由挂载接入，任意语言可接（只需懂 HTTP/OIDC/JWT），不做进程内 dylib/WASM 插件（钩子级 WASM 扩展留作 v2 备选）。

**前端可插拔 = Module Federation + iframe 兜底**：React 插件构建为 MF remote，Shell 按 manifest 运行时加载，共享 React/AntD/主题 token；异构技术栈第三方走 iframe 入口（manifest 中 `entry.type: module | iframe` 二选一）。

**App Manifest 四要素**：`auth`（OIDC 接入）、`permissions`（权限点声明，注入 RBAC）、`api_routes`（路由前缀 → upstream + 所需权限）、`frontend`（菜单 + 页面入口）。

**狗粮原则**：CRM、客服、财务等内部一等模块必须与第三方走同一套注册标准（声明权限点/路由/菜单），区别仅在于内部模块编译进主二进制、第三方跨进程。接入标准必须被内部模块真实使用，防止退化为文档摆设。

## 多模态网关层设计（已确定，2026-06-13 用户拍板）

目标：快速对接新厂商/新接口；视频、图片等复杂模型一等公民；第三方基于平台快速开发自己的 AI 服务（视频生成、游戏、AI 教育等）。

**协议族适配器 + 配置化厂商接入**：
- 代码只为**协议族**写适配器（Rust trait `ProtocolAdapter`）：OpenAI 兼容（覆盖绝大多数国产厂商）、Anthropic、Gemini、各视频/图片任务式 API 等；不照搬 new-api 每厂商一个适配器目录的做法。
- 新厂商若属已知协议族 = **纯配置接入**（管理界面填 base_url/鉴权/模型映射/限流），零代码零发版；只有全新协议族才写代码。
- 厂商 quirk 在协议族适配器内用配置开关消化；**不做通用 JSON 映射 DSL**（过度设计，被两家厂商真实逼出来再考虑）。

**调用模式抽象 + 统一任务子系统**：
- 模型实体两个维度：`modality`（chat/embedding/image/video/audio/…）× `invocation`（sync-stream / async-task）。
- 异步任务一等公民：任务状态机（排队/运行/成功/失败/取消）、轮询 + webhook 双通道、超时重试策略。
- Artifact 走 S3 兼容接口落对象存储（私有化 MinIO / 云 OSS/COS 可插拔）。
- **计费单位泛化**（价格实体扩展，不破坏五要素解耦）：按百万 token（分 prompt/completion）、按张（分辨率分档）、按秒视频（分辨率/时长分档）、按次调用。

**对外 API 风格**：
- Chat/embedding：**OpenAI 兼容**（事实标准，第三方换 base_url 即用）。
- 视频/图片等任务类：**平台统一任务 API**（`POST /v1/tasks` 提交，`type` 如 `video.generation`，`input` 标准字段 + `extra` 厂商独有参数透传，`webhook` 回调；`GET /v1/tasks/{id}` 返回 status/artifacts/usage）。第三方适配一次，底层换厂商不改代码。原生透传通道暂不做（被真实需求逼出来再加）。

**开发者平台闭环**：第三方 App 既是身份/权限接入方（App Manifest），又是 AI 能力消费方（per-app API Key + 配额 + 用量看板 + webhook），用量账单挂 App 与客户档案，与 CRM/财务打通。

## 监控报表系统（已确定，2026-06-13 用户拍板）

目标：客户、销售、财务、运维四类角色查询各自关心的数据并定制报表。

**策展语义层，不开放原始库**：
- 第一性原则——"查询数据库"绝不落成开放 DB 访问。管理员定义**数据集（Dataset）**=策展过的视图，声明可用**指标（metric）**与**维度（dimension）**；报表只能基于数据集搭建，碰不到原始表。新增可查内容 = 管理员加数据集，零代码。

**同一引擎 + 行级数据隔离（RLS）**：四角色不是四套系统，而是同一报表引擎 + 基于身份的行级过滤，复用核心 RBAC。每个数据集声明行级规则（如 `customer_id = :current_org`），引擎查询时强制注入，不可绕过：
- 客户 → 仅自己组织的用量/账单/调用
- 销售 → 自己名下客户的用量/消费/业绩（与 CRM 归属打通）
- 财务 → 全量营收/对账/成本毛利（渠道成本 vs 售价）
- 运维 → 系统健康/渠道可用性/延迟错误率/任务队列

**报表引擎 = 自建轻量语义层**：Rust 后端暴露策展数据集 + 指标/维度 API，前端 AntD 报表构建器；不嵌入外部 BI（DataEase/Metabase/Superset），避免重资产、SSO/行权打通、主题不一致与过度设计。报表本身是一个内部一等 App（遵循狗粮原则）。

**定制报表 = 报表定义（JSON）+ 渲染**：报表是可保存的定义（数据集 + 指标 + 维度 + 过滤 + 图表类型 + 刷新周期），存库、按角色共享；前端 AntD 图表 + 表格渲染，支持定时导出（Excel/PDF）与 webhook/邮件订阅。

**分析数据架构 = 单库 PG + 只读譻本**：先用 PostgreSQL 同库建分析表，读路径走只读副本避免压主库；监控时序（QPS/延迟/渠道健康）暂存 PG，**量级上去再引入专用时序/OLAP（如 TimescaleDB）**——不提前造二级设施（避免过度设计）。监控与业务报表是两种负载，做架构演进时分开评估。

## 中国国情合规要求（注册与运营）

- 注册以**手机号 + 短信验证码**为主通道（不依赖 GitHub/Discord OAuth 作为主要注册方式，可选微信登录）。
- 预留**实名认证**流程（个人/企业认证）。
- 页面底部 ICP 备案号、公安备案号展示位。
- 短信、支付（微信/支付宝/对公转账）等接入国内服务商。

## 参考代码库（同工作区，只读参考）

- `~/claude_project/new-api/` — new-api 上游源码。**值得借鉴**：`relay/channel/` 40+ 供应商适配器模式、Router→Controller→Service→Model 分层、SQLite/MySQL/PostgreSQL 三库兼容写法（见其 CLAUDE.md Rule 2）、`pkg/billingexpr/` 表达式计费的设计文档。**需要重新设计**：`setting/ratio` 倍率体系、用户/分组模型、to-C 风格的运营页面。
- `~/claude_project/new-api-customize/new-api/` — 此前的定制化副本，含倍率配置 diff 记录（`2_diff_ModelRatio.md` 等），是"倍率体系有多繁琐"的一手痛点证据。

注意：new-api 采用 Apache 2.0 类许可并有品牌保护条款；本项目是重新实现，如复制其代码片段需保留许可证义务，优先借鉴设计而非搬运代码。

## 技术选型（已确定，2026-06-13 用户拍板）

- **重构策略**：完全重写，摒弃 new-api 中 to-C 的概念，仅保留核心功能设计（relay 适配器模式等作只读参考）。
- **后端**：Rust + **Axum**（Tokio/Tower 生态，中间件链：认证 → 限流 → 计费 → 审计）。
- **数据库层**：**SeaORM 2.0** 为主（CRM/财务等重关系 CRUD），计费等热路径可下钻 SQLx 原生 SQL；数据库锁定 **PostgreSQL**（不做 new-api 式三库兼容）。
- **前端**：**Vite + React 19 SPA + Ant Design 6**（ProTable/ProForm 适配重表格表单的企业后台；构建产物可由 Rust 单二进制托管）。对外官网如需 SEO 另起独立站点，不混入控制台。
- **前端配套**：TanStack Query（服务端状态）、Zustand（客户端状态）、TanStack Table（复杂表格补充）。

## 待定决策

1. **模块切分与交付顺序**：倾向网关+定价先行，CRM/财务/客服后续迭代，具体待规划。
2. **端口分配**：选定后登记到 `~/claude_project/CLAUDE.md` 的 Port Allocation 表。

每项决策按工作区惯例给出 A/B 方案对比后由用户拍板，并将结论回写到本文件。

## 项目文档地图（设计已完成，2026-06-13）

动手前先读 `docs/`（互相交叉链接）：
- **`docs/architecture.md`** — 系统架构：微内核总览图、后端 crate 切分、前端 Shell 结构、技术组件、网关中间件链、本地兄弟项目借鉴清单。
- **`docs/roadmap.md`** — 功能清单十域 + 里程碑 M0–M6 + 持续约束（交付顺序：MVP 网关+定价 → 财务 → CRM → 报表 → 可插拔+客服 → 合规）。
- **`docs/data-model.md`** — 数据模型 ER 蓝图（建表前，详见下节）。
- **`docs/i18n.md`** — 国际化架构：全栈 i18n（zh-CN 默认+en-US）；UI 文案/内容数据/API 错误/格式化四套解耦；内容用 JSONB `*_i18n` 字段；API 错误走 code+参数（后端不绑定语言）；locale 协商 LocaleLayer；可插拔译文随 App Manifest。
- **`docs/implementation.md`** — 已实现功能与数据库设计（as-built，含实现架构图/真实表 ER 图/定价解析流程图/主题·i18n 设计图）。改完代码后同步更新此文件，保持 as-built 准确。
- **`docs/README.md`** — 文档索引。

**同栈参考实现**：`~/claude_project/billing-report/agent-console/`（Rust axum + React/AntD/Zustand）已实现多角色财务控制台、对账、多级缓存、xlsx 导出、邮件 cron，是 M1–M2 的首要范本。其他借鉴见 `docs/architecture.md` 第 8 节。

## 核心数据模型（已完成 ER 设计，2026-06-13）

完整设计见 **`docs/data-model.md`**（建表前蓝图，PostgreSQL + SeaORM）。要点：

- **十大域**：身份组织 / RBAC / App 注册 / 网关路由 / 定价 / 计费财务 / 多模态任务 / CRM / 报表 / 客服。建议按域拆 crate，与微内核切分对齐。
- **两条独立分组轴**（勿混）：`roles`（RBAC 能力，挂 user，决定能做什么）vs `groups`（商业定价档位，挂 organization，决定付多少钱）。
- **计费主体是 `organizations`**（钱包/分组挂这里），不是 user；个人自主注册 = 自动建 org-of-one，统一计费模型。`users` 为成员，`api_keys` 为虚拟密钥（借鉴 LiteLLM/Portkey）。
- **定价五要素解耦落表**：`models`（纯能力，无价）×`channels`（纯接入，成本售价分离）经 `model_channels` 连成**路由线**；`models`×`groups` 经 `prices`（显式单价 jsonb，无倍率）+ 独立 `discounts` 连成**定价线**；两线仅在 models 相交、互不依赖。最终价 = `prices.lookup` 一次查表 + `discounts` 显式叠加，管理台「价格预览」与网关热路径复用同一解析函数（所见即所得）。
- **倍率体系已废除**：new-api 把 ModelRatio/GroupRatio/CompletionRatio 存 options 表 JSON 大字段并链式相乘是繁琐根因；本设计改任一要素不联动其余四个。
- **多模态**：`models.modality × models.invocation` 两维度；异步任务 = `tasks`（状态机）+`artifacts`（S3 兼容存储）。
- **报表 RLS**：`datasets` 声明 `rls_rule`（按角色的行级过滤分支），`report_definitions` 只能基于数据集，引擎查询时强制注入。

## 工作区约束（继承自上层 CLAUDE.md）

- 本项目尚未分配端口；选定后登记到 `~/claude_project/CLAUDE.md` 的 Port Allocation 表。**不要使用 8080**（与微信客户端冲突，全局禁用）。
- 避免过度设计：CRM/客服等大模块按实际痛点迭代，不预建抽象层。
- 网络请求超时可走代理 `http://127.0.0.1:7897` 重试。

## 开发工作流

在修改任何功能前，先读取相关现有实现，列出：1) 当前文件结构，2) 现有路由/端点，3) 涉及的数据模型。然后提出方案，等待确认后再动手。
