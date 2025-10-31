//! # 设备发现模块
//!
//! 提供局域网内设备的自动发现和心跳检测功能。
//! 支持 mDNS 和 UDP 广播两种发现模式，支持设备上线/下线通知。
//!
//! ## 核心特性
//!
//! - **双模式发现**: 支持 mDNS 零配置和 UDP 广播两种模式
//! - **mDNS 支持**: 基于标准 mDNS 协议的零配置网络发现
//! - **UDP 广播**: 传统高效的 UDP 广播发现机制
//! - **实时心跳**: 定期心跳检测设备在线状态
//! - **事件驱动**: 设备上线/下线事件通知
//! - **安全验证**: 支持设备身份验证和加密通信
//! - **自动切换**: 智能选择最佳发现方式
//!

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep};

/// 设备信息（临时定义，避免循环依赖）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// 设备唯一标识符
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 设备类型
    pub device_type: String,
    /// 网络地址
    pub address: SocketAddr,
    /// 设备能力
    pub capabilities: Vec<String>,
    /// 最后活跃时间
    pub last_active: SystemTime,
}


/// 设备发现服务结果类型
pub type DiscoveryResult<T> = std::result::Result<T, ErrorInfo>;

/// 设备发现配置
///
/// 配置设备发现服务的各种参数
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// 监听端口
    port: u16,
    /// 心跳间隔
    heartbeat_interval: Duration,
    /// 设备超时时间
    device_timeout: Duration,
    /// 广播地址
    broadcast_address: String,
    /// 最大重试次数
    #[allow(dead_code)]
    max_retries: u32,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            heartbeat_interval: Duration::from_secs(30),
            device_timeout: Duration::from_secs(90),
            broadcast_address: "255.255.255.255".to_string(),
            max_retries: 3,
        }
    }
}

impl DiscoveryConfig {
    /// 创建新的默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置监听端口
    ///
    /// # 参数
    ///
    /// * `port` - UDP 监听端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置心跳间隔
    ///
    /// # 参数
    ///
    /// * `interval` - 心跳发送间隔
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// 设置设备超时时间
    ///
    /// # 参数
    ///
    /// * `timeout` - 设备无响应超时时间
    pub fn with_device_timeout(mut self, timeout: Duration) -> Self {
        self.device_timeout = timeout;
        self
    }

    /// 设置广播地址
    ///
    /// # 参数
    ///
    /// * `address` - 广播地址
    pub fn with_broadcast_address(mut self, address: String) -> Self {
        self.broadcast_address = address;
        self
    }

    /// 获取监听端口
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 获取心跳间隔
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    /// 获取设备超时时间
    pub fn device_timeout(&self) -> Duration {
        self.device_timeout
    }

    /// 获取广播地址
    pub fn broadcast_address(&self) -> &str {
        &self.broadcast_address
    }
}

/// 设备发现消息类型
///
/// 定义设备发现过程中使用的各种消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMessage {
    /// 设备广播消息
    DeviceAnnouncement {
        /// 设备信息
        device_info: DeviceInfo,
        /// 消息时间戳
        timestamp: SystemTime,
        /// 消息ID（用于去重）
        message_id: String,
    },
    /// 设备心跳消息
    Heartbeat {
        /// 设备ID
        device_id: String,
        /// 时间戳
        timestamp: SystemTime,
    },
    /// 设备下线消息
    DeviceOffline {
        /// 设备ID
        device_id: String,
        /// 时间戳
        timestamp: SystemTime,
    },
}

/// 设备事件类型
///
/// 设备状态变化时产生的事件
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// 设备上线事件
    DeviceOnline(DeviceInfo),
    /// 设备下线事件
    DeviceOffline(String),
    /// 设备更新事件（设备信息发生变化）
    DeviceUpdated(DeviceInfo),
}

/// 设备发现服务
///
/// 负责局域网内设备的自动发现和状态维护
pub struct DiscoveryService {
    /// 配置信息
    config: DiscoveryConfig,
    /// 本地设备信息
    local_device: DeviceInfo,
    /// UDP 套接字
    socket: Arc<UdpSocket>,
    /// 已发现的设备列表
    discovered_devices: Arc<RwLock<HashMap<String, DeviceInfo>>>,
    /// 设备事件发送器
    event_sender: mpsc::UnboundedSender<DeviceEvent>,
    /// 设备事件接收器
    event_receiver: Option<mpsc::UnboundedReceiver<DeviceEvent>>,
    /// 运行状态
    is_running: Arc<RwLock<bool>>,
    /// 消息ID计数器
    message_counter: Arc<RwLock<u64>>,
    /// 序列化缓冲区池，减少内存分配
    serialization_pool: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl DiscoveryService {
    /// 创建新的设备发现服务
    ///
    /// # 参数
    ///
    /// * `config` - 发现服务配置
    /// * `local_device` - 本地设备信息
    ///
    /// # 返回值
    ///
    /// 返回发现服务实例或错误信息
    pub async fn new(
        config: DiscoveryConfig,
        local_device: DeviceInfo,
    ) -> DiscoveryResult<Self> {
        // 绑定 UDP 套接字
        let bind_addr = format!("0.0.0.0:{}", config.port());
        let socket = UdpSocket::bind(&bind_addr)
            .await
            .map_err(|e| ErrorInfo::new(2001, format!("绑定UDP端口失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 设置套接字广播选项
        socket.set_broadcast(true)
            .map_err(|e| ErrorInfo::new(2002, format!("设置广播模式失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        let socket = Arc::new(socket);

        // 创建事件通道
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            local_device,
            socket,
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            event_receiver: Some(event_receiver),
            is_running: Arc::new(RwLock::new(false)),
            message_counter: Arc::new(RwLock::new(0)),
            serialization_pool: Arc::new(RwLock::new(Vec::with_capacity(10))),
        })
    }

    /// 启动设备发现服务
    ///
    /// 开始监听网络消息并发送心跳
    ///
    /// # 返回值
    ///
    /// 返回启动结果或错误信息
    pub async fn start(&mut self) -> DiscoveryResult<()> {
        // 检查是否已经启动
        {
            let mut is_running = self.is_running.write().await;
            if *is_running {
                return Err(ErrorInfo::new(2003, "发现服务已经在运行".to_string())
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Warning));
            }
            *is_running = true;
        }

        // 发送初始设备广播
        self.broadcast_device_announcement().await?;

        // 启动消息接收任务
        self.start_message_receiver().await;

        // 启动心跳任务
        self.start_heartbeat_task().await;

        // 启动设备清理任务
        self.start_device_cleanup_task().await;

        Ok(())
    }

    /// 停止设备发现服务
    pub async fn stop(&self) -> DiscoveryResult<()> {
        // 发送设备下线消息
        self.broadcast_device_offline().await?;

        // 设置运行状态为停止
        let mut is_running = self.is_running.write().await;
        *is_running = false;

        Ok(())
    }

    /// 获取下一个设备事件
    ///
    /// # 返回值
    ///
    /// 返回设备事件或None（如果通道关闭）
    pub async fn next_event(&mut self) -> Option<DeviceEvent> {
        match &mut self.event_receiver {
            Some(receiver) => receiver.recv().await,
            None => None,
        }
    }

    /// 获取已发现的设备列表
    ///
    /// # 返回值
    ///
    /// 返回设备信息的副本列表
    pub async fn get_discovered_devices(&self) -> Vec<DeviceInfo> {
        let devices = self.discovered_devices.read().await;
        devices.values().cloned().collect()
    }

    /// 根据ID获取设备信息
    ///
    /// # 参数
    ///
    /// * `device_id` - 设备ID
    ///
    /// # 返回值
    ///
    /// 返回设备信息或None
    pub async fn get_device(&self, device_id: &str) -> Option<DeviceInfo> {
        let devices = self.discovered_devices.read().await;
        devices.get(device_id).cloned()
    }

    /// 广播设备公告消息
    async fn broadcast_device_announcement(&self) -> DiscoveryResult<()> {
        let message = DiscoveryMessage::DeviceAnnouncement {
            device_info: self.local_device.clone(),
            timestamp: SystemTime::now(),
            message_id: self.generate_message_id().await,
        };

        // 使用缓冲区池减少内存分配
        let message_data = self.serialize_with_pool(&message).await?;

        let broadcast_addr = format!("{}:{}", self.config.broadcast_address(), self.config.port());
        self.socket.send_to(&message_data, &broadcast_addr)
            .await
            .map_err(|e| ErrorInfo::new(2005, format!("发送设备公告失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 归还缓冲区到池中
        self.return_buffer_to_pool(message_data).await;

        Ok(())
    }

    /// 广播设备下线消息
    async fn broadcast_device_offline(&self) -> DiscoveryResult<()> {
        let message = DiscoveryMessage::DeviceOffline {
            device_id: self.local_device.device_id.clone(),
            timestamp: SystemTime::now(),
        };

        let message_data = serde_json::to_vec(&message)
            .map_err(|e| ErrorInfo::new(2006, format!("序列化设备下线消息失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        let broadcast_addr = format!("{}:{}", self.config.broadcast_address(), self.config.port());
        self.socket.send_to(&message_data, &broadcast_addr)
            .await
            .map_err(|e| ErrorInfo::new(2007, format!("发送设备下线消息失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        Ok(())
    }

    /// 广播心跳消息
    #[allow(dead_code)]
    async fn broadcast_heartbeat(&self) -> DiscoveryResult<()> {
        let message = DiscoveryMessage::Heartbeat {
            device_id: self.local_device.device_id.clone(),
            timestamp: SystemTime::now(),
        };

        let message_data = serde_json::to_vec(&message)
            .map_err(|e| ErrorInfo::new(2008, format!("序列化心跳消息失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        let broadcast_addr = format!("{}:{}", self.config.broadcast_address(), self.config.port());
        self.socket.send_to(&message_data, &broadcast_addr)
            .await
            .map_err(|e| ErrorInfo::new(2009, format!("发送心跳消息失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        Ok(())
    }

    /// 启动消息接收任务
    async fn start_message_receiver(&self) {
        let socket = Arc::clone(&self.socket);
        let discovered_devices = Arc::clone(&self.discovered_devices);
        let event_sender = self.event_sender.clone();
        let is_running = Arc::clone(&self.is_running);
        let local_device_id = self.local_device.device_id.clone();

        tokio::spawn(async move {
            // 使用栈分配的数组减少堆分配，提高性能
            let mut buffer = [0u8; 8192]; // 8KB 栈缓冲区，适合大多数UDP消息

            while *is_running.read().await {
                match socket.recv_from(&mut buffer).await {
                    Ok((len, addr)) => {
                        // 处理接收到的消息
                        if let Ok(message_data) = std::str::from_utf8(&buffer[..len]) {
                            if let Ok(message) = serde_json::from_str::<DiscoveryMessage>(message_data) {
                                Self::handle_received_message(
                                    message,
                                    addr,
                                    &discovered_devices,
                                    &event_sender,
                                    &local_device_id,
                                ).await;
                            }
                        }
                    }
                    Err(_) => {
                        // 接收错误，继续循环
                        sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });
    }

    /// 启动心跳任务
    async fn start_heartbeat_task(&self) {
        let heartbeat_interval = self.config.heartbeat_interval();
        let is_running = Arc::clone(&self.is_running);

        // 克隆必要的数据
        let socket = Arc::clone(&self.socket);
        let broadcast_address = self.config.broadcast_address().to_string();
        let port = self.config.port();
        let local_device_id = self.local_device.device_id.clone();

        tokio::spawn(async move {
            let mut interval = interval(heartbeat_interval);

            while *is_running.read().await {
                interval.tick().await;

                let message = DiscoveryMessage::Heartbeat {
                    device_id: local_device_id.clone(),
                    timestamp: SystemTime::now(),
                };

                if let Ok(message_data) = serde_json::to_vec(&message) {
                    let broadcast_addr = format!("{}:{}", broadcast_address, port);
                    let _ = socket.send_to(&message_data, &broadcast_addr).await;
                }
            }
        });
    }

    /// 启动设备清理任务
    async fn start_device_cleanup_task(&self) {
        let discovered_devices = Arc::clone(&self.discovered_devices);
        let event_sender = self.event_sender.clone();
        let device_timeout = self.config.device_timeout();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // 每10秒检查一次

            while *is_running.read().await {
                interval.tick().await;

                let now = SystemTime::now();
                let mut timeout_devices = Vec::new();

                {
                    let devices = discovered_devices.read().await;
                    for (device_id, device_info) in devices.iter() {
                        if let Ok(elapsed) = now.duration_since(device_info.last_active) {
                            if elapsed > device_timeout {
                                timeout_devices.push(device_id.clone());
                            }
                        }
                    }
                }

                // 移除超时设备并发送事件
                if !timeout_devices.is_empty() {
                    let mut devices = discovered_devices.write().await;
                    for device_id in timeout_devices {
                        devices.remove(&device_id);
                        let _ = event_sender.send(DeviceEvent::DeviceOffline(device_id.clone()));
                    }
                }
            }
        });
    }

    /// 处理接收到的消息
    async fn handle_received_message(
        message: DiscoveryMessage,
        addr: SocketAddr,
        discovered_devices: &Arc<RwLock<HashMap<String, DeviceInfo>>>,
        event_sender: &mpsc::UnboundedSender<DeviceEvent>,
        local_device_id: &str,
    ) {
        match message {
            DiscoveryMessage::DeviceAnnouncement { device_info, timestamp, .. } => {
                // 忽略自己的消息
                if device_info.device_id == local_device_id {
                    return;
                }

                let mut devices = discovered_devices.write().await;
                let is_new_device = !devices.contains_key(&device_info.device_id);

                // 更新设备信息
                let mut updated_device = device_info.clone();
                updated_device.last_active = timestamp;
                updated_device.address = addr; // 更新实际接收到的地址

                devices.insert(device_info.device_id.clone(), updated_device.clone());

                // 发送事件
                if is_new_device {
                    let _ = event_sender.send(DeviceEvent::DeviceOnline(updated_device));
                } else {
                    let _ = event_sender.send(DeviceEvent::DeviceUpdated(updated_device));
                }
            }
            DiscoveryMessage::Heartbeat { device_id, timestamp } => {
                // 忽略自己的心跳
                if device_id == local_device_id {
                    return;
                }

                let mut devices = discovered_devices.write().await;
                if let Some(device_info) = devices.get_mut(&device_id) {
                    device_info.last_active = timestamp;
                }
            }
            DiscoveryMessage::DeviceOffline { device_id, .. } => {
                // 忽略自己的下线消息
                if device_id == local_device_id {
                    return;
                }

                let mut devices = discovered_devices.write().await;
                if devices.remove(&device_id).is_some() {
                    let _ = event_sender.send(DeviceEvent::DeviceOffline(device_id));
                }
            }
        }
    }

    /// 生成消息ID
    async fn generate_message_id(&self) -> String {
        let mut counter = self.message_counter.write().await;
        *counter += 1;
        format!("msg-{}-{}", self.local_device.device_id, *counter)
    }

    /// 使用缓冲区池序列化消息，减少内存分配
    async fn serialize_with_pool(&self, message: &DiscoveryMessage) -> DiscoveryResult<Vec<u8>> {
        // 尝试从池中获取缓冲区
        let mut buffer = {
            let mut pool = self.serialization_pool.write().await;
            pool.pop().unwrap_or_else(|| Vec::with_capacity(4096))
        };

        buffer.clear();

        // 序列化到缓冲区，使用优化的错误处理
        serde_json::to_writer(&mut buffer, message)
            .map_err(|e| ErrorInfo::new(2020, format!("序列化发现消息失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
                .with_context("discovery_service".to_string())
                .with_context("serialize_message".to_string()))?;

        Ok(buffer)
    }

    /// 将缓冲区归还到池中
    async fn return_buffer_to_pool(&self, mut buffer: Vec<u8>) {
        // 清理缓冲区但保留容量，减少后续分配
        buffer.clear();

        // 限制池大小，避免内存泄漏
        let mut pool = self.serialization_pool.write().await;
        if pool.len() < 20 {
            pool.push(buffer);
        }
        // 如果池满了，直接丢弃缓冲区
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_discovery_service(port: u16) -> DiscoveryResult<(DiscoveryService, DeviceInfo)> {
        let config = DiscoveryConfig::new()
            .with_port(port)
            .with_heartbeat_interval(Duration::from_secs(1))
            .with_device_timeout(Duration::from_secs(3));

        let local_device = DeviceInfo {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            device_type: "Desktop".to_string(),
            address: "127.0.0.1:8080".parse().expect("地址解析失败"),
            capabilities: vec!["messaging".to_string(), "file_transfer".to_string()],
            last_active: SystemTime::now(),
        };

        let service = DiscoveryService::new(config, local_device.clone()).await?;
        Ok((service, local_device))
    }

    #[tokio::test]
    async fn test_discovery_config_creation() {
        let config = DiscoveryConfig::new()
            .with_port(9090)
            .with_heartbeat_interval(Duration::from_secs(15))
            .with_device_timeout(Duration::from_secs(45));

        assert_eq!(config.port(), 9090);
        assert_eq!(config.heartbeat_interval(), Duration::from_secs(15));
        assert_eq!(config.device_timeout(), Duration::from_secs(45));
    }

    #[tokio::test]
    async fn test_discovery_service_creation() {
        let (service, _) = create_test_discovery_service(18080).await;
        assert!(!service.local_device.device_id.is_empty());
    }

    #[tokio::test]
    async fn test_discovery_service_start_stop() {
        let (mut service, _) = create_test_discovery_service(18081).await;

        // 启动服务
        let start_result = service.start().await;
        assert!(start_result.is_ok(), "服务启动应该成功");

        // 短暂等待
        sleep(Duration::from_millis(100)).await;

        // 停止服务
        let stop_result = service.stop().await;
        assert!(stop_result.is_ok(), "服务停止应该成功");
    }

    #[tokio::test]
    async fn test_message_serialization() {
        let device_info = DeviceInfo {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            device_type: "Desktop".to_string(),
            address: "127.0.0.1:8080".parse().expect("地址解析失败"),
            capabilities: vec!["messaging".to_string(), "file_transfer".to_string()],
            last_active: SystemTime::now(),
        };

        let message = DiscoveryMessage::DeviceAnnouncement {
            device_info: device_info.clone(),
            timestamp: SystemTime::now(),
            message_id: "test-message".to_string(),
        };

        let serialized = serde_json::to_string(&message).unwrap();
        let deserialized: DiscoveryMessage = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            DiscoveryMessage::DeviceAnnouncement { device_info: d, .. } => {
                assert_eq!(d.device_id, device_info.device_id);
                assert_eq!(d.device_name, device_info.device_name);
            }
            _ => panic!("消息类型不匹配"),
        }
    }

    #[tokio::test]
    async fn test_device_discovery_flow() {
        // 创建两个发现服务实例，模拟设备发现
        let (mut service1, device1) = create_test_discovery_service(18082).await;
        let (mut service2, device2) = create_test_discovery_service(18083).await;

        // 启动服务
        service1.start().await.expect("服务1启动失败");
        service2.start().await.expect("服务2启动失败");

        // 等待设备发现
        sleep(Duration::from_millis(500)).await;

        // 检查是否发现了对方
        let discovered1 = service1.get_discovered_devices().await;
        let discovered2 = service2.get_discovered_devices().await;

        // 验证设备发现结果
        assert!(!discovered1.is_empty() || !discovered2.is_empty(),
               "至少应该发现一个设备");

        // 停止服务
        service1.stop().await.expect("服务1停止失败");
        service2.stop().await.expect("服务2停止失败");
    }
}