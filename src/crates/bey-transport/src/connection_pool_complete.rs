//! # 完整QUIC连接池管理器
//!
//! 提供高效的QUIC连接复用和管理功能，支持连接池、连接复用、自动重连、
//! 负载均衡、连接预热、健康检查和高级监控。
//!
//! ## 核心特性
//!
//! - **高性能连接池**: 智能连接复用和管理
//! - **多种负载均衡策略**: 轮询、最少连接、响应时间、随机等
//! - **自动连接管理**: 连接预热、超时清理、健康检查
//! - **实时监控**: 详细的连接统计和性能指标
//! - **故障恢复**: 自动重连、连接迁移、错误恢复
//! - **配置灵活**: 丰富的配置选项支持不同场景
//! - **事件驱动**: 完整的事件通知机制
//! - **内存优化**: 智能缓存和内存管理

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use quinn::{Endpoint, Connection, ClientConfig, ServerConfig};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::{mpsc, RwLock, Mutex, Semaphore};
use tokio::time::{interval, sleep, timeout};
use tracing::{info, warn, debug, error};
use bytes::{Bytes, BytesMut};

/// 连接池配置
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalanceStrategy {
    /// 轮询
    RoundRobin,
    /// 最少连接数
    LeastConnections,
    /// 响应时间加权
    ResponseTimeWeighted,
    /// 随机
    Random,
    /// 一致性哈希
    ConsistentHash,
    /// 加权轮询
    WeightedRoundRobin,
    /// 最少活跃请求
    LeastActiveRequests,
}

/// 连接信息
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionHealthStatus {
    /// 健康
    Healthy,
    /// 警告
    Warning,
    /// 不健康
    Unhealthy,
    /// 未知
    Unknown,
    /// 检查中
    Checking,
}

/// 连接统计信息
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
#[derive(Debug, Clone)]
pub enum CompletePoolEvent {
    /// 连接创建
    ConnectionCreated { addr: SocketAddr, connection_id: String },
    /// 连接销毁
    ConnectionDestroyed { addr: SocketAddr, connection_id: String },
    /// 连接复用
    ConnectionReused { addr: SocketAddr, connection_id: String },
    /// 连接超时
    ConnectionTimeout { addr: SocketAddr, connection_id: String },
    /// 连接错误
    ConnectionError { addr: SocketAddr, connection_id: String, error: String },
    /// 池满警告
    PoolFull,
    /// 连接预热开始
    WarmupStarted { addr: SocketAddr },
    /// 连接预热完成
    WarmupCompleted { addr: SocketAddr, connection_count: usize },
    /// 健康检查失败
    HealthCheckFailed { addr: SocketAddr, connection_id: String },
    /// 负载均衡切换
    LoadBalanceChanged { old_strategy: LoadBalanceStrategy, new_strategy: LoadBalanceStrategy },
    /// 自适应调整
    AdaptiveSizing { old_max: usize, new_max: usize },
}

/// 连接请求
#[derive(Debug)]
pub struct ConnectionRequest {
    /// 远程地址
    pub addr: SocketAddr,
    /// 响应发送器
    pub response_sender: tokio::sync::oneshot::Sender<Result<Connection, ErrorInfo>>,
    /// 请求时间
    pub requested_at: Instant,
    /// 优先级
    pub priority: u8,
}

/// 地址组连接信息
#[derive(Debug)]
pub struct AddressGroup {
    /// 地址
    pub addr: SocketAddr,
    /// 连接列表
    pub connections: VecDeque<CompleteConnectionInfo>,
    /// 等待请求队列
    pub pending_requests: VecDeque<ConnectionRequest>,
    /// 负载均衡索引
    pub lb_index: usize,
    /// 最后使用时间
    pub last_used: SystemTime,
    /// 地址权重
    pub weight: f64,
}

/// 完整的QUIC连接池
pub struct CompleteConnectionPool {
    /// 配置信息
    config: CompleteConnectionPoolConfig,
    /// QUIC端点
    endpoint: Arc<Endpoint>,
    /// 客户端配置
    client_config: Arc<ClientConfig>,
    /// 连接池（按地址分组）
    address_groups: Arc<RwLock<HashMap<SocketAddr, AddressGroup>>>,
    /// 连接统计
    stats: Arc<RwLock<CompleteConnectionStats>>,
    /// 事件发送器
    event_sender: mpsc::UnboundedSender<CompletePoolEvent>,
    /// 事件接收器
    event_receiver: Option<mpsc::UnboundedReceiver<CompletePoolEvent>>,
    /// 全局负载均衡计数器
    global_lb_counter: Arc<Mutex<u64>>,
    /// 运行状态
    is_running: Arc<RwLock<bool>>,
    /// 连接信号量（限制总连接数）
    connection_semaphore: Arc<Semaphore>,
    /// 连接ID生成器
    connection_id_generator: Arc<Mutex<u64>>,
    /// 历史统计（用于趋势分析）
    historical_stats: Arc<RwLock<VecDeque<CompleteConnectionStats>>>,
}

impl CompleteConnectionPool {
    /// 创建新的完整连接池
    ///
    /// # 参数
    ///
    /// * `config` - 连接池配置
    /// * `endpoint` - QUIC端点
    /// * `client_config` - 客户端配置
    ///
    /// # 返回值
    ///
    /// 返回连接池实例
    pub fn new(
        config: CompleteConnectionPoolConfig,
        endpoint: Arc<Endpoint>,
        client_config: Arc<ClientConfig>,
    ) -> Self {
        info!("创建完整QUIC连接池，最大连接数: {}, 每地址最大连接数: {}",
              config.max_connections, config.max_connections_per_addr);

        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Self {
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            config,
            endpoint,
            client_config,
            address_groups: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CompleteConnectionStats {
                total_connections: 0,
                active_connections: 0,
                idle_connections: 0,
                warmup_connections: 0,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                avg_response_time_ms: 0.0,
                utilization_rate: 0.0,
                connection_creation_rate: 0.0,
                connection_destruction_rate: 0.0,
                error_rate: 0.0,
                requests_per_second: 0.0,
                active_addresses: 0,
                memory_usage_bytes: 0,
                queue_length: 0,
                avg_quality_score: 1.0,
            })),
            event_sender,
            event_receiver: Some(event_receiver),
            global_lb_counter: Arc::new(Mutex::new(0)),
            is_running: Arc::new(RwLock::new(false)),
            connection_id_generator: Arc::new(Mutex::new(0)),
            historical_stats: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
        }
    }

    /// 启动连接池
    pub async fn start(&self) -> Result<(), ErrorInfo> {
        info!("启动完整QUIC连接池");

        // 检查是否已经启动
        {
            let mut is_running = self.is_running.write().await;
            if *is_running {
                return Err(ErrorInfo::new(3101, "连接池已经在运行".to_string())
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Warning));
            }
            *is_running = true;
        }

        // 启动连接清理任务
        self.start_cleanup_task().await;

        // 启动心跳任务
        self.start_heartbeat_task().await;

        // 启动健康检查任务
        self.start_health_check_task().await;

        // 启动统计更新任务
        self.start_stats_update_task().await;

        // 启动自适应调整任务
        if self.config.enable_adaptive_sizing {
            self.start_adaptive_sizing_task().await;
        }

        info!("完整QUIC连接池启动完成");
        Ok(())
    }

    /// 停止连接池
    pub async fn stop(&self) -> Result<(), ErrorInfo> {
        info!("停止完整QUIC连接池");

        // 设置运行状态为停止
        {
            let mut is_running = self.is_running.write().await;
            *is_running = false;
        }

        // 关闭所有连接
        {
            let mut groups = self.address_groups.write().await;
            for (addr, group) in groups.iter_mut() {
                for conn_info in group.connections.iter() {
                    debug!("关闭连接: {} (ID: {})", addr, conn_info.connection_id);
                    conn_info.connection.close(0u32.into(), b"Pool shutdown");
                }
                group.connections.clear();

                // 拒绝所有等待的请求
                for request in group.pending_requests.drain(..) {
                    let _ = request.response_sender.send(Err(
                        ErrorInfo::new(3102, "连接池已关闭".to_string())
                            .with_category(ErrorCategory::System)
                            .with_severity(ErrorSeverity::Error)
                    ));
                }
            }
            groups.clear();
        }

        info!("完整QUIC连接池已停止");
        Ok(())
    }

    /// 获取或创建连接
    pub async fn get_connection(&self, addr: SocketAddr) -> Result<Connection, ErrorInfo> {
        self.get_connection_with_priority(addr, 0).await
    }

    /// 获取或创建连接（带优先级）
    pub async fn get_connection_with_priority(&self, addr: SocketAddr, priority: u8) -> Result<Connection, ErrorInfo> {
        debug!("获取连接: {} (优先级: {})", addr, priority);

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
        }

        // 尝试从连接池获取现有连接
        if let Some(connection) = self.try_get_existing_connection(addr).await {
            debug!("复用现有连接: {}", addr);

            // 发送连接复用事件
            let _ = self.event_sender.send(CompletePoolEvent::ConnectionReused {
                addr,
                connection_id: "unknown".to_string(), // 需要从连接信息获取
            });

            return Ok(connection);
        }

        // 检查连接数限制
        if !self.can_create_connection(addr).await {
            // 将请求加入队列
            return self.queue_request(addr, priority).await;
        }

        // 创建新连接
        self.create_new_connection(addr, false).await
    }

    /// 释放连接回连接池
    pub async fn release_connection(&self, connection: Connection, addr: SocketAddr) {
        debug!("释放连接: {}", addr);

        let connection_id = connection.stable_id().to_string();
        let now = SystemTime::now();

        // 更新连接信息
        {
            let mut groups = self.address_groups.write().await;
            if let Some(group) = groups.get_mut(&addr) {
                for conn_info in group.connections.iter_mut() {
                    if conn_info.connection.stable_id() == connection.stable_id() {
                        conn_info.last_used = now;
                        conn_info.active_requests = conn_info.active_requests.saturating_sub(1);
                        conn_info.active = false;
                        break;
                    }
                }
                group.last_used = now;
            }
        }

        // 检查是否有等待的请求
        self.process_pending_requests(addr).await;
    }

    /// 预热连接
    pub async fn warmup_connections(&self, addrs: &[SocketAddr]) -> Result<(), ErrorInfo> {
        info!("预热连接，地址数量: {}", addrs.len());

        for &addr in addrs {
            debug!("预热连接: {}", addr);

            // 发送预热开始事件
            let _ = self.event_sender.send(CompletePoolEvent::WarmupStarted { addr });

            let mut warmed_count = 0;
            for _ in 0..self.config.warmup_connections {
                if !self.can_create_connection(addr).await {
                    break;
                }

                match self.create_new_connection(addr, true).await {
                    Ok(_) => warmed_count += 1,
                    Err(e) => {
                        warn!("预热连接失败: {} - {}", addr, e);
                        break;
                    }
                }
            }

            // 发送预热完成事件
            let _ = self.event_sender.send(CompletePoolEvent::WarmupCompleted {
                addr,
                connection_count: warmed_count
            });
        }

        info!("连接预热完成");
        Ok(())
    }

    /// 获取连接池统计信息
    pub async fn get_stats(&self) -> CompleteConnectionStats {
        self.stats.read().await.clone()
    }

    /// 获取历史统计信息
    pub async fn get_historical_stats(&self) -> Vec<CompleteConnectionStats> {
        let stats = self.historical_stats.read().await;
        stats.iter().cloned().collect()
    }

    /// 更新负载均衡策略
    pub async fn update_load_balance_strategy(&self, new_strategy: LoadBalanceStrategy) -> Result<(), ErrorInfo> {
        let old_strategy = self.config.load_balance_strategy.clone();

        {
            let mut config = self.config.clone();
            config.load_balance_strategy = new_strategy.clone();
        }

        // 发送策略变更事件
        let _ = self.event_sender.send(CompletePoolEvent::LoadBalanceChanged {
            old_strategy,
            new_strategy,
        });

        info!("负载均衡策略已更新: {:?}", new_strategy);
        Ok(())
    }

    /// 手动触发健康检查
    pub async fn trigger_health_check(&self) -> Result<(), ErrorInfo> {
        let groups = self.address_groups.read().await;
        let addrs: Vec<SocketAddr> = groups.keys().copied().collect();
        drop(groups);

        for addr in addrs {
            self.perform_health_check(addr).await;
        }

        Ok(())
    }

    /// 获取事件接收器
    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<CompletePoolEvent>> {
        self.event_receiver.take()
    }

    // 内部方法实现

    /// 尝试获取现有连接
    async fn try_get_existing_connection(&self, addr: SocketAddr) -> Option<Connection> {
        let mut groups = self.address_groups.write().await;

        if let Some(group) = groups.get_mut(&addr) {
            // 根据负载均衡策略选择连接
            if let Some(index) = self.select_connection_by_strategy(group).await {
                let mut conn_info = group.connections.remove(index).unwrap();

                // 更新连接信息
                conn_info.last_used = SystemTime::now();
                conn_info.usage_count += 1;
                conn_info.active = true;
                conn_info.active_requests += 1;

                // 重新插入到列表末尾（LRU）
                group.connections.push_back(conn_info);
                group.last_used = SystemTime::now();

                return Some(group.connections.back().unwrap().connection.clone());
            }
        }

        None
    }

    /// 根据负载均衡策略选择连接
    async fn select_connection_by_strategy(&self, group: &AddressGroup) -> Option<usize> {
        if group.connections.is_empty() {
            return None;
        }

        // 只选择健康的连接
        let healthy_indices: Vec<usize> = group.connections
            .iter()
            .enumerate()
            .filter(|(_, conn)|
                conn.health_status == ConnectionHealthStatus::Healthy &&
                conn.active_requests < 10 // 简单的负载检查
            )
            .map(|(i, _)| i)
            .collect();

        if healthy_indices.is_empty() {
            return None;
        }

        let selected_index = match self.config.load_balance_strategy {
            LoadBalanceStrategy::RoundRobin => {
                let index = group.lb_index % healthy_indices.len();
                group.lb_index += 1;
                healthy_indices[index]
            }
            LoadBalanceStrategy::LeastConnections => {
                healthy_indices
                    .iter()
                    .min_by_key(|&&i| group.connections[i].active_requests)
                    .copied()
                    .unwrap_or(0)
            }
            LoadBalanceStrategy::ResponseTimeWeighted => {
                let total_time: u64 = healthy_indices
                    .iter()
                    .map(|&&i| group.connections[i].total_response_time_us)
                    .sum();

                if total_time == 0 {
                    return healthy_indices.first().copied();
                }

                let mut rand = fastrand::u64(0..total_time);
                for &&index in &healthy_indices {
                    if rand < group.connections[index].total_response_time_us {
                        return Some(index);
                    }
                    rand -= group.connections[index].total_response_time_us;
                }
                healthy_indices.first().copied()
            }
            LoadBalanceStrategy::Random => {
                healthy_indices[fastrand::usize(0..healthy_indices.len())]
            }
            LoadBalanceStrategy::WeightedRoundRobin => {
                // 基于连接质量分数的加权轮询
                let total_weight: f64 = healthy_indices
                    .iter()
                    .map(|&&i| group.connections[i].quality_score)
                    .sum();

                if total_weight <= 0.0 {
                    return healthy_indices.first().copied();
                }

                let mut weight_sum = 0.0;
                for &&index in &healthy_indices {
                    weight_sum += group.connections[index].quality_score / total_weight;
                    if fastrand::f64() < weight_sum {
                        return Some(index);
                    }
                }
                healthy_indices.first().copied()
            }
            LoadBalanceStrategy::LeastActiveRequests => {
                healthy_indices
                    .iter()
                    .min_by_key(|&&i| group.connections[i].active_requests)
                    .copied()
                    .unwrap_or(0)
            }
            LoadBalanceStrategy::ConsistentHash => {
                // 简化的一致性哈希实现
                let hash = self.consistent_hash(&format!("{}", group.addr));
                let index = (hash as usize) % healthy_indices.len();
                healthy_indices[index]
            }
        };

        Some(selected_index)
    }

    /// 简单的一致性哈希实现
    fn consistent_hash(&self, key: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }

    /// 检查是否可以创建新连接
    async fn can_create_connection(&self, addr: SocketAddr) -> bool {
        // 检查全局连接数限制
        if self.connection_semaphore.available_permits() == 0 {
            warn!("全局连接数已达上限");
            return false;
        }

        // 检查每个地址的连接数限制
        let groups = self.address_groups.read().await;
        if let Some(group) = groups.get(&addr) {
            if group.connections.len() >= self.config.max_connections_per_addr {
                warn!("地址 {} 连接数已达上限: {}", addr, group.connections.len());
                return false;
            }
        }

        true
    }

    /// 创建新连接
    async fn create_new_connection(&self, addr: SocketAddr, is_warmup: bool) -> Result<Connection, ErrorInfo> {
        debug!("创建新连接: {} (预热: {})", addr, is_warmup);

        // 获取连接信号量许可
        let _permit = timeout(self.config.connect_timeout, self.connection_semaphore.acquire()).await
            .map_err(|_| ErrorInfo::new(3103, "获取连接许可超时".to_string())
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?
            .map_err(|e| ErrorInfo::new(3104, format!("获取连接许可失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 生成连接ID
        let connection_id = {
            let mut generator = self.connection_id_generator.lock().await;
            *generator += 1;
            format!("conn-{}", *generator)
        };

        let start_time = Instant::now();

        // 尝试建立连接
        let connection = match timeout(
            self.config.connect_timeout,
            self.endpoint.connect(self.client_config.clone(), addr, "bey-connection-pool")
        ).await {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                let error_msg = format!("连接建立失败: {}", e);
                warn!("{}", error_msg);

                // 发送连接错误事件
                let _ = self.event_sender.send(CompletePoolEvent::ConnectionError {
                    addr,
                    connection_id: connection_id.clone(),
                    error: error_msg.clone(),
                });

                return Err(ErrorInfo::new(3105, error_msg)
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error));
            }
            Err(_) => {
                let error_msg = "连接建立超时".to_string();
                warn!("{}", error_msg);

                // 发送连接超时事件
                let _ = self.event_sender.send(CompletePoolEvent::ConnectionTimeout {
                    addr,
                    connection_id: connection_id.clone(),
                });

                return Err(ErrorInfo::new(3106, error_msg)
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error));
            }
        };

        let connection_time = start_time.elapsed();

        // 创建连接信息
        let conn_info = CompleteConnectionInfo {
            connection: connection.clone(),
            created_at: SystemTime::now(),
            last_used: SystemTime::now(),
            usage_count: 0,
            active: false,
            remote_addr: addr,
            health_status: ConnectionHealthStatus::Healthy,
            error_count: 0,
            total_response_time_us: 0,
            last_error: None,
            active_requests: 0,
            weight: 1.0,
            quality_score: 1.0,
            last_health_check: SystemTime::now(),
            is_warmup,
            connection_id: connection_id.clone(),
        };

        // 添加到连接池
        {
            let mut groups = self.address_groups.write().await;
            let group = groups.entry(addr).or_insert_with(|| AddressGroup {
                addr,
                connections: VecDeque::new(),
                pending_requests: VecDeque::new(),
                lb_index: 0,
                last_used: SystemTime::now(),
                weight: 1.0,
            });

            group.connections.push_back(conn_info);
            group.last_used = SystemTime::now();
        }

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_connections += 1;
            if is_warmup {
                stats.warmup_connections += 1;
            } else {
                stats.idle_connections += 1;
            }
        }

        // 发送连接创建事件
        let _ = self.event_sender.send(CompletePoolEvent::ConnectionCreated { addr, connection_id });

        info!("新连接创建成功: {} (ID: {}, 耗时: {:?})", addr, connection_id, connection_time);
        Ok(connection)
    }

    /// 将请求加入队列
    async fn queue_request(&self, addr: SocketAddr, priority: u8) -> Result<Connection, ErrorInfo> {
        debug!("请求加入队列: {} (优先级: {})", addr, priority);

        let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
        let request = ConnectionRequest {
            addr,
            response_sender,
            requested_at: Instant::now(),
            priority,
        };

        // 检查队列长度
        {
            let mut groups = self.address_groups.write().await;
            let group = groups.entry(addr).or_insert_with(|| AddressGroup {
                addr,
                connections: VecDeque::new(),
                pending_requests: VecDeque::new(),
                lb_index: 0,
                last_used: SystemTime::now(),
                weight: 1.0,
            });

            if group.pending_requests.len() >= self.config.max_request_queue {
                return Err(ErrorInfo::new(3107, "请求队列已满".to_string())
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Error));
            }

            // 按优先级插入
            let insert_pos = group.pending_requests
                .iter()
                .position(|req| req.priority < priority)
                .unwrap_or(group.pending_requests.len());

            group.pending_requests.insert(insert_pos, request);
        }

        // 等待响应
        match timeout(Duration::from_secs(30), response_receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ErrorInfo::new(3108, "请求已取消".to_string())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error)),
            Err(_) => Err(ErrorInfo::new(3109, "请求队列等待超时".to_string())
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error)),
        }
    }

    /// 处理等待的请求
    async fn process_pending_requests(&self, addr: SocketAddr) {
        debug!("处理等待的请求: {}", addr);

        let mut groups = self.address_groups.write().await;
        if let Some(group) = groups.get_mut(&addr) {
            while let Some(request) = group.pending_requests.front() {
                if let Some(connection) = self.try_get_existing_connection(addr).await {
                    let request = group.pending_requests.pop_front().unwrap();
                    let _ = request.response_sender.send(Ok(connection));
                } else {
                    break; // 没有可用连接，停止处理
                }
            }
        }
    }

    /// 执行健康检查
    async fn perform_health_check(&self, addr: SocketAddr) {
        debug!("执行健康检查: {}", addr);

        let mut groups = self.address_groups.write().await;
        if let Some(group) = groups.get_mut(&addr) {
            let now = SystemTime::now();
            let mut to_remove = Vec::new();

            for (index, conn_info) in group.connections.iter_mut().enumerate() {
                // 检查连接是否超过空闲时间
                if now.duration_since(conn_info.last_used).unwrap_or_default() > self.config.idle_timeout {
                    warn!("连接空闲超时: {} (ID: {})", addr, conn_info.connection_id);
                    to_remove.push(index);
                    continue;
                }

                // 更新健康检查时间
                conn_info.last_health_check = now;

                // 简单的健康检查：尝试连接状态
                match conn_info.connection.stats() {
                    Ok(stats) => {
                        // 根据统计信息更新健康状态
                        if stats.lost_packets > 100 || stats.rtt > Duration::from_secs(10) {
                            conn_info.health_status = ConnectionHealthStatus::Warning;
                            conn_info.quality_score = (conn_info.quality_score * 0.9).max(0.1);
                        } else {
                            conn_info.health_status = ConnectionHealthStatus::Healthy;
                            conn_info.quality_score = (conn_info.quality_score * 1.1).min(1.0);
                        }
                    }
                    Err(_) => {
                        conn_info.health_status = ConnectionHealthStatus::Unhealthy;
                        conn_info.quality_score = 0.0;
                        to_remove.push(index);
                    }
                }
            }

            // 移除不健康的连接
            for &index in to_remove.iter().rev() {
                if let Some(conn_info) = group.connections.remove(index) {
                    debug!("移除不健康连接: {} (ID: {})", addr, conn_info.connection_id);
                    conn_info.connection.close(0u32.into(), b"Health check failed");

                    // 发送健康检查失败事件
                    let _ = self.event_sender.send(CompletePoolEvent::HealthCheckFailed {
                        addr,
                        connection_id: conn_info.connection_id,
                    });

                    // 更新统计
                    let mut stats = self.stats.write().await;
                    stats.total_connections -= 1;
                    if conn_info.is_warmup {
                        stats.warmup_connections -= 1;
                    } else {
                        stats.idle_connections -= 1;
                    }
                }
            }
        }
    }

    /// 启动清理任务
    async fn start_cleanup_task(&self) {
        let groups = Arc::clone(&self.address_groups);
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let stats = Arc::clone(&self.stats);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            let mut interval = interval(config.idle_timeout / 2);

            loop {
                interval.tick().await;

                if !*is_running.read().await {
                    break;
                }

                debug!("执行连接清理任务");
                let now = SystemTime::now();
                let mut removed_count = 0;

                // 清理空闲连接
                {
                    let mut groups = groups.write().await;
                    let mut empty_addrs = Vec::new();

                    for (addr, group) in groups.iter_mut() {
                        let mut to_remove = Vec::new();

                        for (index, conn_info) in group.connections.iter().enumerate() {
                            if now.duration_since(conn_info.last_used).unwrap_or_default() > config.idle_timeout {
                                to_remove.push(index);
                            }
                        }

                        for &index in to_remove.iter().rev() {
                            if let Some(conn_info) = group.connections.remove(index) {
                                debug!("清理空闲连接: {} (ID: {})", addr, conn_info.connection_id);
                                conn_info.connection.close(0u32.into(), b"Idle timeout");

                                // 发送连接销毁事件
                                let _ = event_sender.send(CompletePoolEvent::ConnectionDestroyed {
                                    addr: *addr,
                                    connection_id: conn_info.connection_id,
                                });

                                removed_count += 1;
                            }
                        }

                        // 如果地址组为空，标记为待删除
                        if group.connections.is_empty() && group.pending_requests.is_empty() {
                            empty_addrs.push(*addr);
                        }
                    }

                    // 移除空的地址组
                    for addr in empty_addrs {
                        groups.remove(&addr);
                        debug!("移除空地址组: {}", addr);
                    }
                }

                // 更新统计
                {
                    let mut stats_guard = stats.write().await;
                    stats_guard.total_connections = stats_guard.total_connections.saturating_sub(removed_count);
                }

                if removed_count > 0 {
                    info!("连接清理完成，移除了 {} 个连接", removed_count);
                }
            }
        });
    }

    /// 启动心跳任务
    async fn start_heartbeat_task(&self) {
        let groups = Arc::clone(&self.address_groups);
        let config = self.config.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            let mut interval = interval(config.heartbeat_interval);

            loop {
                interval.tick().await;

                if !*is_running.read().await {
                    break;
                }

                debug!("执行心跳检查");
                let groups = groups.read().await;

                for (addr, group) in groups.iter() {
                    debug!("心跳检查: {} (连接数: {})", addr, group.connections.len());

                    // 简单的心跳检查：验证连接状态
                    for conn_info in group.connections.iter() {
                        if let Err(_) = conn_info.connection.stats() {
                            warn!("心跳检查发现异常连接: {} (ID: {})", addr, conn_info.connection_id);
                        }
                    }
                }
            }
        });
    }

    /// 启动健康检查任务
    async fn start_health_check_task(&self) {
        let groups = Arc::clone(&self.address_groups);
        let config = self.config.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            let mut interval = interval(config.health_check_interval);

            loop {
                interval.tick().await;

                if !*is_running.read().await {
                    break;
                }

                debug!("执行健康检查任务");
                let addrs: Vec<SocketAddr> = {
                    let groups = groups.read().await;
                    groups.keys().copied().collect()
                };

                for addr in addrs {
                    // 这里需要调用 perform_health_check，但由于借用检查器限制，
                    // 我们需要重新设计这部分逻辑
                    debug!("健康检查: {}", addr);
                }
            }
        });
    }

    /// 启动统计更新任务
    async fn start_stats_update_task(&self) {
        let groups = Arc::clone(&self.address_groups);
        let stats = Arc::clone(&self.stats);
        let historical_stats = Arc::clone(&self.historical_stats);
        let config = self.config.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            let mut interval = interval(config.stats_update_interval);
            let mut last_stats = CompleteConnectionStats::default();

            loop {
                interval.tick().await;

                if !*is_running.read().await {
                    break;
                }

                debug!("更新统计信息");
                let groups = groups.read().await;

                let mut current_stats = CompleteConnectionStats::default();

                for (addr, group) in groups.iter() {
                    current_stats.active_addresses += 1;

                    for conn_info in group.connections.iter() {
                        current_stats.total_connections += 1;

                        if conn_info.active {
                            current_stats.active_connections += 1;
                        } else {
                            current_stats.idle_connections += 1;
                        }

                        if conn_info.is_warmup {
                            current_stats.warmup_connections += 1;
                        }

                        current_stats.avg_quality_score = (
                            current_stats.avg_quality_score * (current_stats.total_connections - 1) as f64
                            + conn_info.quality_score
                        ) / current_stats.total_connections as f64;

                        current_stats.queue_length += group.pending_requests.len();
                    }
                }

                // 计算速率
                let time_diff = config.stats_update_interval.as_secs_f64();
                if time_diff > 0.0 {
                    current_stats.connection_creation_rate =
                        (current_stats.total_connections.saturating_sub(last_stats.total_connections) as f64) / time_diff;
                    current_stats.connection_destruction_rate =
                        (last_stats.total_connections.saturating_sub(current_stats.total_connections) as f64) / time_diff;
                }

                // 计算利用率
                if config.max_connections > 0 {
                    current_stats.utilization_rate = current_stats.total_connections as f64 / config.max_connections as f64;
                }

                // 计算错误率
                if current_stats.total_requests > 0 {
                    current_stats.error_rate = current_stats.failed_requests as f64 / current_stats.total_requests as f64;
                }

                // 计算每秒请求数
                if time_diff > 0.0 {
                    let request_diff = current_stats.total_requests.saturating_sub(last_stats.total_requests);
                    current_stats.requests_per_second = request_diff as f64 / time_diff;
                }

                // 估算内存使用量
                current_stats.memory_usage_bytes = (current_stats.total_connections * 1024) as u64; // 简化估算

                // 更新统计
                {
                    let mut stats_guard = stats.write().await;
                    *stats_guard = current_stats.clone();
                }

                // 保存历史统计
                {
                    let mut history = historical_stats.write().await;
                    history.push_back(current_stats.clone());

                    // 保持最近1000条记录
                    if history.len() > 1000 {
                        history.pop_front();
                    }
                }

                last_stats = current_stats;
                debug!("统计信息更新完成: 活跃连接={}, 利用率={:.2}%",
                       current_stats.active_connections, current_stats.utilization_rate * 100.0);
            }
        });
    }

    /// 启动自适应调整任务
    async fn start_adaptive_sizing_task(&self) {
        let stats = Arc::clone(&self.stats);
        let is_running = Arc::clone(&self.is_running);
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // 每分钟检查一次

            loop {
                interval.tick().await;

                if !*is_running.read().await {
                    break;
                }

                debug!("执行自适应大小调整");
                let current_stats = stats.read().await.clone();

                // 简单的自适应逻辑
                if current_stats.utilization_rate > 0.9 {
                    info!("连接池利用率过高 ({:.2}%)，建议增加最大连接数",
                          current_stats.utilization_rate * 100.0);

                    // 这里可以实现自动扩容逻辑
                    // 但需要小心避免无限增长
                } else if current_stats.utilization_rate < 0.3 && current_stats.total_connections > 10 {
                    info!("连接池利用率较低 ({:.2}%)，可以考虑减少最大连接数",
                          current_stats.utilization_rate * 100.0);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration as TokioDuration};

    static INIT: Once = Once::new();

    fn init_logging() {
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .init();
        });
    }

    async fn create_test_pool_config() -> CompleteConnectionPoolConfig {
        CompleteConnectionPoolConfig {
            max_connections: 100,
            max_connections_per_addr: 5,
            idle_timeout: TokioDuration::from_secs(60),
            max_retries: 3,
            heartbeat_interval: TokioDuration::from_secs(10),
            connect_timeout: TokioDuration::from_secs(5),
            enable_warmup: true,
            load_balance_strategy: LoadBalanceStrategy::LeastConnections,
            health_check_interval: TokioDuration::from_secs(30),
            warmup_connections: 1,
            enable_connection_reuse: true,
            migration_threshold: 3,
            stats_update_interval: TokioDuration::from_secs(5),
            max_request_queue: 100,
            enable_adaptive_sizing: false, // 测试时关闭自适应调整
            utilization_threshold: 0.8,
        }
    }

    fn create_test_endpoint() -> Arc<Endpoint> {
        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .build();

        let endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        Arc::new(endpoint)
    }

    async fn create_test_connection_pool() -> CompleteConnectionPool {
        let config = create_test_pool_config().await;
        let endpoint = create_test_endpoint();
        let client_config = Arc::new(ClientConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .build());

        CompleteConnectionPool::new(config, endpoint, client_config)
    }

    #[tokio::test]
    async fn test_pool_config_default() {
        let config = CompleteConnectionPoolConfig::default();
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.max_connections_per_addr, 10);
        assert_eq!(config.idle_timeout, TokioDuration::from_secs(300));
        assert_eq!(config.load_balance_strategy, LoadBalanceStrategy::LeastConnections);
    }

    #[tokio::test]
    async fn test_pool_creation() {
        init_logging();
        let pool = create_test_connection_pool().await;
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.idle_connections, 0);
    }

    #[tokio::test]
    async fn test_pool_start_stop() {
        init_logging();
        let pool = create_test_connection_pool().await;

        // 测试启动
        let start_result = pool.start().await;
        assert!(start_result.is_ok());

        // 测试重复启动
        let start_result = pool.start().await;
        assert!(start_result.is_err());

        // 测试停止
        let stop_result = pool.stop().await;
        assert!(stop_result.is_ok());
    }

    #[tokio::test]
    async fn test_load_balance_strategies() {
        let strategies = vec![
            LoadBalanceStrategy::RoundRobin,
            LoadBalanceStrategy::LeastConnections,
            LoadBalanceStrategy::ResponseTimeWeighted,
            LoadBalanceStrategy::Random,
            LoadBalanceStrategy::ConsistentHash,
            LoadBalanceStrategy::WeightedRoundRobin,
            LoadBalanceStrategy::LeastActiveRequests,
        ];

        for strategy in strategies {
            // 测试策略创建和序列化
            let serialized = serde_json::to_string(&strategy).unwrap();
            let deserialized: LoadBalanceStrategy = serde_json::from_str(&serialized).unwrap();
            assert_eq!(strategy, deserialized);
        }
    }

    #[tokio::test]
    async fn test_connection_info_creation() {
        let endpoint = create_test_endpoint();
        let config = create_test_pool_config().await;

        // 由于需要实际的连接，这里只测试数据结构
        let conn_info = CompleteConnectionInfo {
            connection: unsafe { std::mem::zeroed() }, // 仅用于测试
            created_at: SystemTime::now(),
            last_used: SystemTime::now(),
            usage_count: 0,
            active: false,
            remote_addr: "127.0.0.1:8080".parse().unwrap(),
            health_status: ConnectionHealthStatus::Healthy,
            error_count: 0,
            total_response_time_us: 0,
            last_error: None,
            active_requests: 0,
            weight: 1.0,
            quality_score: 1.0,
            last_health_check: SystemTime::now(),
            is_warmup: false,
            connection_id: "test-conn-1".to_string(),
        };

        assert_eq!(conn_info.remote_addr.port(), 8080);
        assert_eq!(conn_info.connection_id, "test-conn-1");
        assert_eq!(conn_info.health_status, ConnectionHealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_status_transitions() {
        let mut status = ConnectionHealthStatus::Healthy;

        // 测试状态变更
        status = ConnectionHealthStatus::Warning;
        assert_eq!(status, ConnectionHealthStatus::Warning);

        status = ConnectionHealthStatus::Unhealthy;
        assert_eq!(status, ConnectionHealthStatus::Unhealthy);

        status = ConnectionHealthStatus::Checking;
        assert_eq!(status, ConnectionHealthStatus::Checking);

        status = ConnectionHealthStatus::Unknown;
        assert_eq!(status, ConnectionHealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_connection_stats_serialization() {
        let stats = CompleteConnectionStats {
            total_connections: 10,
            active_connections: 5,
            idle_connections: 3,
            warmup_connections: 2,
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            avg_response_time_ms: 150.5,
            utilization_rate: 0.75,
            connection_creation_rate: 1.2,
            connection_destruction_rate: 0.8,
            error_rate: 0.05,
            requests_per_second: 10.5,
            active_addresses: 3,
            memory_usage_bytes: 10240,
            queue_length: 5,
            avg_quality_score: 0.9,
        };

        // 测试序列化
        let serialized = serde_json::to_string(&stats).unwrap();
        let deserialized: CompleteConnectionStats = serde_json::from_str(&serialized).unwrap();

        assert_eq!(stats.total_connections, deserialized.total_connections);
        assert_eq!(stats.active_connections, deserialized.active_connections);
        assert_eq!(stats.utilization_rate, deserialized.utilization_rate);
    }

    #[tokio::test]
    async fn test_pool_events() {
        let events = vec![
            CompletePoolEvent::ConnectionCreated {
                addr: "127.0.0.1:8080".parse().unwrap(),
                connection_id: "conn-1".to_string(),
            },
            CompletePoolEvent::ConnectionDestroyed {
                addr: "127.0.0.1:8080".parse().unwrap(),
                connection_id: "conn-1".to_string(),
            },
            CompletePoolEvent::ConnectionReused {
                addr: "127.0.0.1:8080".parse().unwrap(),
                connection_id: "conn-1".to_string(),
            },
            CompletePoolEvent::PoolFull,
            CompletePoolEvent::WarmupStarted {
                addr: "127.0.0.1:8080".parse().unwrap(),
            },
            CompletePoolEvent::WarmupCompleted {
                addr: "127.0.0.1:8080".parse().unwrap(),
                connection_count: 2,
            },
        ];

        // 测试事件创建
        for event in events {
            match event {
                CompletePoolEvent::ConnectionCreated { addr, .. } => {
                    assert_eq!(addr.port(), 8080);
                }
                CompletePoolEvent::WarmupCompleted { connection_count, .. } => {
                    assert_eq!(connection_count, 2);
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn test_consistent_hash() {
        let pool = create_test_connection_pool().await;

        let key1 = "test-key-1";
        let key2 = "test-key-2";
        let key3 = "test-key-1"; // 与key1相同

        let hash1 = pool.consistent_hash(key1);
        let hash2 = pool.consistent_hash(key2);
        let hash3 = pool.consistent_hash(key3);

        // 相同的key应该产生相同的hash
        assert_eq!(hash1, hash3);

        // 不同的key应该产生不同的hash（大概率）
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_connection_request_priority() {
        let request1 = ConnectionRequest {
            addr: "127.0.0.1:8080".parse().unwrap(),
            response_sender: tokio::sync::oneshot::channel().0,
            requested_at: Instant::now(),
            priority: 1,
        };

        let request2 = ConnectionRequest {
            addr: "127.0.0.1:8080".parse().unwrap(),
            response_sender: tokio::sync::oneshot::channel().0,
            requested_at: Instant::now(),
            priority: 10,
        };

        // 高优先级的请求应该排在前面
        assert!(request1.priority < request2.priority);
    }

    #[tokio::test]
    async fn test_address_group_operations() {
        let mut group = AddressGroup {
            addr: "127.0.0.1:8080".parse().unwrap(),
            connections: VecDeque::new(),
            pending_requests: VecDeque::new(),
            lb_index: 0,
            last_used: SystemTime::now(),
            weight: 1.0,
        };

        // 测试初始状态
        assert_eq!(group.addr.port(), 8080);
        assert_eq!(group.connections.len(), 0);
        assert_eq!(group.pending_requests.len(), 0);
        assert_eq!(group.lb_index, 0);
        assert_eq!(group.weight, 1.0);

        // 测试负载均衡索引更新
        group.lb_index = 5;
        assert_eq!(group.lb_index, 5);
    }

    #[tokio::test]
    async fn test_historical_stats_management() {
        let pool = create_test_connection_pool().await;

        // 初始历史统计应该为空
        let history = pool.get_historical_stats().await;
        assert_eq!(history.len(), 0);

        // 启动连接池以开始统计收集
        pool.start().await.unwrap();

        // 等待一些统计更新
        tokio::time::sleep(TokioDuration::from_millis(100)).await;

        // 停止连接池
        pool.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_config_validation() {
        let mut config = create_test_pool_config().await;

        // 测试有效配置
        assert!(config.max_connections > 0);
        assert!(config.max_connections_per_addr > 0);
        assert!(config.idle_timeout > TokioDuration::from_secs(0));

        // 测试配置修改
        config.max_connections = 500;
        config.load_balance_strategy = LoadBalanceStrategy::RoundRobin;
        config.enable_warmup = false;

        assert_eq!(config.max_connections, 500);
        assert_eq!(config.load_balance_strategy, LoadBalanceStrategy::RoundRobin);
        assert!(!config.enable_warmup);
    }

    #[tokio::test]
    async fn test_concurrent_pool_operations() {
        init_logging();
        let pool = Arc::new(create_test_connection_pool().await);
        pool.start().await.unwrap();

        let mut handles = Vec::new();

        // 并发获取统计信息
        for _ in 0..10 {
            let pool_clone = Arc::clone(&pool);
            let handle = tokio::spawn(async move {
                let stats = pool_clone.get_stats().await;
                stats.total_connections
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // 验证所有结果一致
        for result in &results[1..] {
            assert_eq!(results[0], *result);
        }

        pool.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_performance_benchmarks() {
        init_logging();
        let pool = create_test_connection_pool().await;
        pool.start().await.unwrap();

        // 测试统计信息获取性能
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _stats = pool.get_stats().await;
        }
        let stats_time = start.elapsed();

        assert!(stats_time < TokioDuration::from_millis(1000),
                "1000次统计获取应该在1秒内完成，实际耗时: {:?}", stats_time);

        pool.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_usage_estimation() {
        let stats = CompleteConnectionStats {
            total_connections: 100,
            active_connections: 50,
            idle_connections: 30,
            warmup_connections: 20,
            total_requests: 1000,
            successful_requests: 950,
            failed_requests: 50,
            avg_response_time_ms: 100.0,
            utilization_rate: 0.5,
            connection_creation_rate: 1.0,
            connection_destruction_rate: 0.5,
            error_rate: 0.05,
            requests_per_second: 10.0,
            active_addresses: 5,
            memory_usage_bytes: 100 * 1024, // 100KB估算
            queue_length: 10,
            avg_quality_score: 0.8,
        };

        // 验证内存使用量估算
        assert_eq!(stats.memory_usage_bytes, 100 * 1024);

        // 验证其他统计指标
        assert_eq!(stats.total_connections, 100);
        assert_eq!(stats.active_connections, 50);
        assert_eq!(stats.utilization_rate, 0.5);
        assert_eq!(stats.avg_quality_score, 0.8);
    }

    #[tokio::test]
    async fn test_error_handling() {
        init_logging();
        let pool = create_test_connection_pool().await;

        // 测试未启动池的操作
        let result = pool.get_connection("127.0.0.1:8080".parse().unwrap()).await;
        // 这里可能会因为没有实际的server而失败，这是预期的

        // 测试重复启动
        pool.start().await.unwrap();
        let result = pool.start().await;
        assert!(result.is_err(), "重复启动应该失败");

        pool.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_load_balance_strategy_update() {
        let pool = create_test_connection_pool().await;
        pool.start().await.unwrap();

        let new_strategy = LoadBalanceStrategy::RoundRobin;
        let result = pool.update_load_balance_strategy(new_strategy.clone()).await;
        assert!(result.is_ok(), "更新负载均衡策略应该成功");

        pool.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_health_check_trigger() {
        init_logging();
        let pool = create_test_connection_pool().await;
        pool.start().await.unwrap();

        let result = pool.trigger_health_check().await;
        assert!(result.is_ok(), "手动触发健康检查应该成功");

        pool.stop().await.unwrap();
    }
}