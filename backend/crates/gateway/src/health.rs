//! 渠道健康管理：自动禁用判定（纯函数）+ 定时探活 / 被动恢复。
//!
//! 状态语义复用 `ChannelStatus`：手动 `Disabled` 与自动 `CircuitBroken` 分开，被动恢复只动
//! CircuitBroken，绝不触碰管理员手动禁用的渠道。自动禁用以**状态码（401）为主、关键词为辅**
//! （new-api 教训：纯关键词黑名单脆弱），并受渠道级 `auto_ban` 一票否决。

use std::time::Duration;

use rise_core::{AppResult, AppState};
use rise_entity::channels;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

/// 自动禁用判定输入。
pub struct DisableCheck<'a> {
    /// 上游 HTTP 状态码（0 = 连接失败，不触发禁用——可能只是网络抖动）。
    pub status: u16,
    /// 上游响应/错误体（关键词匹配）。
    pub body: &'a str,
    /// 渠道级开关：false 则永不自动禁用。
    pub auto_ban: bool,
}

/// 触发自动禁用的状态码（401 = 鉴权失败 / key 失效）。可配留后续（避免过早造配置）。
const DISABLE_STATUS_CODES: &[u16] = &[401];

/// 错误关键词黑名单（小写匹配；偏脆弱，仅作状态码判定的补充）。
const DISABLE_KEYWORDS: &[&str] = &[
    "insufficient",
    "quota exceeded",
    "billing",
    "permission denied",
    "invalid api key",
    "expired",
];

/// 判定是否应自动禁用渠道；`Some(reason)` = 禁用并记原因，`None` = 不动。
pub fn should_disable(check: &DisableCheck) -> Option<String> {
    if !check.auto_ban {
        return None;
    }
    if DISABLE_STATUS_CODES.contains(&check.status) {
        return Some(format!(
            "upstream status {} (auth/credential failure)",
            check.status
        ));
    }
    let lower = check.body.to_lowercase();
    DISABLE_KEYWORDS
        .iter()
        .find(|kw| lower.contains(**kw))
        .map(|kw| format!("matched error keyword: {kw}"))
}

/// 响应时间阈值判定（探活用）：超阈值视为不健康。`threshold_ms == 0` 关闭。
pub fn exceeds_rt_threshold(latency_ms: i64, threshold_ms: u32) -> bool {
    threshold_ms > 0 && latency_ms > i64::from(threshold_ms)
}

/// 一轮探活的统计。
#[derive(Default, Debug)]
pub struct HealthCheckSummary {
    pub tested: usize,
    pub disabled: usize,
    pub recovered: usize,
}

/// 启动渠道健康探活（仅 `enabled` 时进入循环）。
///
/// **单实例假设**：多实例并发探活会重复打上游 / 状态抖动；多实例选主（PG advisory lock）留
/// 后续，本轮靠默认关闭 + 单实例部署规避。
pub fn spawn_health_check(state: AppState) {
    let cfg = state.config.channel_health.clone();
    if !cfg.enabled {
        tracing::info!("channel health check disabled (RR_CHANNEL_HEALTH_ENABLED != true)");
        return;
    }
    tokio::spawn(async move {
        // 启动延迟，避开迁移/seed 启动期竞争
        tokio::time::sleep(Duration::from_secs(60)).await;
        let period = Duration::from_secs(u64::from(cfg.interval_minutes) * 60);
        loop {
            match state.db() {
                Ok(db) => match run_health_check_once(db, cfg.rt_threshold_ms).await {
                    Ok(s) => tracing::info!(
                        tested = s.tested,
                        disabled = s.disabled,
                        recovered = s.recovered,
                        "channel health check done"
                    ),
                    Err(e) => tracing::warn!(error = %e, "channel health check failed"),
                },
                Err(_) => tracing::warn!("channel health check: db not ready"),
            }
            tokio::time::sleep(period).await;
        }
    });
}

/// 探活一轮：Enabled 失败/超时（且 auto_ban）→ 熔断；CircuitBroken 通过 → 被动恢复 Enabled。
/// 手动 `Disabled` 绝不触碰。无测试模型的渠道跳过（不误判健康渠道为坏）。
pub async fn run_health_check_once(
    db: &DatabaseConnection,
    rt_threshold_ms: u32,
) -> AppResult<HealthCheckSummary> {
    let mut summary = HealthCheckSummary::default();

    // 1. 测 Enabled 渠道
    let enabled = channels::Entity::find()
        .filter(channels::Column::Status.eq(channels::ChannelStatus::Enabled))
        .all(db)
        .await?;
    for ch in enabled {
        let Ok(outcome) = crate::channel::test_channel_once(db, &ch, None).await else {
            continue; // 无测试模型等 → 跳过
        };
        summary.tested += 1;
        // 测速写回 + 可能的熔断合并为同一行的一次 UPDATE（避免两次独立写）。
        let auto_ban = ch.auto_ban;
        let mut am: channels::ActiveModel = ch.into();
        am.response_time = Set(Some(outcome.latency_ms.min(i32::MAX as i64) as i32));
        am.test_time = Set(Some(chrono::Utc::now().fixed_offset()));
        let reason = if !outcome.ok {
            Some(
                outcome
                    .error
                    .unwrap_or_else(|| format!("probe failed (status {})", outcome.status)),
            )
        } else if exceeds_rt_threshold(outcome.latency_ms, rt_threshold_ms) {
            Some(format!(
                "response time {}ms exceeds {}ms threshold",
                outcome.latency_ms, rt_threshold_ms
            ))
        } else {
            None
        };
        if let Some(reason) = reason {
            if auto_ban {
                am.status = Set(channels::ChannelStatus::CircuitBroken);
                am.disabled_reason = Set(Some(reason));
                summary.disabled += 1;
            }
        }
        am.update(db).await?;
    }

    // 2. 被动恢复：测 CircuitBroken 渠道，通过则恢复 Enabled
    let broken = channels::Entity::find()
        .filter(channels::Column::Status.eq(channels::ChannelStatus::CircuitBroken))
        .all(db)
        .await?;
    for ch in broken {
        let Ok(outcome) = crate::channel::test_channel_once(db, &ch, None).await else {
            continue;
        };
        summary.tested += 1;
        // 测速写回 + 可能的恢复合并为一次 UPDATE。
        let mut am: channels::ActiveModel = ch.into();
        am.response_time = Set(Some(outcome.latency_ms.min(i32::MAX as i64) as i32));
        am.test_time = Set(Some(chrono::Utc::now().fixed_offset()));
        if outcome.ok && !exceeds_rt_threshold(outcome.latency_ms, rt_threshold_ms) {
            am.status = Set(channels::ChannelStatus::Enabled);
            am.disabled_reason = Set(None);
            summary.recovered += 1;
        }
        am.update(db).await?;
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(status: u16, body: &str, auto_ban: bool) -> DisableCheck<'_> {
        DisableCheck {
            status,
            body,
            auto_ban,
        }
    }

    #[test]
    fn disables_on_401() {
        assert!(should_disable(&check(401, "", true)).is_some());
    }

    #[test]
    fn auto_ban_off_is_one_vote_veto() {
        // 即便命中 401 + 关键词，auto_ban=false 也不禁用
        assert!(should_disable(&check(401, "insufficient balance", false)).is_none());
    }

    #[test]
    fn disables_on_keyword_even_when_not_401() {
        assert!(
            should_disable(&check(400, "Your account has insufficient balance", true)).is_some()
        );
    }

    #[test]
    fn keeps_on_ordinary_4xx() {
        // 普通客户端错误（参数缺失）不该熔断渠道
        assert!(
            should_disable(&check(400, "invalid request: missing field 'model'", true)).is_none()
        );
    }

    #[test]
    fn connect_failure_status_zero_not_disabled() {
        assert!(should_disable(&check(0, "", true)).is_none());
    }

    #[test]
    fn rt_threshold_logic() {
        assert!(exceeds_rt_threshold(6000, 5000));
        assert!(!exceeds_rt_threshold(3000, 5000));
        assert!(!exceeds_rt_threshold(99999, 0)); // 0 = 关闭
    }
}
