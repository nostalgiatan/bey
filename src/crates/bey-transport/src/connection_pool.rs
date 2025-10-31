//! # QUIC连接池管理器
//!
//! 提供高效的QUIC连接复用和管理功能
//! 支持连接池、连接复用、自动重连和负载均衡

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use quinn::{Endpoint, Connection, ClientConfig, ServerConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{interval, sleep};
use tracing::{info, warn, debug, error};

/// 连接池配置
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// 最大连接数
    pub max_connections: usize,
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
}

/// 负载均衡策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalanceStrategy {
    /// 轮询
    RoundRobin,
    /// 最少连接数
    LeastConnections,
    /// 响应时间
    ResponseTime,
    /// 随机
    Random,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            idle_timeout: Duration::from_secs(300), // 5分钟
            max_retries: 3,
            heartbeat_interval: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            enable_warmup: true,
            load_balance_strategy: LoadBalanceStrategy::LeastConnections,
        }
    }
}

/// 连接信息
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
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
}

/// 连接健康状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionHealthStatus {
    /// 健康
    Healthy,
    /// 警告
    Warning,
    /// 不健康
    Unhealthy,
    /// 未知
    Unknown,
}

/// 连接统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    /// 总连接数
    pub total_connections: usize,
    /// 活跃连接数
    pub active_connections: usize,
    /// 空闲连接数
    pub idle_connections: usize,
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
}

/// 连接池事件
#[derive(Debug, Clone)]
pub enum PoolEvent {
    /// 连接创建
    ConnectionCreated(SocketAddr),
    /// 连接销毁
    ConnectionDestroyed(SocketAddr),
    /// 连接复用
    ConnectionReused(SocketAddr),
    /// 连接超时
    ConnectionTimeout(SocketAddr),
    /// 连接错误
    ConnectionError(SocketAddr, String),
    /// 池满警告
    PoolFull,
}

/// QUIC连接池
pub struct ConnectionPool {
    /// 配置信息
    config: ConnectionPoolConfig,
    /// QUIC端点
    endpoint: Arc<Endpoint>,
    /// 客户端配置
    client_config: Arc<ClientConfig>,
    /// 连接池（按地址分组）
    connections: Arc<RwLock<HashMap<SocketAddr, Vec<ConnectionInfo>>>>,
    /// 连接统计
    stats: Arc<RwLock<ConnectionStats>>,
    /// 事件发送器
    event_sender: mpsc::UnboundedSender<PoolEvent>,
    /// 事件接收器
    event_receiver: Option<mpsc::UnboundedReceiver<PoolEvent>>,
    /// 负载均衡计数器
    lb_counter: Arc<Mutex<u64>>,
    /// 运行状态
    is_running: Arc<RwLock<bool>>,
}

impl ConnectionPool {
    /// 创建新的连接池
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
        config: ConnectionPoolConfig,
        endpoint: Arc<Endpoint>,
        client_config: Arc<ClientConfig>,
    ) -> Self {
        info!("创建QUIC连接池，最大连接数: {}", config.max_connections);

        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Self {
            config,
            endpoint,
            client_config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ConnectionStats {
                total_connections: 0,
                active_connections: 0,
                idle_connections: 0,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                avg_response_time_ms: 0.0,
                utilization_rate: 0.0,
            })),
            event_sender,
            event_receiver: Some(event_receiver),
            lb_counter: Arc::new(Mutex::new(0)),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动连接池
    ///
    /// # 返回值
    ///
    /// 返回启动结果或错误信息
    pub async fn start(&self) -> Result<(), ErrorInfo> {
        info!("启动QUIC连接池");

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

        // 启动统计更新任务
        self.start_stats_update_task().await;

        info!("QUIC连接池启动完成");
        Ok(())
    }

    /// 停止连接池
    pub async fn stop(&self) -> Result<(), ErrorInfo> {
        info!("停止QUIC连接池");

        // 设置运行状态为停止
        let mut is_running = self.is_running.write().await;
        *is_running = false;

        // 关闭所有连接
        {
            let mut connections = self.connections.write().await;
            for (addr, conn_list) in connections.iter_mut() {
                for conn_info in conn_list.iter() {
                    debug!("关闭连接: {}", addr);
                    // 关闭连接
                    conn_info.connection.close(0u32.into(), b"Pool shutdown");
                }
            }
            connections.clear();
        }

        info!("QUIC连接池已停止");
        Ok(())
    }

    /// 获取或创建连接
    ///
    /// # 参数
    ///
    /// * `addr` - 远程地址
    ///
    /// # 返回值
    ///
    /// 返回连接或错误信息
    pub async fn get_connection(&self, addr: SocketAddr) -> Result<Connection, ErrorInfo> {
        debug!("获取连接: {}", addr);

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
        }

        // 尝试从连接池获取现有连接
        if let Some(connection) = self.try_get_existing_connection(addr).await {
            debug!("复用现有连接: {}", addr);

            // 发送连接复用事件
            let _ = self.event_sender.send(PoolEvent::ConnectionReused(addr));

            return Ok(connection);
        }

        // 创建新连接
        self.create_new_connection(addr).await
    }

    /// 释放连接回连接池
    ///
    /// # 参数
    ///
    /// * `connection` - 要释放的连接
    /// * `addr` - 远程地址
    pub async fn release_connection(&self, connection: Connection, addr: SocketAddr) {
        debug!("释放连接: {}", addr);

        // 更新连接使用信息
        {
            let mut connections = self.connections.write().await;
            if let Some(conn_list) = connections.get_mut(&addr) {
                for conn_info in conn_list.iter_mut() {
                    if conn_info.active {
                        conn_info.last_used = SystemTime::now();
                        conn_info.usage_count += 1;
                        conn_info.active = false;
                        break;
                    }
                }
            }
        }

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.successful_requests += 1;
        }
    }

    /// 获取连接池统计信息
    ///
    /// # 返回值
    ///
    /// 返回统计信息
    pub async fn get_stats(&self) -> ConnectionStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// 获取下一个连接池事件
    ///
    /// # 返回值
    ///
    /// 返回事件或None（如果通道关闭）
    pub async fn next_event(&mut self) -> Option<PoolEvent> {
        match &mut self.event_receiver {
            Some(receiver) => receiver.recv().await,
            None => None,
        }
    }

    /// 根据负载均衡策略选择地址
    ///
    /// # 参数
    ///
    /// * `addresses` - 候选地址列表
    ///
    /// # 返回值
    ///
    /// 返回选择的地址
    pub async fn select_address(&self, addresses: &[SocketAddr]) -> Option<SocketAddr> {
        if addresses.is_empty() {
            return None;
        }

        match self.config.load_balance_strategy {
            LoadBalanceStrategy::RoundRobin => {
                let mut counter = self.lb_counter.lock().await;
                let index = (*counter as usize) % addresses.len();
                *counter += 1;
                Some(addresses[index])
            }
            LoadBalanceStrategy::LeastConnections => {
                let connections = self.connections.read().await;
                let mut min_connections = usize::MAX;
                let mut selected_addr = None;

                for &addr in addresses {
                    let conn_count = connections.get(&addr).map(|list| list.len()).unwrap_or(0);
                    if conn_count < min_connections {
                        min_connections = conn_count;
                        selected_addr = Some(addr);
                    }
                }

                selected_addr
            }
            LoadBalanceStrategy::Random => {
                let index = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as usize) % addresses.len();
                Some(addresses[index])
            }
            LoadBalanceStrategy::ResponseTime => {
                // 简化实现：使用轮询
                self.select_address(&addresses[..1]).await
            }
        }
    }

    // 私有方法

    /// 尝试获取现有连接
    async fn try_get_existing_connection(&self, addr: SocketAddr) -> Option<Connection> {
        let mut connections = self.connections.write().await;

        if let Some(conn_list) = connections.get_mut(&addr) {
            // 查找空闲连接
            for i in 0..conn_list.len() {
                if !conn_list[i].active {
                    let conn_info = conn_list.swap_remove(i);
                    conn_info.active = true;
                    return Some(conn_info.connection);
                }
            }
        }

        None
    }

    /// 创建新连接
    async fn create_new_connection(&self, addr: SocketAddr) -> Result<Connection, ErrorInfo> {
        debug!("创建新连接: {}", addr);

        // 检查连接数限制
        {
            let connections = self.connections.read().await;
            let total_connections: usize = connections.values().map(|list| list.len()).sum();
            if total_connections >= self.config.max_connections {
                warn!("连接池已满，最大连接数: {}", self.config.max_connections);
                let _ = self.event_sender.send(PoolEvent::PoolFull);
                return Err(ErrorInfo::new(3102, "连接池已满".to_string())
                    .with_category(ErrorCategory::Resource)
                    .with_severity(ErrorSeverity::Warning));
            }
        }

        // 建立连接（带超时）
        let connection = tokio::time::timeout(
            self.config.connect_timeout,
            self.endpoint.connect(addr, "bey.local")
        )
        .await
        .map_err(|_| ErrorInfo::new(3103, "连接建立超时".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Error))?
        .map_err(|e| ErrorInfo::new(3104, format!("连接建立失败: {}", e))
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Error))?;

        // 创建连接信息
        let conn_info = ConnectionInfo {
            connection: connection.clone(),
            created_at: SystemTime::now(),
            last_used: SystemTime::now(),
            usage_count: 0,
            active: true,
            remote_addr: addr,
            health_status: ConnectionHealthStatus::Healthy,
            error_count: 0,
            total_response_time_us: 0,
            last_error: None,
        };

        // 添加到连接池
        {
            let mut connections = self.connections.write().await;
            connections.entry(addr).or_insert_with(Vec::new).push(conn_info);

            // 更新统计
            let mut stats = self.stats.write().await;
            stats.total_connections += 1;
            stats.active_connections += 1;
        }

        info!("新连接创建成功: {}", addr);
        let _ = self.event_sender.send(PoolEvent::ConnectionCreated(addr));

        Ok(connection)
    }

    /// 启动连接清理任务
    async fn start_cleanup_task(&self) {
        let connections = Arc::clone(&self.connections);
        let idle_timeout = self.config.idle_timeout;
        let event_sender = self.event_sender.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            debug!("启动连接清理任务");

            let mut interval = interval(Duration::from_secs(60)); // 每分钟检查一次

            while *is_running.read().await {
                interval.tick().await;

                let now = SystemTime::now();
                let mut connections_to_remove = Vec::new();

                {
                    let mut connections_map = connections.write().await;
                    for (addr, conn_list) in connections_map.iter_mut() {
                        conn_list.retain(|conn_info| {
                            // 检查是否超时
                            if let Ok(elapsed) = now.duration_since(conn_info.last_used) {
                                if elapsed > idle_timeout && !conn_info.active {
                                    debug!("连接超时，移除: {}", addr);
                                    let _ = event_sender.send(PoolEvent::ConnectionTimeout(*addr));
                                    return false;
                                }
                            }
                            true
                        });

                        // 如果连接列表为空，移除该地址
                        if conn_list.is_empty() {
                            connections_to_remove.push(*addr);
                        }
                    }

                    // 移除空的地址条目
                    for addr in connections_to_remove {
                        connections_map.remove(&addr);
                        debug!("地址连接列表已清空: {}", addr);
                    }
                }

                debug!("连接清理完成");
            }

            debug!("连接清理任务停止");
        });
    }

    /// 启动心跳任务
    async fn start_heartbeat_task(&self) {
        let connections = Arc::clone(&self.connections);
        let heartbeat_interval = self.config.heartbeat_interval;
        let event_sender = self.event_sender.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            debug!("启动连接心跳任务");

            let mut interval = interval(heartbeat_interval);

            while *is_running.read().await {
                interval.tick().await;

                let mut connections_to_remove = Vec::new();

                {
                    let connections_map = connections.read().await;
                    for (addr, conn_list) in connections_map.iter() {
                        for conn_info in conn_list.iter() {
                            // 发送心跳
                            if let Err(_) = conn_info.connection.ping() {
                                warn!("心跳失败，连接可能已断开: {}", addr);
                                let _ = event_sender.send(PoolEvent::ConnectionError(*addr, "心跳失败".to_string()));
                                connections_to_remove.push(*addr);
                                break;
                            }
                        }
                    }
                }

                // 移除失败的连接
                if !connections_to_remove.is_empty() {
                    let mut connections_map = connections.write().await;
                    for addr in connections_to_remove {
                        if let Some(conn_list) = connections_map.get_mut(&addr) {
                            conn_list.retain(|conn_info| {
                                // 重新检查连接状态
                                conn_info.connection.close(0u32.into(), b"heartbeat_failed");
                                false
                            });
                        }

                        // 如果连接列表为空，移除该地址
                        if connections_map.get(&addr).map(|list| list.is_empty()).unwrap_or(true) {
                            connections_map.remove(&addr);
                        }
                    }
                }

                debug!("心跳检查完成");
            }

            debug!("连接心跳任务停止");
        });
    }

    /// 启动统计更新任务
    async fn start_stats_update_task(&self) {
        let connections = Arc::clone(&self.connections);
        let stats = Arc::clone(&self.stats);
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            debug!("启动统计更新任务");

            let mut interval = interval(Duration::from_secs(10)); // 每10秒更新一次

            while *is_running.read().await {
                interval.tick().await;

                let mut total_connections = 0;
                let mut active_connections = 0;
                let mut idle_connections = 0;

                {
                    let connections_map = connections.read().await;
                    for conn_list in connections_map.values() {
                        total_connections += conn_list.len();
                        for conn_info in conn_list.iter() {
                            if conn_info.active {
                                active_connections += 1;
                            } else {
                                idle_connections += 1;
                            }
                        }
                    }
                }

                {
                    let mut current_stats = stats.write().await;
                    current_stats.total_connections = total_connections;
                    current_stats.active_connections = active_connections;
                    current_stats.idle_connections = idle_connections;

                    // 计算利用率
                    if self.config.max_connections > 0 {
                        current_stats.utilization_rate = (total_connections as f64) / (self.config.max_connections as f64) * 100.0;
                    }

                    // 计算成功率
                    if current_stats.total_requests > 0 {
                        let success_rate = (current_stats.successful_requests as f64) / (current_stats.total_requests as f64) * 100.0;
                        debug!("连接池统计 - 总连接: {}, 活跃: {}, 空闲: {}, 利用率: {:.1}%, 成功率: {:.1}%",
                            total_connections, active_connections, idle_connections,
                            current_stats.utilization_rate, success_rate);
                    }
                }

                debug!("统计更新完成");
            }

            debug!("统计更新任务停止");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_connection_pool_config_default() {
        let config = ConnectionPoolConfig::default();

        assert_eq!(config.max_connections, 100);
        assert_eq!(config.idle_timeout, Duration::from_secs(300));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert!(config.enable_warmup);
        assert!(matches!(config.load_balance_strategy, LoadBalanceStrategy::LeastConnections));
    }

    #[tokio::test]
    async fn test_load_balance_strategy_round_robin() {
        let config = ConnectionPoolConfig {
            load_balance_strategy: LoadBalanceStrategy::RoundRobin,
            ..Default::default()
        };

        let endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let client_config = ClientConfig::new(Arc::new(
            rustls::ClientConfig::builder().with_safe_defaults().with_no_client_auth().unwrap()
        ));

        let pool = ConnectionPool::new(config, Arc::new(endpoint), Arc::new(client_config));

        let addresses = vec![
            "192.168.1.100:8080".parse().unwrap(),
            "192.168.1.101:8080".parse().unwrap(),
            "192.168.1.102:8080".parse().unwrap(),
        ];

        // 测试轮询
        let addr1 = pool.select_address(&addresses).await.unwrap();
        let addr2 = pool.select_address(&addresses).await.unwrap();
        let addr3 = pool.select_address(&addresses).await.unwrap();
        let addr4 = pool.select_address(&addresses).await.unwrap();

        assert_eq!(addr1, addresses[0]);
        assert_eq!(addr2, addresses[1]);
        assert_eq!(addr3, addresses[2]);
        assert_eq!(addr4, addresses[0]); // 轮回
    }

    #[tokio::test]
    async fn test_connection_stats_initialization() {
        let stats = ConnectionStats {
            total_connections: 0,
            active_connections: 0,
            idle_connections: 0,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_response_time_ms: 0.0,
            utilization_rate: 0.0,
        };

        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.successful_requests, 0);
        assert_eq!(stats.utilization_rate, 0.0);
    }
}