//! 策展 source 白名单（代码侧）——「不开放原始库」的安全边界。
//!
//! 每个 source 声明：物理关系名（视图/表）、可选时间列、可作维度的列（含时间分桶）、
//! 可聚合的指标及其 SQL 聚合表达式。`datasets` 行只能引用已注册 source，且其 metrics/dimensions
//! 只能是 source 白名单的子集——管理员可零代码策展子集 / 改标签，但**碰不到白名单外的列**。
//! 查询引擎只拼装这些受控标识符 + 绑定参数注入值，无字符串拼接用户输入 → 无注入面。

/// 可作维度的列：`key` 对外名，`expr` 是 SELECT/GROUP BY 用的 SQL 片段（受控标识符）。
pub struct Dim {
    pub key: &'static str,
    pub expr: &'static str,
}

/// 可聚合指标：`key` 对外名，`agg` 是聚合 SQL 表达式（结果将 `::float8` 输出）。
pub struct Met {
    pub key: &'static str,
    pub agg: &'static str,
}

/// 策展 source：物理关系 + 时间列 + 维度/指标白名单。
pub struct Source {
    /// 注册键（datasets.source 引用此值）
    pub key: &'static str,
    /// 物理视图/表名（受控标识符）
    pub relation: &'static str,
    /// 时间列（支持 from/to 时间窗过滤）；无则为 None
    pub time_column: Option<&'static str>,
    pub dims: &'static [Dim],
    pub mets: &'static [Met],
}

impl Source {
    pub fn dim(&self, key: &str) -> Option<&Dim> {
        self.dims.iter().find(|d| d.key == key)
    }
    pub fn met(&self, key: &str) -> Option<&Met> {
        self.mets.iter().find(|m| m.key == key)
    }
}

/// 已注册 source 白名单。新数据源 = 在此加一条（+ 必要时建策展视图迁移），属代码改动。
static SOURCES: &[Source] = &[
    Source {
        key: "usage",
        relation: "usage_logs",
        time_column: Some("created_at"),
        dims: &[
            Dim {
                key: "model_id",
                expr: "model_id",
            },
            Dim {
                key: "channel_id",
                expr: "channel_id",
            },
            Dim {
                key: "day",
                expr: "date_trunc('day', created_at)",
            },
        ],
        mets: &[
            Met {
                key: "calls",
                agg: "count(*)",
            },
            Met {
                key: "revenue",
                agg: "coalesce(sum(charged_amount), 0)",
            },
            Met {
                key: "avg_latency",
                agg: "avg(latency_ms)",
            },
            // P95 延迟（有序集聚合，合法 PG；NULL latency 自动忽略）——运维数据集用
            Met {
                key: "p95_latency",
                agg: "percentile_cont(0.95) within group (order by latency_ms)",
            },
            // 流式调用占比 0..1——运维数据集用
            Met {
                key: "stream_ratio",
                agg: "avg(case when is_stream then 1 else 0 end)",
            },
        ],
    },
    // orders：充值订单（账单 + 销售业绩共用）。Paid=2（OrderStatus 枚举 smallint）。
    Source {
        key: "orders",
        relation: "orders",
        time_column: Some("created_at"),
        dims: &[
            Dim {
                key: "status",
                expr: "status",
            },
            Dim {
                key: "pay_channel",
                expr: "pay_channel",
            },
            Dim {
                key: "created_by_sales_id",
                expr: "created_by_sales_id",
            },
            Dim {
                key: "org_id",
                expr: "org_id",
            },
            Dim {
                key: "day",
                expr: "date_trunc('day', created_at)",
            },
        ],
        mets: &[
            Met {
                key: "order_count",
                agg: "count(*)",
            },
            Met {
                key: "order_amount",
                agg: "coalesce(sum(amount), 0)",
            },
            // 已支付金额/单数：filter status=2(Paid)
            Met {
                key: "paid_amount",
                agg: "coalesce(sum(amount) filter (where status = 2), 0)",
            },
            Met {
                key: "paid_count",
                agg: "count(*) filter (where status = 2)",
            },
            Met {
                key: "customer_count",
                agg: "count(distinct org_id)",
            },
        ],
    },
];

/// 按注册键取 source。
pub fn source(key: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.key == key)
}

/// 校验是否合法 SQL 标识符（小写字母/数字/下划线，字母或下划线开头）。
/// rls_rule.column 由管理员声明（非终端用户输入），此为纵深防御。
pub fn is_safe_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_lowercase() || (i > 0 && b.is_ascii_digit()))
}
