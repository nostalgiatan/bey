//! # 策略引擎类型定义模块
//!
//! 定义策略引擎使用的基本类型和枚举

use serde::{Deserialize, Serialize};

/// 策略动作类型
///
/// 定义策略评估后可以执行的动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyAction {
    /// 允许操作
    Allow,
    /// 拒绝操作
    Deny,
    /// 需要额外验证
    RequireAuthentication,
    /// 限制访问
    Restrict,
    /// 记录日志
    Log,
    /// 需要审批
    RequireApproval,
    /// 隔离处理
    Quarantine,
}

/// 策略条件操作符
///
/// 定义条件评估使用的各种操作符
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConditionOperator {
    /// 等于
    Equals,
    /// 不等于
    NotEquals,
    /// 大于
    GreaterThan,
    /// 大于等于
    GreaterThanOrEqual,
    /// 小于
    LessThan,
    /// 小于等于
    LessThanOrEqual,
    /// 包含
    Contains,
    /// 不包含
    NotContains,
    /// 正则匹配
    Regex,
    /// 在列表中
    In,
    /// 不在列表中
    NotIn,
    /// 逻辑与
    And,
    /// 逻辑或
    Or,
    /// 逻辑非
    Not,
}

impl std::fmt::Display for ConditionOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConditionOperator::Equals => write!(f, "=="),
            ConditionOperator::NotEquals => write!(f, "!="),
            ConditionOperator::GreaterThan => write!(f, ">"),
            ConditionOperator::GreaterThanOrEqual => write!(f, ">="),
            ConditionOperator::LessThan => write!(f, "<"),
            ConditionOperator::LessThanOrEqual => write!(f, "<="),
            ConditionOperator::Contains => write!(f, "contains"),
            ConditionOperator::NotContains => write!(f, "not_contains"),
            ConditionOperator::Regex => write!(f, "regex"),
            ConditionOperator::In => write!(f, "in"),
            ConditionOperator::NotIn => write!(f, "not_in"),
            ConditionOperator::And => write!(f, "and"),
            ConditionOperator::Or => write!(f, "or"),
            ConditionOperator::Not => write!(f, "not"),
        }
    }
}

/// 策略引擎统计信息
///
/// 记录策略引擎的运行统计数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyEngineStats {
    /// 总评估次数
    pub total_evaluations: u64,
    /// 缓存命中次数
    pub cache_hits: u64,
    /// 缓存未命中次数
    pub cache_misses: u64,
    /// 平均评估时间（微秒）
    pub average_evaluation_time_us: u64,
    /// 最慢评估时间（微秒）
    pub slowest_evaluation_time_us: u64,
    /// 最快评估时间（微秒）
    pub fastest_evaluation_time_us: u64,
    /// 策略集合数量
    pub policy_sets_count: usize,
    /// 总规则数量
    pub total_rules_count: usize,
    /// 启用的规则数量
    pub enabled_rules_count: usize,
}
