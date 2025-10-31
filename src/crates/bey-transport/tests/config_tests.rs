//! # mTLS 和连接池模块单元测试
//!
//! 测试 mTLS 配置和连接池配置模块

use bey_transport::mtls::{MtlsConfig, MtlsStats};
use bey_transport::pool::{CompleteConnectionPoolConfig, LoadBalanceStrategy};
use std::time::Duration;

// ============ mTLS 模块测试 ============

#[test]
fn test_mtls_config_default() {
    // 测试 mTLS 配置默认值
    let config = MtlsConfig::default();
    
    assert!(config.enabled);
    assert_eq!(config.certificates_dir.to_str().unwrap(), "./certs");
    assert!(config.enable_config_cache);
    assert_eq!(config.config_cache_ttl, Duration::from_secs(3600));
    assert_eq!(config.max_config_cache_entries, 100);
    assert_eq!(config.device_id_prefix, "bey");
    assert_eq!(config.organization_name, "BEY");
    assert_eq!(config.country_code, "CN");
}

#[test]
fn test_mtls_config_customization() {
    // 测试 mTLS 配置自定义
    let mut config = MtlsConfig::default();
    config.enabled = false;
    config.max_config_cache_entries = 200;
    config.organization_name = "TestOrg".to_string();
    
    assert!(!config.enabled);
    assert_eq!(config.max_config_cache_entries, 200);
    assert_eq!(config.organization_name, "TestOrg");
}

#[test]
fn test_mtls_stats_default() {
    // 测试 mTLS 统计默认值
    let stats = MtlsStats::default();
    
    assert_eq!(stats.config_generations, 0);
    assert_eq!(stats.config_cache_hits, 0);
    assert_eq!(stats.config_cache_misses, 0);
    assert_eq!(stats.certificate_renewals, 0);
    assert_eq!(stats.certificate_verifications, 0);
    assert_eq!(stats.connections_established, 0);
    assert_eq!(stats.connection_failures, 0);
}

#[test]
fn test_mtls_stats_increment() {
    // 测试 mTLS 统计递增
    let mut stats = MtlsStats::default();
    
    stats.config_generations += 1;
    stats.config_cache_hits += 5;
    stats.certificate_renewals += 2;
    
    assert_eq!(stats.config_generations, 1);
    assert_eq!(stats.config_cache_hits, 5);
    assert_eq!(stats.certificate_renewals, 2);
}

// ============ 连接池模块测试 ============

#[test]
fn test_pool_config_default() {
    // 测试连接池配置默认值
    let config = CompleteConnectionPoolConfig::default();
    
    assert_eq!(config.max_connections, 1000);
    assert_eq!(config.max_connections_per_addr, 10);
    assert_eq!(config.idle_timeout, Duration::from_secs(300));
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    assert_eq!(config.connect_timeout, Duration::from_secs(10));
    assert!(config.enable_warmup);
    assert_eq!(config.warmup_connections, 2);
    assert!(config.enable_connection_reuse);
}

#[test]
fn test_pool_config_customization() {
    // 测试连接池配置自定义
    let mut config = CompleteConnectionPoolConfig::default();
    config.max_connections = 500;
    config.max_connections_per_addr = 5;
    config.enable_warmup = false;
    
    assert_eq!(config.max_connections, 500);
    assert_eq!(config.max_connections_per_addr, 5);
    assert!(!config.enable_warmup);
}

#[test]
fn test_load_balance_strategy_types() {
    // 测试负载均衡策略类型
    let strategies = vec![
        LoadBalanceStrategy::RoundRobin,
        LoadBalanceStrategy::LeastConnections,
        LoadBalanceStrategy::ResponseTimeWeighted,
        LoadBalanceStrategy::Random,
        LoadBalanceStrategy::ConsistentHash,
        LoadBalanceStrategy::WeightedRoundRobin,
        LoadBalanceStrategy::LeastActiveRequests,
    ];
    
    assert_eq!(strategies.len(), 7);
}

#[test]
fn test_pool_config_with_custom_strategy() {
    // 测试连接池配置使用自定义负载均衡策略
    let mut config = CompleteConnectionPoolConfig::default();
    config.load_balance_strategy = LoadBalanceStrategy::Random;
    
    match config.load_balance_strategy {
        LoadBalanceStrategy::Random => assert!(true),
        _ => panic!("策略应该是 Random"),
    }
}

#[test]
fn test_pool_config_validation_thresholds() {
    // 测试连接池配置阈值
    let config = CompleteConnectionPoolConfig::default();
    
    assert!(config.utilization_threshold > 0.0);
    assert!(config.utilization_threshold <= 1.0);
    assert_eq!(config.utilization_threshold, 0.8);
}

#[test]
fn test_pool_config_queue_settings() {
    // 测试连接池队列设置
    let config = CompleteConnectionPoolConfig::default();
    
    assert_eq!(config.max_request_queue, 10000);
    assert!(config.enable_adaptive_sizing);
}
