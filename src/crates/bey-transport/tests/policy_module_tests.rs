//! # 策略模块单元测试
//!
//! 测试策略模块的各个子模块功能

use bey_transport::policy::*;
use std::time::Duration;

#[test]
fn test_policy_action_types() {
    // 测试策略动作类型
    let allow = PolicyAction::Allow;
    let deny = PolicyAction::Deny;
    
    assert_ne!(allow, deny);
    assert_eq!(allow, PolicyAction::Allow);
}

#[test]
fn test_condition_operator_display() {
    // 测试条件操作符的显示
    assert_eq!(ConditionOperator::Equals.to_string(), "==");
    assert_eq!(ConditionOperator::NotEquals.to_string(), "!=");
    assert_eq!(ConditionOperator::GreaterThan.to_string(), ">");
    assert_eq!(ConditionOperator::LessThan.to_string(), "<");
}

#[test]
fn test_policy_context_creation() {
    // 测试策略上下文创建
    let context = PolicyContext::new()
        .with_requester_id("user-123".to_string())
        .with_resource("/api/data".to_string())
        .with_operation("read".to_string());
    
    assert_eq!(context.requester_id, Some("user-123".to_string()));
    assert_eq!(context.resource, Some("/api/data".to_string()));
    assert_eq!(context.operation, Some("read".to_string()));
}

#[test]
fn test_policy_context_fields() {
    // 测试策略上下文字段设置和获取
    let context = PolicyContext::new()
        .set_field("role".to_string(), serde_json::Value::String("admin".to_string()))
        .set_field("age".to_string(), serde_json::Value::Number(serde_json::Number::from(25)));
    
    assert_eq!(
        context.get_field_value("role"),
        Some(&serde_json::Value::String("admin".to_string()))
    );
    assert_eq!(
        context.get_field_value("age"),
        Some(&serde_json::Value::Number(serde_json::Number::from(25)))
    );
    assert_eq!(context.get_field_value("nonexistent"), None);
}

#[test]
fn test_policy_condition_creation() {
    // 测试策略条件创建
    let condition = PolicyCondition::new(
        "role".to_string(),
        ConditionOperator::Equals,
        serde_json::Value::String("admin".to_string()),
        "角色检查".to_string(),
    );
    
    assert_eq!(condition.field, "role");
    assert_eq!(condition.operator, ConditionOperator::Equals);
    assert_eq!(condition.weight, 1.0);
}

#[test]
fn test_policy_condition_with_weight() {
    // 测试策略条件权重设置
    let condition = PolicyCondition::new(
        "score".to_string(),
        ConditionOperator::GreaterThan,
        serde_json::Value::Number(serde_json::Number::from(80)),
        "分数检查".to_string(),
    )
    .with_weight(2.5);
    
    assert_eq!(condition.weight, 2.5);
}

#[test]
fn test_policy_rule_creation() {
    // 测试策略规则创建
    let rule = PolicyRule::new(
        "rule-001".to_string(),
        "管理员访问规则".to_string(),
        "允许管理员访问所有资源".to_string(),
        100,
        PolicyAction::Allow,
    );
    
    assert_eq!(rule.id, "rule-001");
    assert_eq!(rule.name, "管理员访问规则");
    assert_eq!(rule.priority, 100);
    assert_eq!(rule.action, PolicyAction::Allow);
    assert!(rule.enabled);
}

#[test]
fn test_policy_rule_with_condition() {
    // 测试策略规则添加条件
    let condition = PolicyCondition::new(
        "role".to_string(),
        ConditionOperator::Equals,
        serde_json::Value::String("admin".to_string()),
        "角色检查".to_string(),
    );
    
    let rule = PolicyRule::new(
        "rule-002".to_string(),
        "管理员规则".to_string(),
        "检查用户是否为管理员".to_string(),
        90,
        PolicyAction::Allow,
    )
    .add_condition(condition);
    
    assert_eq!(rule.conditions.len(), 1);
    assert_eq!(rule.conditions[0].field, "role");
}

#[test]
fn test_policy_set_creation() {
    // 测试策略集合创建
    let policy_set = PolicySet::new(
        "policy-001".to_string(),
        "访问控制策略".to_string(),
        "控制资源访问的策略集合".to_string(),
        PolicyAction::Deny,
    );
    
    assert_eq!(policy_set.id, "policy-001");
    assert_eq!(policy_set.name, "访问控制策略");
    assert_eq!(policy_set.default_action, PolicyAction::Deny);
    assert!(policy_set.enabled);
}

#[test]
fn test_policy_set_add_rule() {
    // 测试策略集合添加规则
    let rule = PolicyRule::new(
        "rule-001".to_string(),
        "测试规则".to_string(),
        "测试规则描述".to_string(),
        50,
        PolicyAction::Allow,
    );
    
    let policy_set = PolicySet::new(
        "policy-001".to_string(),
        "测试策略".to_string(),
        "测试策略描述".to_string(),
        PolicyAction::Deny,
    )
    .add_rule(rule);
    
    assert_eq!(policy_set.rules.len(), 1);
    assert_eq!(policy_set.rules[0].id, "rule-001");
}

#[test]
fn test_policy_engine_config_default() {
    // 测试策略引擎配置默认值
    let config = PolicyEngineConfig::default();
    
    assert!(config.enable_cache);
    assert_eq!(config.cache_ttl, Duration::from_secs(300));
    assert_eq!(config.max_cache_entries, 10000);
    assert!(config.enable_performance_monitoring);
    assert!(!config.enable_detailed_logging);
}

#[test]
fn test_policy_engine_config_customization() {
    // 测试策略引擎配置自定义
    let mut config = PolicyEngineConfig::default();
    config.enable_cache = false;
    config.max_cache_entries = 5000;
    config.enable_detailed_logging = true;
    
    assert!(!config.enable_cache);
    assert_eq!(config.max_cache_entries, 5000);
    assert!(config.enable_detailed_logging);
}

#[test]
fn test_policy_engine_stats_default() {
    // 测试策略引擎统计默认值
    let stats = PolicyEngineStats::default();
    
    assert_eq!(stats.total_evaluations, 0);
    assert_eq!(stats.cache_hits, 0);
    assert_eq!(stats.cache_misses, 0);
    assert_eq!(stats.policy_sets_count, 0);
}
