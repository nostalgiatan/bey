//! # 连接池配置模块
//!
//! 定义连接池的配置选项和负载均衡策略

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 连接池配置
///
/// 提供连接池的各种配置选项，包括连接数限制、超时设置、负载均衡策略等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteConnectionPoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 每个地址的最大连接数
    pub max_connections_per_addr: usize,
    /// 空闲超时时间
    pub idle_timeout: Duration,
    /// 连接重试次数
    pub max_retries: u32,
    /// 心跳间隔
    pub heartbeat_interval: Duration,
    /// 连接建立超时
    pub connect_timeout: Duration,
    /// 是否启用连接预热
    pub enable_warmup: bool,
    /// 负载均衡策略
    pub load_balance_strategy: LoadBalanceStrategy,
    /// 健康检查间隔
    pub health_check_interval: Duration,
    /// 连接预热数量
    pub warmup_connections: usize,
    /// 是否启用连接复用
    pub enable_connection_reuse: bool,
    /// 连接迁移阈值
    pub migration_threshold: u32,
    /// 统计更新间隔
    pub stats_update_interval: Duration,
    /// 最大请求队列长度
    pub max_request_queue: usize,
    /// 是否启用自适应连接数调整
    pub enable_adaptive_sizing: bool,
    /// 连接池利用率阈值
    pub utilization_threshold: f64,
}

impl Default for CompleteConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            max_connections_per_addr: 10,
            idle_timeout: Duration::from_secs(300), // 5分钟
            max_retries: 3,
            heartbeat_interval: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            enable_warmup: true,
            load_balance_strategy: LoadBalanceStrategy::LeastConnections,
            health_check_interval: Duration::from_secs(60),
            warmup_connections: 2,
            enable_connection_reuse: true,
            migration_threshold: 5,
            stats_update_interval: Duration::from_secs(10),
            max_request_queue: 10000,
            enable_adaptive_sizing: true,
            utilization_threshold: 0.8,
        }
    }
}

/// 负载均衡策略
///
/// 支持多种负载均衡算法以适应不同的使用场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalanceStrategy {
    /// 轮询策略
    RoundRobin,
    /// 最少连接数策略
    LeastConnections,
    /// 响应时间加权策略
    ResponseTimeWeighted,
    /// 随机选择策略
    Random,
    /// 一致性哈希策略
    ConsistentHash,
    /// 加权轮询策略
    WeightedRoundRobin,
    /// 最少活跃请求策略
    LeastActiveRequests,
}
