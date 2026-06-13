.PHONY: help infra-up infra-down be-build be-run be-fmt be-lint be-test migrate fe-install fe-dev fe-build

help:
	@echo "Rise Router —— 常用命令"
	@echo "  make infra-up     启动 PG/Redis/MinIO (docker compose)"
	@echo "  make infra-down   停止基础设施"
	@echo "  make be-build     编译后端"
	@echo "  make be-run       运行后端 (rise-server, :8088)"
	@echo "  make be-fmt       cargo fmt"
	@echo "  make be-lint      cargo clippy"
	@echo "  make be-test      cargo test"
	@echo "  make migrate      应用数据库迁移 (migration up)"
	@echo "  make fe-install   安装前端依赖"
	@echo "  make fe-dev       前端开发服务器 (:5273)"
	@echo "  make fe-build     前端生产构建"

infra-up:
	docker compose up -d

infra-down:
	docker compose down

be-build:
	cd backend && cargo build

be-run:
	cd backend && cargo run -p rise-server

be-fmt:
	cd backend && cargo fmt --all

be-lint:
	cd backend && cargo clippy --all-targets -- -D warnings

be-test:
	cd backend && cargo test

migrate:
	cd backend && DATABASE_URL=$${DATABASE_URL:-postgres://rise:rise@localhost:5432/rise_router} cargo run -p migration -- up

fe-install:
	cd frontend/shell && pnpm install

fe-dev:
	cd frontend/shell && pnpm dev

fe-build:
	cd frontend/shell && pnpm build
