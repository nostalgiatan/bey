//! # 策略模块
//!
//! 提供灵活、高性能的安全策略管理功能

pub mod types;
pub mod context;
pub mod condition;
pub mod rule;
pub mod set;
pub mod config;

// 重新导出常用类型
pub use types::{PolicyAction, ConditionOperator, PolicyEngineStats};
pub use context::PolicyContext;
pub use condition::{PolicyCondition, ConditionEvaluationResult};
pub use rule::{PolicyRule, PolicyEvaluationResult};
pub use set::{PolicySet, PolicySetEvaluationResult};
pub use config::PolicyEngineConfig;
