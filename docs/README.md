# Rise Router 设计文档索引

Rise Router 是对开源 new-api 的企业级（to-B）重写，定位为**微内核 AI API Router 平台**：核心提供用户/权限/路由/计费四大基座，业务能力（含内部模块与第三方系统）以 App 形式可插拔接入。核心设计目标是消灭 new-api 繁琐的倍率体系，实现**定价五要素（模型/渠道/价格/分组/折扣）完全解耦**。

> 状态：设计阶段（2026-06-13）。代码尚未初始化；动手前请通读以下文档。

## 文档

| 文档 | 内容 |
|---|---|
| [architecture.md](./architecture.md) | 系统架构：微内核总览图、后端 Cargo workspace crate 切分、前端 Shell + Module Federation 结构、技术组件、网关中间件链、本地兄弟项目借鉴清单、核心架构原则 |
| [roadmap.md](./roadmap.md) | 功能清单（十大域）+ 开发里程碑 M0–M6 + 持续约束 |
| [data-model.md](./data-model.md) | 核心数据模型 ER 设计（建表前蓝图，PostgreSQL + SeaORM，十大域 + 跨域审计 + Mermaid ER 图）|
| [i18n.md](./i18n.md) | 国际化架构：全栈 i18n（UI/内容/API错误/格式化四套解耦）、JSONB 本地化字段、locale 协商、可插拔译文 |
| [implementation.md](./implementation.md) | **已实现功能与数据库设计（as-built）**：实现架构图、真实表 ER 图、定价解析流程图、主题/i18n 设计图、API 端点清单 |

## 阅读顺序建议

1. 先读 **architecture.md** 第 1–2 节，建立"微内核平台 + 五要素解耦"的整体认知。
2. 再读 **data-model.md**，理解十大域的实体与两条解耦主线（路由线 / 定价线）。
3. 最后读 **roadmap.md**，明确交付顺序（MVP 网关+定价 → 财务 → CRM → 报表 → 可插拔+客服 → 合规）。

## 技术栈速览

- **后端**：Rust + Axum + SeaORM 2.0 + PostgreSQL（单库）；Redis；S3 兼容对象存储。
- **前端**：Vite + React 19 + Ant Design 6 + TanStack Query + Zustand；Module Federation 可插拔。
- **同栈参考实现**：`~/claude_project/billing-report/agent-console/`（Rust axum + React）。
