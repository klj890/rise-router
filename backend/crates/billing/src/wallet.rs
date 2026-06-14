//! 钱包：可用额度查询 + 充值入账 + 消费扣减（settle 复用）。
//!
//! 余额变更走原子 `UPDATE ... RETURNING balance`（行锁串行化并发扣减，免读改写竞态），
//! 同时追加 transactions 流水（balance_after 取 RETURNING 的新余额）。

use rise_core::{AppError, AppResult};
use rise_entity::{transactions, wallets};
use rust_decimal::Decimal;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, Set,
    Statement, TransactionError, TransactionTrait,
};

/// 确保 org 有钱包行（不存在则建 0 余额）。幂等。
async fn ensure_wallet<C: ConnectionTrait>(db: &C, org_id: i32) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "INSERT INTO wallets (org_id, balance, credit_limit, frozen) VALUES ($1, 0, 0, 0) \
         ON CONFLICT (org_id) DO NOTHING",
        [org_id.into()],
    ))
    .await?;
    Ok(())
}

/// 原子调整余额并记一笔流水，返回调整后的新余额。delta 有符号（+充值/-消费）。
/// DB 级（Result<_, DbErr>），可在外层事务中复用（settle 调用）。
#[allow(clippy::too_many_arguments)]
async fn adjust_balance<C: ConnectionTrait>(
    db: &C,
    org_id: i32,
    delta: Decimal,
    kind: transactions::TxnKind,
    ref_type: Option<&str>,
    ref_id: Option<i64>,
    memo: Option<String>,
    at: DateTimeWithTimeZone,
) -> Result<Decimal, DbErr> {
    // 对齐 numeric(18,8)：delta 若超 8 位会与库存截断分歧（充值金额可能带更多小数）
    let delta = delta.round_dp(8);
    // 乐观更新：先 UPDATE（热路径钱包已存在，命中即省一次写）；行锁串行化并发扣减、RETURNING 拿新余额。
    // 仅当钱包不存在（0 行）才建钱包后重试，避免每次扣减都打一次 INSERT。
    let sql = "UPDATE wallets SET balance = balance + $1 WHERE org_id = $2 RETURNING balance";
    let backend = db.get_database_backend();
    let row = match db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            sql,
            [delta.into(), org_id.into()],
        ))
        .await?
    {
        Some(r) => r,
        None => {
            ensure_wallet(db, org_id).await?;
            db.query_one_raw(Statement::from_sql_and_values(
                backend,
                sql,
                [delta.into(), org_id.into()],
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("wallet missing after ensure".into()))?
        }
    };
    let new_balance: Decimal = row.try_get("", "balance")?;

    transactions::ActiveModel {
        org_id: Set(org_id),
        kind: Set(kind),
        amount: Set(delta),
        balance_after: Set(new_balance),
        ref_type: Set(ref_type.map(str::to_string)),
        ref_id: Set(ref_id),
        memo: Set(memo),
        created_at: Set(at),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(new_balance)
}

/// 消费扣减（settle 在其事务内复用）：余额 -amount + 记 Consume 流水关联 usage_log。
/// **契约**：调用方负责事务（consume 不自包事务，避免 settle 大事务内再开 savepoint 的往返开销）。
/// 本函数为 crate 内部原语，唯一调用方 settle_chat 已在事务中调用，故扣余额+记流水随 settle 原子。
pub async fn consume<C: ConnectionTrait>(
    db: &C,
    org_id: i32,
    amount: Decimal,
    usage_log_id: i64,
    at: DateTimeWithTimeZone,
) -> Result<(), DbErr> {
    // 先拒真正的非正数（负/零=编程错误）
    if amount <= Decimal::ZERO {
        return Err(DbErr::Custom("consume amount must be positive".into()));
    }
    let amount = amount.round_dp(8);
    // 极小正数 round 到 0：跳过扣费/流水但返回 Ok，避免 settle 事务回滚丢失 usage_log
    if amount.is_zero() {
        return Ok(());
    }
    adjust_balance(
        db,
        org_id,
        -amount,
        transactions::TxnKind::Consume,
        Some("usage_log"),
        Some(usage_log_id),
        None,
        at,
    )
    .await?;
    Ok(())
}

/// 钱包可用额度 = 余额 + 授信 − 冻结；无钱包视为 0。
pub async fn wallet_available<C: ConnectionTrait>(db: &C, org_id: i32) -> AppResult<Decimal> {
    let w = wallets::Entity::find()
        .filter(wallets::Column::OrgId.eq(org_id))
        .one(db)
        .await?;
    Ok(w.map(|w| w.balance + w.credit_limit - w.frozen)
        .unwrap_or(Decimal::ZERO))
}

/// 网关计费预检：可用额度 <= 0 即拒（402）。后扣模型下放行可用额度 > 0 的请求。
pub async fn ensure_funds<C: ConnectionTrait>(db: &C, org_id: i32) -> AppResult<()> {
    if wallet_available(db, org_id).await? > Decimal::ZERO {
        Ok(())
    } else {
        Err(AppError::InsufficientBalance)
    }
}

/// 手动充值入账（管理员/销售代客）。事务内 余额+amount + 记 Recharge 流水。
/// 泛型 `C: TransactionTrait`：传 DatabaseConnection 起新事务，或传 DatabaseTransaction 作 savepoint
/// 嵌入外层事务（片 B 订单支付成功后「改订单状态 + 入账钱包」原子复用）。
pub async fn recharge<C: TransactionTrait>(
    db: &C,
    org_id: i32,
    amount: Decimal,
    ref_type: &str,
    ref_id: Option<i64>,
    memo: Option<String>,
    at: DateTimeWithTimeZone,
) -> AppResult<Decimal> {
    // 先 round 到 8 位再校验：极小正数 round 到 0 会记一笔 0 元充值流水却不增余额
    let amount = amount.round_dp(8);
    if amount <= Decimal::ZERO {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    // 上限：numeric(18,8) 整数部分仅 10 位，过大值写库会溢出 500；应用层先拦为 400
    if amount > Decimal::from(9_999_999_999i64) {
        return Err(AppError::BadRequest("amount exceeds maximum limit".into()));
    }
    let ref_type = ref_type.to_string();
    db.transaction::<_, Decimal, DbErr>(move |txn| {
        Box::pin(async move {
            adjust_balance(
                txn,
                org_id,
                amount,
                transactions::TxnKind::Recharge,
                Some(&ref_type),
                ref_id,
                memo,
                at,
            )
            .await
        })
    })
    .await
    .map_err(|e| match e {
        TransactionError::Connection(db) | TransactionError::Transaction(db) => AppError::Db(db),
    })
}
