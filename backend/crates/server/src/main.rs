//! rise-server —— 单二进制入口：装配 axum 路由、挂载各业务域、应用中间件链。
//!
//! M0 中间件链为骨架（trace + cors）；后续里程碑按
//! `docs/architecture.md` 第 6 节补全：认证 → 两层白名单 → RBAC → 限流 → 计费预扣 → 审计。

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use rise_core::{db, AppState, Config};
use serde_json::json;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();
    init_tracing(&config.log_level);

    // M0：容忍数据库未就绪，脚手架仍可启动；/readyz 报告真实状态。
    let db = match db::connect(&config.database_url).await {
        Ok(conn) => {
            tracing::info!("database connected");
            // 幂等落地 RBAC 内置角色/权限点（重放安全）。失败仅告警不阻断启动。
            if let Err(e) = rise_rbac::seed_builtins(&conn).await {
                tracing::warn!(error = %e, "rbac seed_builtins failed; RBAC may be incomplete");
            }
            Some(conn)
        }
        Err(e) => {
            tracing::warn!(error = %e, "database not connected; serving in degraded mode");
            None
        }
    };

    let bind_addr = config.bind_addr.clone();
    let state = AppState::new(config, db);
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("rise-server listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing(log_level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "rise_server={lvl},rise_core={lvl},tower_http={lvl}",
            lvl = log_level
        ))
    });
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

fn build_router(state: AppState) -> Router {
    // 各业务域以统一标准挂载（狗粮原则：内部模块与第三方走同一注册方式）。
    let api = Router::new()
        .nest("/identity", rise_identity::routes())
        .nest("/rbac", rise_rbac::routes())
        .nest("/gateway", rise_gateway::routes())
        .nest("/pricing", rise_pricing::routes())
        .nest("/billing", rise_billing::routes())
        .nest("/task", rise_task::routes())
        .nest("/report", rise_report::routes())
        .nest("/crm", rise_crm::routes())
        .nest("/support", rise_support::routes());

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .nest("/api", api)
        // OpenAI 兼容入口挂在根 /v1（relay 转发）
        .merge(rise_gateway::relay_routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// 存活探针：进程在跑即 200，不依赖外部依赖。
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// 就绪探针：检查数据库连通性。
async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match &state.db {
        Some(conn) => match conn.ping().await {
            Ok(_) => (
                StatusCode::OK,
                Json(json!({ "status": "ready", "db": "up" })),
            ),
            Err(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "degraded", "db": e.to_string() })),
            ),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "degraded", "db": "not_connected" })),
        ),
    }
}
