//! 策略引擎综合测试
//!
//! 测试策略引擎的核心功能，包括条件评估、规则评估、策略集合评估和缓存机制。

use bey_transport::{
    CompletePolicyEngine, PolicyEngineConfig, PolicySet, PolicyRule, PolicyCondition, PolicyContext,
    PolicyAction, ConditionOperator, PolicySetEvaluationResult,
};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{sleep, TokioDuration};

fn init_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
}

fn create_test_context() -> PolicyContext {
    PolicyContext::new()
        .with_requester_id("user-123".to_string())
        .with_resource("/api/data".to_string())
        .with_operation("read".to_string())
        .with_field("ip_address".to_string(), serde_json::Value::String("192.168.1.100".to_string()))
        .with_field("role".to_string(), serde_json::Value::String("admin".to_string()))
        .with_field("department".to_string(), serde_json::Value::String("engineering".to_string()))
        .with_field("age".to_string(), serde_json::Value::Number(serde_json::Number::from(25)))
        .with_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(85)))
        .with_field("permissions".to_string(), serde_json::Value::Array(vec![
            serde_json::Value::String("read".to_string()),
            serde_json::Value::String("write".to_string()),
        ]))
}

fn create_complex_policy_set() -> PolicySet {
    PolicySet::new(
        "complex-access-control".to_string(),
        "复杂访问控制策略".to_string(),
        "多维度访问控制策略集合".to_string(),
        PolicyAction::Deny,
    )
    // 管理员完全访问规则
    .add_rule(
        PolicyRule::new(
            "admin-full-access".to_string(),
            "管理员完全访问".to_string(),
            "管理员用户拥有完全访问权限".to_string(),
            100,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("admin".to_string()),
            "角色检查".to_string(),
        ))
        .with_tag("admin".to_string())
        .with_tag("high-priority".to_string())
    )
    // 工程部门权限规则
    .add_rule(
        PolicyRule::new(
            "engineering-access".to_string(),
            "工程部门访问".to_string(),
            "工程部门用户可以访问技术资源".to_string(),
            80,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "department".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("engineering".to_string()),
            "部门检查".to_string(),
        ))
        .add_condition(PolicyCondition::new(
            "age".to_string(),
            ConditionOperator::GreaterThanOrEqual,
            serde_json::Value::Number(serde_json::Number::from(18)),
            "年龄检查".to_string(),
        ))
        .with_condition_combination(ConditionOperator::And)
        .with_tag("department".to_string())
    )
    // 高分用户访问规则
    .add_rule(
        PolicyRule::new(
            "high-score-access".to_string(),
            "高分用户访问".to_string(),
            "评分高于80的用户可以访问".to_string(),
            60,
            PolicyAction::Allow,
        )
        .add_condition(PolicyCondition::new(
            "score".to_string(),
            ConditionOperator::GreaterThan,
            serde_json::Value::Number(serde_json::Number::from(80)),
            "评分检查".to_string(),
        ))
        .with_tag("performance".to_string())
    )
    // 拒绝访客规则
    .add_rule(
        PolicyRule::new(
            "deny-guest".to_string(),
            "拒绝访客".to_string(),
            "访客用户不能访问任何资源".to_string(),
            90,
            PolicyAction::Deny,
        )
        .add_condition(PolicyCondition::new(
            "role".to_string(),
            ConditionOperator::Equals,
            serde_json::Value::String("guest".to_string()),
            "角色检查".to_string(),
        ))
        .with_tag("security".to_string())
    )
}

#[tokio::test]
async fn test_policy_engine_basic_operations() {
    init_logging();

    let config = PolicyEngineConfig::default();
    let engine = CompletePolicyEngine::new(config);

    // 测试策略集合管理
    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    // 验证策略集合已添加
    let policy_sets = engine.list_policy_sets().await;
    assert_eq!(policy_sets.len(), 1);
    assert_eq!(policy_sets[0].id, "complex-access-control");
    assert_eq!(policy_sets[0].rules.len(), 4);

    // 测试按标签查找策略集合
    let admin_policies = engine.find_policy_sets_by_tag("admin").await;
    assert_eq!(admin_policies.len(), 1);

    let engineering_policies = engine.find_policy_sets_by_tag("department").await;
    assert_eq!(engineering_policies.len(), 1);

    // 验证统计信息
    let stats = engine.get_stats().await;
    assert_eq!(stats.policy_sets_count, 1);
    assert_eq!(stats.total_rules_count, 4);
    assert_eq!(stats.enabled_rules_count, 4);

    println!("✅ 策略引擎基本操作测试通过");
}

#[tokio::test]
async fn test_policy_evaluation_scenarios() {
    init_logging();

    let config = PolicyEngineConfig {
        enable_cache: true,
        enable_performance_monitoring: true,
        ..Default::default()
    };
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    // 场景1: 管理员用户
    let admin_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("admin".to_string()));

    let result = engine.evaluate("complex-access-control", &admin_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Allow);
    assert_eq!(result.matched_rules.len(), 1);
    assert_eq!(result.matched_rules[0].rule_id, "admin-full-access");
    println!("✅ 管理员用户评估: {:?}", result.final_action);

    // 场景2: 工程部门用户（年龄足够）
    let engineering_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("developer".to_string()));

    let result = engine.evaluate("complex-access-control", &engineering_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Allow);
    assert_eq!(result.matched_rules.len(), 1);
    assert_eq!(result.matched_rules[0].rule_id, "engineering-access");
    println!("✅ 工程部门用户评估: {:?}", result.final_action);

    // 场景3: 高分用户
    let high_score_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("user".to_string()))
        .with_field("department".to_string(), serde_json::Value::String("marketing".to_string()));

    let result = engine.evaluate("complex-access-control", &high_score_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Allow);
    assert_eq!(result.matched_rules.len(), 1);
    assert_eq!(result.matched_rules[0].rule_id, "high-score-access");
    println!("✅ 高分用户评估: {:?}", result.final_action);

    // 场景4: 访客用户（应该被拒绝）
    let guest_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("guest".to_string()))
        .with_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(95)));

    let result = engine.evaluate("complex-access-control", &guest_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Deny);
    assert_eq!(result.matched_rules.len(), 1);
    assert_eq!(result.matched_rules[0].rule_id, "deny-guest");
    println!("✅ 访客用户评估: {:?}", result.final_action);

    // 场景5: 普通用户（无匹配规则，使用默认动作）
    let normal_user_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("user".to_string()))
        .with_field("department".to_string(), serde_json::Value::String("sales".to_string()))
        .with_field("age".to_string(), serde_json::Value::Number(serde_json::Number::from(20)))
        .with_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(60)));

    let result = engine.evaluate("complex-access-control", &normal_user_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Deny); // 默认拒绝
    assert_eq!(result.matched_rules.len(), 0);
    println!("✅ 普通用户评估: {:?}", result.final_action);

    println!("✅ 策略评估场景测试通过");
}

#[tokio::test]
async fn test_caching_mechanism() {
    init_logging();

    let config = PolicyEngineConfig {
        enable_cache: true,
        cache_ttl: Duration::from_secs(5),
        max_cache_entries: 100,
        ..Default::default()
    };
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    let context = create_test_context();

    // 第一次评估
    let start = std::time::Instant::now();
    let result1 = engine.evaluate("complex-access-control", &context).await.unwrap();
    let first_time = start.elapsed();

    // 第二次评估（应该使用缓存）
    let start = std::time::Instant::now();
    let result2 = engine.evaluate("complex-access-control", &context).await.unwrap();
    let second_time = start.elapsed();

    // 验证结果一致性
    assert_eq!(result1.final_action, result2.final_action);
    assert_eq!(result1.matched_rules.len(), result2.matched_rules.len());

    // 验证缓存效果
    assert!(second_time < first_time, "缓存评估应该更快: {:?} vs {:?}", second_time, first_time);

    // 验证统计信息
    let stats = engine.get_stats().await;
    assert!(stats.cache_hits > 0, "应该有缓存命中");
    assert!(stats.cache_misses > 0, "应该有缓存未命中");

    println!("✅ 缓存机制测试通过");
    println!("   第一次评估耗时: {:?}", first_time);
    println!("   第二次评估耗时: {:?}", second_time);
    println!("   缓存命中率: {:.2}%", (stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64) * 100.0);
}

#[tokio::test]
async fn test_batch_evaluation() {
    init_logging();

    let config = PolicyEngineConfig::default();
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    // 准备批量评估数据
    let evaluations = vec![
        ("complex-access-control".to_string(), create_test_context()
            .with_field("role".to_string(), serde_json::Value::String("admin".to_string()))),
        ("complex-access-control".to_string(), create_test_context()
            .with_field("role".to_string(), serde_json::Value::String("developer".to_string()))),
        ("complex-access-control".to_string(), create_test_context()
            .with_field("role".to_string(), serde_json::Value::String("guest".to_string()))),
        ("complex-access-control".to_string(), create_test_context()
            .with_field("role".to_string(), serde_json::Value::String("user".to_string())
            .with_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(90)))),
    ];

    // 执行批量评估
    let start = std::time::Instant::now();
    let results = engine.evaluate_multiple(evaluations).await.unwrap();
    let total_time = start.elapsed();

    // 验证结果
    assert_eq!(results.len(), 4);

    // 验证各种场景的结果
    let mut allow_count = 0;
    let mut deny_count = 0;

    for (policy_set_id, result) in results {
        assert_eq!(policy_set_id, "complex-access-control");
        assert!(result.is_ok());

        let evaluation_result = result.unwrap();
        match evaluation_result.final_action {
            PolicyAction::Allow => allow_count += 1,
            PolicyAction::Deny => deny_count += 1,
            _ => {}
        }
    }

    assert_eq!(allow_count, 3); // admin, developer, high-score user
    assert_eq!(deny_count, 1);  // guest

    println!("✅ 批量评估测试通过");
    println!("   4个评估耗时: {:?}", total_time);
    println!("   允许: {}, 拒绝: {}", allow_count, deny_count);
}

#[tokio::test]
async fn test_performance_benchmarks() {
    init_logging();

    let config = PolicyEngineConfig {
        enable_cache: true,
        enable_performance_monitoring: true,
        ..Default::default()
    };
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    let context = create_test_context();

    // 性能测试：单次评估
    let iterations = 1000;
    let start = std::time::Instant::now();

    for i in 0..iterations {
        let mut test_context = context.clone();
        test_context.set_field(
            "iteration".to_string(),
            serde_json::Value::Number(serde_json::Number::from(i))
        );

        let _ = engine.evaluate("complex-access-control", &test_context).await.unwrap();
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations;

    println!("✅ 性能基准测试完成");
    println!("   {} 次评估总耗时: {:?}", iterations, total_time);
    println!("   平均每次评估耗时: {:?}", avg_time);
    println!("   每秒可处理评估次数: {:.0}", 1000.0 / avg_time.as_secs_f64());

    // 验证性能要求
    assert!(avg_time < TokioDuration::from_millis(1), "平均评估时间应小于1毫秒");

    // 验证统计信息
    let stats = engine.get_stats().await;
    assert_eq!(stats.total_evaluations, iterations as u64);
    assert!(stats.average_evaluation_time_us > 0);
    assert!(stats.slowest_evaluation_time_us >= stats.fastest_evaluation_time_us);

    println!("   统计信息:");
    println!("     总评估次数: {}", stats.total_evaluations);
    println!("     平均评估时间: {}μs", stats.average_evaluation_time_us);
    println!("     最快评估时间: {}μs", stats.fastest_evaluation_time_us);
    println!("     最慢评估时间: {}μs", stats.slowest_evaluation_time_us);
    println!("     缓存命中率: {:.2}%", (stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64) * 100.0);
}

#[tokio::test]
async fn test_policy_set_management() {
    init_logging();

    let config = PolicyEngineConfig::default();
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set.clone()).await.unwrap();

    // 测试启用/禁用策略集合
    engine.set_policy_set_enabled("complex-access-control", false).await.unwrap();

    let disabled_context = create_test_context()
        .with_field("role".to_string(), serde_json::Value::String("admin".to_string()));

    let result = engine.evaluate("complex-access-control", &disabled_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Deny); // 应该使用默认动作，因为策略集合被禁用

    // 重新启用策略集合
    engine.set_policy_set_enabled("complex-access-control", true).await.unwrap();

    let result = engine.evaluate("complex-access-control", &disabled_context).await.unwrap();
    assert_eq!(result.final_action, PolicyAction::Allow); // 应该允许访问

    // 测试移除策略集合
    engine.remove_policy_set("complex-access-control").await.unwrap();

    let policy_sets = engine.list_policy_sets().await;
    assert_eq!(policy_sets.len(), 0);

    println!("✅ 策略集合管理测试通过");
}

#[tokio::test]
async fn test_complex_condition_evaluation() {
    init_logging();

    // 测试正则表达式条件
    let email_context = PolicyContext::new()
        .with_field("email".to_string(), serde_json::Value::String("user@example.com".to_string()));

    let regex_condition = PolicyCondition::new(
        "email".to_string(),
        ConditionOperator::Regex,
        serde_json::Value::String(r".*@example\.com$".to_string()),
        "邮箱格式检查".to_string(),
    );

    assert!(regex_condition.evaluate(&email_context).unwrap());

    // 测试数组包含条件
    let permissions_context = PolicyContext::new()
        .with_field("permissions".to_string(), serde_json::Value::Array(vec![
            serde_json::Value::String("read".to_string()),
            serde_json::Value::String("write".to_string()),
            serde_json::Value::String("admin".to_string()),
        ]));

    let array_condition = PolicyCondition::new(
        "permissions".to_string(),
        ConditionOperator::In,
        serde_json::Value::String("admin".to_string()),
        "权限检查".to_string(),
    );

    assert!(array_condition.evaluate(&permissions_context).unwrap());

    // 测试数值比较条件
    let score_context = PolicyContext::new()
        .with_field("score".to_string(), serde_json::Value::Number(serde_json::Number::from(85)));

    let greater_condition = PolicyCondition::new(
        "score".to_string(),
        ConditionOperator::GreaterThan,
        serde_json::Value::Number(serde_json::Number::from(80)),
        "分数比较".to_string(),
    );

    assert!(greater_condition.evaluate(&score_context).unwrap());

    // 测试字符串包含条件
    let ip_context = PolicyContext::new()
        .with_field("ip_address".to_string(), serde_json::Value::String("192.168.1.100".to_string()));

    let contains_condition = PolicyCondition::new(
        "ip_address".to_string(),
        ConditionOperator::Contains,
        serde_json::Value::String("192.168".to_string()),
        "IP段检查".to_string(),
    );

    assert!(contains_condition.evaluate(&ip_context).unwrap());

    println!("✅ 复杂条件评估测试通过");
}

#[tokio::test]
async fn test_concurrent_policy_evaluation() {
    init_logging();

    let config = PolicyEngineConfig {
        enable_cache: true,
        ..Default::default()
    };
    let engine = std::sync::Arc::new(CompletePolicyEngine::new(config));

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    // 并发评估测试
    let concurrent_count = 50;
    let mut handles = Vec::new();

    for i in 0..concurrent_count {
        let engine_clone = std::sync::Arc::clone(&engine);
        let handle = tokio::spawn(async move {
            let context = create_test_context()
                .with_field("user_id".to_string(), serde_json::Value::String(format!("user-{}", i)))
                .with_field("role".to_string(),
                    if i % 3 == 0 {
                        serde_json::Value::String("admin".to_string())
                    } else if i % 3 == 1 {
                        serde_json::Value::String("developer".to_string())
                    } else {
                        serde_json::Value::String("user".to_string())
                    });

            let result = engine_clone.evaluate("complex-access-control", &context).await.unwrap();
            (i, result.final_action)
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    let mut results = Vec::new();
    for handle in handles {
        let (i, action) = handle.await.unwrap();
        results.push((i, action));
    }

    // 验证结果
    assert_eq!(results.len(), concurrent_count);

    let mut admin_count = 0;
    let mut developer_count = 0;
    let mut other_count = 0;

    for (i, action) in results {
        match i % 3 {
            0 => {
                assert_eq!(action, PolicyAction::Allow);
                admin_count += 1;
            }
            1 => {
                assert_eq!(action, PolicyAction::Allow);
                developer_count += 1;
            }
            _ => {
                // 其他用户可能因为高分被允许，或者被拒绝
                other_count += 1;
            }
        }
    }

    println!("✅ 并发策略评估测试通过");
    println!("   并发任务数: {}", concurrent_count);
    println!("   管理员用户: {} (全部允许)", admin_count);
    println!("   开发者用户: {} (全部允许)", developer_count);
    println!("   其他用户: {}", other_count);

    // 验证统计信息
    let stats = engine.get_stats().await;
    assert_eq!(stats.total_evaluations, concurrent_count as u64);
    println!("   总评估次数: {}", stats.total_evaluations);
}

#[tokio::test]
async fn test_statistics_tracking() {
    init_logging();

    let config = PolicyEngineConfig {
        enable_performance_monitoring: true,
        ..Default::default()
    };
    let engine = CompletePolicyEngine::new(config);

    let policy_set = create_complex_policy_set();
    engine.add_policy_set(policy_set).await.unwrap();

    // 执行一些操作来生成统计数据
    let context = create_test_context();

    for i in 0..10 {
        let mut test_context = context.clone();
        if i % 2 == 0 {
            test_context.set_field("role".to_string(), serde_json::Value::String("admin".to_string()));
        } else {
            test_context.set_field("role".to_string(), serde_json::Value::String("user".to_string()));
        }

        let _ = engine.evaluate("complex-access-control", &test_context).await.unwrap();
    }

    // 检查统计信息
    let stats = engine.get_stats().await;
    assert_eq!(stats.total_evaluations, 10);
    assert!(stats.average_evaluation_time_us > 0);

    // 重置统计信息
    engine.reset_stats().await;

    let reset_stats = engine.get_stats().await;
    assert_eq!(reset_stats.total_evaluations, 0);
    assert_eq!(reset_stats.average_evaluation_time_us, 0);

    println!("✅ 统计信息跟踪测试通过");
    println!("   重置前评估次数: {}", stats.total_evaluations);
    println!("   重置后评估次数: {}", reset_stats.total_evaluations);
}

#[tokio::test]
async fn test_error_handling() {
    init_logging();

    let engine = CompletePolicyEngine::new(PolicyEngineConfig::default());

    // 测试评估不存在的策略集合
    let context = create_test_context();
    let result = engine.evaluate("non-existent-policy", &context).await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.message.contains("策略集合不存在"));

    // 测试移除不存在的策略集合
    let remove_result = engine.remove_policy_set("non-existent-policy").await;
    assert!(remove_result.is_err());

    // 测试启用不存在的策略集合
    let enable_result = engine.set_policy_set_enabled("non-existent-policy", true).await;
    assert!(enable_result.is_err());

    println!("✅ 错误处理测试通过");
}