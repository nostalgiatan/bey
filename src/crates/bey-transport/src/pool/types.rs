//! # 连接池类型定义模块
//!
//! 定义连接池使用的各种数据结构和类型

use error::ErrorInfo;
use quinn::Connection;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{SystemTime, Instant};
use super::config::LoadBalanceStrategy;

/// 连接信息
///
/// 存储单个连接的详细信息，包括连接对象、状态、统计数据等
#[derive(Debug, Clone)]
pub struct CompleteConnectionInfo {
    /// 连接对象
    pub connection: Connection,
    /// 创建时间
    pub created_at: SystemTime,
    /// 最后使用时间
    pub last_used: SystemTime,
    /// 使用次数
    pub usage_count: u64,
    /// 是否活跃
    pub active: bool,
    /// 远程地址
    pub remote_addr: SocketAddr,
    /// 连接健康状态
    pub health_status: ConnectionHealthStatus,
    /// 错误计数
    pub error_count: u64,
    /// 总响应时间（微秒）
    pub total_response_time_us: u64,
    /// 最后错误信息
    pub last_error: Option<String>,
    /// 活跃请求数
    pub active_requests: u32,
    /// 权重（用于负载均衡）
    pub weight: f64,
    /// 连接质量分数
    pub quality_score: f64,
    /// 最后健康检查时间
    pub last_health_check: SystemTime,
    /// 是否为预热连接
    pub is_warmup: bool,
    /// 连接ID
    pub connection_id: String,
}

/// 连接健康状态
///
/// 表示连接的健康程度
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionHealthStatus {
    /// 健康状态
    Healthy,
    /// 警告状态
    Warning,
    /// 不健康状态
    Unhealthy,
    /// 未知状态
    Unknown,
    /// 检查中状态
    Checking,
}

/// 连接统计信息
///
/// 提供连接池的全面统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteConnectionStats {
    /// 总连接数
    pub total_connections: usize,
    /// 活跃连接数
    pub active_connections: usize,
    /// 空闲连接数
    pub idle_connections: usize,
    /// 预热连接数
    pub warmup_connections: usize,
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数
    pub successful_requests: u64,
    /// 失败请求数
    pub failed_requests: u64,
    /// 平均响应时间（毫秒）
    pub avg_response_time_ms: f64,
    /// 连接池利用率
    pub utilization_rate: f64,
    /// 连接创建速率（每秒）
    pub connection_creation_rate: f64,
    /// 连接销毁速率（每秒）
    pub connection_destruction_rate: f64,
    /// 错误率
    pub error_rate: f64,
    /// 每秒请求数
    pub requests_per_second: f64,
    /// 活跃地址数
    pub active_addresses: usize,
    /// 内存使用量（字节）
    pub memory_usage_bytes: u64,
    /// 队列长度
    pub queue_length: usize,
    /// 平均连接质量分数
    pub avg_quality_score: f64,
}

/// 连接池事件
///
/// 连接池运行过程中产生的各种事件
#[derive(Debug, Clone)]
pub enum CompletePoolEvent {
    /// 连接创建事件
    ConnectionCreated { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String 
    },
    /// 连接销毁事件
    ConnectionDestroyed { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String 
    },
    /// 连接复用事件
    ConnectionReused { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String 
    },
    /// 连接超时事件
    ConnectionTimeout { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String 
    },
    /// 连接错误事件
    ConnectionError { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String, 
        /// 错误信息
        error: String 
    },
    /// 池满警告事件
    PoolFull,
    /// 连接预热开始事件
    WarmupStarted { 
        /// 远程地址
        addr: SocketAddr 
    },
    /// 连接预热完成事件
    WarmupCompleted { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接数量
        connection_count: usize 
    },
    /// 健康检查失败事件
    HealthCheckFailed { 
        /// 远程地址
        addr: SocketAddr, 
        /// 连接ID
        connection_id: String 
    },
    /// 负载均衡策略切换事件
    LoadBalanceChanged { 
        /// 旧策略
        old_strategy: LoadBalanceStrategy, 
        /// 新策略
        new_strategy: LoadBalanceStrategy 
    },
    /// 自适应调整事件
    AdaptiveSizing { 
        /// 旧的最大连接数
        old_max: usize, 
        /// 新的最大连接数
        new_max: usize 
    },
}

/// 连接请求
///
/// 表示一个对连接的请求
#[derive(Debug)]
pub struct ConnectionRequest {
    /// 远程地址
    pub addr: SocketAddr,
    /// 响应发送器
    pub response_sender: tokio::sync::oneshot::Sender<Result<Connection, ErrorInfo>>,
    /// 请求时间
    pub requested_at: Instant,
    /// 优先级（数值越大优先级越高）
    pub priority: u8,
}

/// 地址组连接信息
///
/// 管理同一地址的多个连接
#[derive(Debug)]
pub struct AddressGroup {
    /// 地址
    pub addr: SocketAddr,
    /// 连接列表
    pub connections: VecDeque<CompleteConnectionInfo>,
    /// 等待请求队列
    pub pending_requests: VecDeque<ConnectionRequest>,
    /// 负载均衡索引（用于轮询）
    pub lb_index: usize,
    /// 最后使用时间
    pub last_used: SystemTime,
    /// 地址权重（用于加权负载均衡）
    pub weight: f64,
}
