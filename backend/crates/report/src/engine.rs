//! 报表查询引擎（M4 片A 内核）：白名单校验 + 行级隔离(RLS)强制注入 + 参数化执行。
//!
//! 安全三步（对齐 docs/data-model.md §⑨）：
//! ① 权限门禁：principal 须持 `dataset.required_permission`；
//! ② 取 RLS 分支：按 principal 有效角色查 `dataset.rls_rule[role]`（缺键=禁止；null=全量；
//!    {column,param}=按列绑定参数过滤）；
//! ③ 拼装：维度/指标只取自 source 白名单（受控标识符），过滤值一律绑定参数（$1..）注入，
//!    无字符串拼接用户输入。用户无法绕过 RLS。
use rise_core::{AppError, AppResult, AppState};
use rise_entity::datasets;
use rise_identity::Principal;
use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};
use serde::{Deserialize, Serialize};

use crate::source::{self, Source};

/// 查询请求体（POST /datasets/{slug}/query）。
#[derive(Debug, Deserialize)]
pub struct QueryReq {
    /// 选定指标 key（须在数据集 metrics ∩ source 指标白名单内，至少一个）
    pub metrics: Vec<String>,
    /// 选定维度 key（可空 = 整体聚合）
    #[serde(default)]
    pub dimensions: Vec<String>,
    /// 时间窗起（含），RFC3339；source 须声明 time_column
    pub from: Option<String>,
    /// 时间窗止（不含），RFC3339
    pub to: Option<String>,
    /// 行数上限（默认 1000，硬上限 10000）
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct QueryResp {
    pub dataset: String,
    /// 实际生效的 RLS：角色 + 是否注入了行级过滤
    pub role: String,
    pub rls_filtered: bool,
    pub dimensions: Vec<String>,
    pub metrics: Vec<String>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

const LIMIT_DEFAULT: u64 = 1000;
const LIMIT_MAX: u64 = 10_000;

/// 从 datasets.metrics / dimensions（JSON 数组 [{key,label}]）提取允许的 key 集。
fn declared_keys(field: &serde_json::Value) -> Vec<String> {
    field
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.get("key").and_then(|k| k.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// 解析 rls_rule 的当前角色分支，产出可选过滤 (column, bound_value)。
/// 缺键 → Forbidden（该角色不可访问）；null → None（全量）；{column,param} → Some。
fn resolve_rls(
    dataset: &datasets::Model,
    principal: &Principal,
) -> AppResult<Option<(String, Value)>> {
    let rule = dataset
        .rls_rule
        .get(principal.role.as_str())
        .ok_or(AppError::Forbidden)?; // 角色无分支声明 = 禁止访问该数据集
    if rule.is_null() {
        return Ok(None); // 全量（finance/admin 等）
    }
    let column = rule
        .get("column")
        .and_then(|c| c.as_str())
        .filter(|c| source::is_safe_ident(c))
        .ok_or_else(|| AppError::Internal("dataset rls_rule.column invalid".into()))?;
    let param = rule
        .get("param")
        .and_then(|p| p.as_str())
        .ok_or_else(|| AppError::Internal("dataset rls_rule.param missing".into()))?;
    let val: i32 = match param {
        "current_org" => principal.org_id.ok_or(AppError::Forbidden)?,
        "current_user" | "current_sales" => principal.user_id.ok_or(AppError::Forbidden)?,
        _ => return Err(AppError::Internal("dataset rls_rule.param unknown".into())),
    };
    Ok(Some((column.to_owned(), Value::from(val))))
}

/// 执行一次数据集查询（已鉴权 + RLS 强制注入）。
pub async fn run(
    state: &AppState,
    principal: &Principal,
    dataset: &datasets::Model,
    req: QueryReq,
) -> AppResult<QueryResp> {
    // ① 权限门禁
    if !principal.perms.contains(&dataset.required_permission) {
        return Err(AppError::Forbidden);
    }
    let src: &Source = source::source(&dataset.source)
        .ok_or_else(|| AppError::Internal("dataset source not registered".into()))?;

    // 校验指标：非空，且 ∈ 数据集声明 ∩ source 白名单
    if req.metrics.is_empty() {
        return Err(AppError::BadRequest("at least one metric required".into()));
    }
    let allowed_metrics = declared_keys(&dataset.metrics);
    let allowed_dims = declared_keys(&dataset.dimensions);

    let mut select_parts: Vec<String> = Vec::new();
    for d in &req.dimensions {
        if !allowed_dims.contains(d) {
            return Err(AppError::BadRequest(format!("unknown dimension: {d}")));
        }
        let dim = src
            .dim(d)
            .ok_or_else(|| AppError::Internal("dimension not in source".into()))?;
        select_parts.push(format!("({})::text AS \"{}\"", dim.expr, dim.key));
    }
    for m in &req.metrics {
        if !allowed_metrics.contains(m) {
            return Err(AppError::BadRequest(format!("unknown metric: {m}")));
        }
        let met = src
            .met(m)
            .ok_or_else(|| AppError::Internal("metric not in source".into()))?;
        select_parts.push(format!("({})::float8 AS \"{}\"", met.agg, met.key));
    }

    // ② RLS 分支 + ③ 拼装（绑定参数）
    let rls = resolve_rls(dataset, principal)?;
    let mut values: Vec<Value> = Vec::new();
    let mut wheres: Vec<String> = Vec::new();

    if let Some((column, val)) = &rls {
        values.push(val.clone());
        wheres.push(format!("{} = ${}", column, values.len()));
    }
    // 时间窗（source 须有 time_column）
    if req.from.is_some() || req.to.is_some() {
        let tcol = src
            .time_column
            .ok_or_else(|| AppError::BadRequest("dataset has no time column".into()))?;
        if let Some(from) = &req.from {
            let dt = chrono::DateTime::parse_from_rfc3339(from)
                .map_err(|_| AppError::BadRequest("invalid 'from' (RFC3339)".into()))?;
            values.push(Value::from(dt));
            wheres.push(format!("{} >= ${}", tcol, values.len()));
        }
        if let Some(to) = &req.to {
            let dt = chrono::DateTime::parse_from_rfc3339(to)
                .map_err(|_| AppError::BadRequest("invalid 'to' (RFC3339)".into()))?;
            values.push(Value::from(dt));
            wheres.push(format!("{} < ${}", tcol, values.len()));
        }
    }

    let where_clause = if wheres.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", wheres.join(" AND "))
    };
    // 维度表达式用于 GROUP BY（与 SELECT 同源）
    let group_clause = if req.dimensions.is_empty() {
        String::new()
    } else {
        let exprs: Vec<&str> = req
            .dimensions
            .iter()
            .filter_map(|d| src.dim(d).map(|x| x.expr))
            .collect();
        format!(" GROUP BY {}", exprs.join(", "))
    };
    let order_clause = if req.dimensions.is_empty() {
        String::new()
    } else {
        " ORDER BY 1".to_owned()
    };
    let limit = req.limit.unwrap_or(LIMIT_DEFAULT).min(LIMIT_MAX);

    let sql = format!(
        "SELECT {} FROM {}{}{}{} LIMIT {}",
        select_parts.join(", "),
        src.relation,
        where_clause,
        group_clause,
        order_clause,
        limit,
    );

    let db = state.db()?;
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
    let result = db.query_all_raw(stmt).await?;

    let mut rows = Vec::with_capacity(result.len());
    for qr in &result {
        let mut obj = serde_json::Map::new();
        for d in &req.dimensions {
            let v: Option<String> = qr.try_get("", d).map_err(AppError::from)?;
            obj.insert(
                d.clone(),
                v.map(serde_json::Value::from)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        for m in &req.metrics {
            let v: Option<f64> = qr.try_get("", m).map_err(AppError::from)?;
            obj.insert(
                m.clone(),
                v.and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        rows.push(obj);
    }

    Ok(QueryResp {
        dataset: dataset.slug.clone(),
        role: principal.role.clone(),
        rls_filtered: rls.is_some(),
        dimensions: req.dimensions,
        metrics: req.metrics,
        rows,
    })
}
