# Rise Router

企业级（to-B）AI API Router 平台 —— 对开源 new-api 的重新设计与重构。

核心目标是消灭 new-api 繁琐的倍率体系，改为**定价五要素（模型 / 渠道 / 价格 / 用户分组 / 折扣）完全解耦**的关系模型，并落成**微内核可插拔平台**：核心提供用户 / 权限 / 路由 / 计费四大基座，业务能力（含内部模块与第三方系统）以 App 形式接入。

## 技术栈

- **后端**：Rust + Axum + SeaORM 2.0 + PostgreSQL（单库）；Redis；S3 兼容对象存储
- **前端**：Vite + React 19 + Ant Design 6 + TanStack Query + Zustand（Module Federation 可插拔）

## 文档

| 文档 | 内容 |
|---|---|
| [docs/README.md](docs/README.md) | 文档索引与阅读顺序 |
| [docs/architecture.md](docs/architecture.md) | 系统架构、crate 切分、中间件链、借鉴清单 |
| [docs/roadmap.md](docs/roadmap.md) | 功能清单（十大域）+ 里程碑 M0–M6 |
| [docs/data-model.md](docs/data-model.md) | 核心数据模型 ER 设计（PostgreSQL + SeaORM） |

## 工程结构

```
backend/         Cargo workspace
  crates/core    微内核：config/db/error/AppState
  crates/{identity,rbac,gateway,pricing,billing,task,report,crm,support}
                 9 个业务域，各暴露 routes() 挂到 /api/<域>
  crates/server  axum 装配 + /healthz + /readyz + 中间件链
  migration      SeaORM 迁移
frontend/shell/  React 19 + AntD 6 控制台
```

## 本地开发

```bash
make infra-up      # 启动 PostgreSQL / Redis / MinIO
make migrate       # 应用数据库迁移
make be-run        # 后端 :8088
make fe-dev        # 前端 :5273
```

更多命令见 `make help`。当前进度：**M0 工程脚手架**（详见 [docs/roadmap.md](docs/roadmap.md)）。

## License

Apache-2.0
