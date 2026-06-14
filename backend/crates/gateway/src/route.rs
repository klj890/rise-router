//! 路由解析纯函数：候选排序 + 加权选取。无 DB 依赖，便于单测。
//!
//! 设计（docs/data-model.md §4）：给定 model → 候选渠道按优先级降序、同优先级加权随机；
//! 高优先级全部失败再降级到次优先级。路由与定价完全分离（仅在 models 处相交）。

use rand::Rng;
use serde::Serialize;

/// 一条候选路由（model_channel ⨝ channel，有效优先级/权重已算好）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteCandidate {
    pub channel_id: i32,
    pub channel_name: String,
    pub protocol_adapter: String,
    pub base_url: String,
    pub upstream_model_name: String,
    /// 有效优先级 = model_channel.priority ?? channel.priority
    pub priority: i32,
    /// 有效权重 = model_channel.weight ?? channel.weight
    pub weight: i32,
}

/// 故障转移顺序：优先级降序，同优先级权重降序，再按 channel_id 升序保证稳定。
pub fn rank_routes(mut candidates: Vec<RouteCandidate>) -> Vec<RouteCandidate> {
    // 比较器含 channel_id 兜底，排序完全确定，可用 unstable（无临时分配、更快）
    candidates.sort_unstable_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(b.weight.cmp(&a.weight))
            .then(a.channel_id.cmp(&b.channel_id))
    });
    candidates
}

/// 在最高优先级组内按权重选一条；`rand` 取 [0, 总权重) 的值（由调用方注入，便于测试）。
/// 全部权重为 0 时回落到第一条。无候选返回 None。
pub fn pick_weighted(candidates: &[RouteCandidate], rand: u64) -> Option<&RouteCandidate> {
    // 全程在 slice 上遍历，零堆分配（路由热路径）
    let top = candidates.iter().map(|c| c.priority).max()?;
    let in_top = |c: &&RouteCandidate| c.priority == top;

    let total: u64 = candidates
        .iter()
        .filter(in_top)
        .map(|c| c.weight.max(0) as u64)
        .sum();
    if total == 0 {
        // 同优先级且权重全 0：取 channel_id 最小的（稳定）
        return candidates
            .iter()
            .filter(in_top)
            .min_by_key(|c| c.channel_id);
    }
    let mut acc = 0u64;
    let target = rand % total;
    for c in candidates.iter().filter(in_top) {
        acc += c.weight.max(0) as u64;
        if target < acc {
            return Some(c);
        }
    }
    candidates.iter().rfind(in_top)
}

/// 故障转移序（加权随机）：优先级降序分层，**层内按权重随机洗牌**（同优先级负载均衡），
/// 拼成依次尝试的顺序。权重全 0 的层按 channel_id 升序（稳定回落）。
/// rng 由调用方注入（生产用 thread_rng，测试用种子化），便于确定性单测。
///
/// 与 [`rank_routes`]（完全确定，供 `/route` 预览）区分：本函数引入随机，用于真实转发负载均衡。
pub fn weighted_failover_order<R: Rng + ?Sized>(
    candidates: &[RouteCandidate],
    rng: &mut R,
) -> Vec<RouteCandidate> {
    // 先按优先级降序、id 升序排好，便于切层与零权重稳定回落
    let mut sorted = candidates.to_vec();
    sorted.sort_unstable_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.channel_id.cmp(&b.channel_id))
    });

    let mut result = Vec::with_capacity(sorted.len());
    let mut i = 0;
    while i < sorted.len() {
        let prio = sorted[i].priority;
        let end = i + sorted[i..]
            .iter()
            .take_while(|c| c.priority == prio)
            .count();
        let mut tier: Vec<RouteCandidate> = sorted[i..end].to_vec();
        i = end;
        // 层内加权随机洗牌：反复按权重无放回抽取
        while !tier.is_empty() {
            let total: u64 = tier.iter().map(|c| c.weight.max(0) as u64).sum();
            let idx = if total == 0 {
                0 // 全 0 权重：取当前首条（已 id 升序），稳定
            } else {
                let target = rng.gen_range(0..total);
                let mut acc = 0u64;
                let mut sel = tier.len() - 1;
                for (j, c) in tier.iter().enumerate() {
                    acc += c.weight.max(0) as u64;
                    if target < acc {
                        sel = j;
                        break;
                    }
                }
                sel
            };
            result.push(tier.remove(idx));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn cand(id: i32, priority: i32, weight: i32) -> RouteCandidate {
        RouteCandidate {
            channel_id: id,
            channel_name: format!("ch{id}"),
            protocol_adapter: "openai_compatible".into(),
            base_url: "http://x".into(),
            upstream_model_name: "gpt-4o".into(),
            priority,
            weight,
        }
    }

    #[test]
    fn ranks_by_priority_then_weight_then_id() {
        let ranked = rank_routes(vec![
            cand(1, 10, 5),
            cand(2, 20, 1),
            cand(3, 20, 9),
            cand(4, 20, 9),
        ]);
        let ids: Vec<i32> = ranked.iter().map(|c| c.channel_id).collect();
        // 优先级 20 组在前；组内权重 9 在前；同权重按 id 升序；优先级 10 垫底
        assert_eq!(ids, vec![3, 4, 2, 1]);
    }

    #[test]
    fn weighted_pick_respects_ranges() {
        let group = vec![cand(1, 10, 30), cand(2, 10, 70)];
        // 总权重 100：[0,30)→ch1，[30,100)→ch2
        assert_eq!(pick_weighted(&group, 0).unwrap().channel_id, 1);
        assert_eq!(pick_weighted(&group, 29).unwrap().channel_id, 1);
        assert_eq!(pick_weighted(&group, 30).unwrap().channel_id, 2);
        assert_eq!(pick_weighted(&group, 99).unwrap().channel_id, 2);
        // rand 取模总权重
        assert_eq!(pick_weighted(&group, 100).unwrap().channel_id, 1);
    }

    #[test]
    fn weighted_pick_only_top_priority() {
        // 低优先级渠道不应被选中（即便权重大）
        let cs = vec![cand(1, 20, 1), cand(2, 10, 999)];
        assert_eq!(pick_weighted(&cs, 0).unwrap().channel_id, 1);
        assert_eq!(pick_weighted(&cs, 12345).unwrap().channel_id, 1);
    }

    #[test]
    fn all_zero_weight_falls_back_to_min_id() {
        let cs = vec![cand(5, 10, 0), cand(2, 10, 0)];
        assert_eq!(pick_weighted(&cs, 7).unwrap().channel_id, 2);
    }

    #[test]
    fn empty_returns_none() {
        assert!(pick_weighted(&[], 0).is_none());
        assert!(rank_routes(vec![]).is_empty());
    }

    #[test]
    fn failover_order_is_permutation_with_priority_tiers_preserved() {
        let cs = vec![
            cand(1, 10, 5),
            cand(2, 20, 1),
            cand(3, 20, 9),
            cand(4, 5, 3),
        ];
        let mut rng = StdRng::seed_from_u64(7);
        let ordered = weighted_failover_order(&cs, &mut rng);
        // 是输入的一个排列
        let mut ids: Vec<i32> = ordered.iter().map(|c| c.channel_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        // 优先级单调不增（高优先级层整体在前）
        let prios: Vec<i32> = ordered.iter().map(|c| c.priority).collect();
        assert!(prios.windows(2).all(|w| w[0] >= w[1]));
        // 最高优先级 20 的两条排在最前
        assert_eq!(prios[0], 20);
        assert_eq!(prios[1], 20);
    }

    #[test]
    fn failover_order_weights_first_pick() {
        // 同优先级权重 1 vs 99：多种子下权重 99 的渠道应在多数情况排首位
        let cs = vec![cand(1, 10, 1), cand(2, 10, 99)];
        let mut first_is_2 = 0;
        for seed in 0..200u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            if weighted_failover_order(&cs, &mut rng)[0].channel_id == 2 {
                first_is_2 += 1;
            }
        }
        // 期望 ~99%，宽松断言 > 85% 即证明加权生效
        assert!(
            first_is_2 > 170,
            "weight-99 first-pick rate too low: {first_is_2}/200"
        );
    }

    #[test]
    fn failover_order_zero_weight_tier_is_id_ascending() {
        let cs = vec![cand(5, 10, 0), cand(2, 10, 0), cand(8, 10, 0)];
        let mut rng = StdRng::seed_from_u64(1);
        let ids: Vec<i32> = weighted_failover_order(&cs, &mut rng)
            .iter()
            .map(|c| c.channel_id)
            .collect();
        assert_eq!(ids, vec![2, 5, 8]);
    }
}
