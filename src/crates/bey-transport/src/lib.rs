//! # 安全传输层模块
//!
//! 基于 QUIC 协议的安全传输层，提供端到端加密、身份验证和数据完整性保护。
//! 所有证书相关操作完全由 bey_identity 证书管理模块处理，本模块只负责连接管理。
//!
//! ## 核心特性
//!
//! - **QUIC 协议**: 基于 UDP 的高性能传输协议
//! - **TLS 1.3 加密**: 最新的传输层安全协议
//! - **双向认证**: 客户端和服务端的相互身份验证
//! - **证书管理**: 完全依赖 bey_identity 模块进行证书管理
//! - **连接复用**: 支持多路复用和流管理
//! - **策略引擎**: 集成安全策略管理

// 模块声明
pub mod mtls_manager;
pub mod policy_engine;

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use quinn::{Endpoint, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, debug};
use bey_identity::CertificateManager;
use mtls_manager::CompleteMtlsManager;
use policy_engine::{CompletePolicyEngine, PolicyContext, PolicyAction};

// 类型别名
pub type MtlsStats = mtls_manager::MtlsStats;
pub type PolicyEngineStats = policy_engine::PolicyEngineStats;


/// 传输消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportMessage {
    /// 消息ID
    pub id: String,
    /// 消息类型
    pub message_type: String,
    /// 消息内容
    pub content: serde_json::Value,
    /// 时间戳
    pub timestamp: std::time::SystemTime,
    /// 发送者ID
    pub sender_id: String,
    /// 接收者ID
    pub receiver_id: Option<String>,
}

/// 安全传输层结果类型
pub type TransportResult<T> = std::result::Result<T, ErrorInfo>;

/// 传输层配置
///
/// 配置安全传输层的各种参数
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// 监听端口
    port: u16,
    /// 证书存储目录
    certificates_dir: PathBuf,
    /// 连接超时时间
    connection_timeout: Duration,
    /// 最大并发连接数
    max_connections: u32,
    /// 是否启用客户端证书验证
    require_client_cert: bool,
    /// 心跳间隔
    keep_alive_interval: Duration,
    /// 最大空闲超时
    idle_timeout: Duration,
    /// 组织名称
    organization_name: String,
    /// 国家代码
    country_code: String,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            port: 8443,
            certificates_dir: PathBuf::from("./certs"),
            connection_timeout: Duration::from_secs(30),
            max_connections: 100,
            require_client_cert: true,
            keep_alive_interval: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(60),
            organization_name: "BEY".to_string(),
            country_code: "CN".to_string(),
        }
    }
}

impl TransportConfig {
    /// 创建新的默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置监听端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置证书存储目录
    pub fn with_certificates_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.certificates_dir = dir.as_ref().to_path_buf();
        self
    }

    /// 设置连接超时时间
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// 设置最大并发连接数
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// 设置是否需要客户端证书
    pub fn with_require_client_cert(mut self, require: bool) -> Self {
        self.require_client_cert = require;
        self
    }

    /// 设置心跳间隔
    pub fn with_keep_alive_interval(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }

    /// 设置最大空闲超时
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// 设置组织名称
    pub fn with_organization_name(mut self, name: String) -> Self {
        self.organization_name = name;
        self
    }

    /// 设置国家代码
    pub fn with_country_code(mut self, code: String) -> Self {
        self.country_code = code;
        self
    }

    /// 获取监听端口
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 获取证书存储目录
    pub fn certificates_dir(&self) -> &Path {
        &self.certificates_dir
    }

    /// 获取连接超时时间
    pub fn connection_timeout(&self) -> Duration {
        self.connection_timeout
    }

    /// 获取最大并发连接数
    pub fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// 获取是否需要客户端证书
    pub fn require_client_cert(&self) -> bool {
        self.require_client_cert
    }

    /// 获取心跳间隔
    pub fn keep_alive_interval(&self) -> Duration {
        self.keep_alive_interval
    }

    /// 获取最大空闲超时
    pub fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }
}

/// 安全传输层
///
/// 基于 QUIC 协议的安全传输层实现，完全依赖 bey_identity 进行证书管理
pub struct SecureTransport {
    /// 配置信息
    config: TransportConfig,
    /// 服务器端点
    endpoint: Option<Endpoint>,
    /// 活跃连接
    connections: Arc<RwLock<HashMap<SocketAddr, Connection>>>,
    /// 运行状态
    is_running: Arc<RwLock<bool>>,
    /// 设备ID
    device_id: String,
    /// mTLS管理器
    mtls_manager: Arc<CompleteMtlsManager>,
    /// 策略引擎
    policy_engine: Arc<CompletePolicyEngine>,
}

impl SecureTransport {
    /// 创建新的安全传输层实例
    ///
    /// # 参数
    ///
    /// * `config` - 传输层配置
    /// * `device_id` - 设备唯一标识
    ///
    /// # 返回值
    ///
    /// 返回传输层实例或错误信息
    pub async fn new(config: TransportConfig, device_id: String) -> TransportResult<Self> {
        info!("初始化安全传输层，设备ID: {}", device_id);

        // 创建证书管理器
        use bey_identity::config::CertificateConfig;
        let cert_config = CertificateConfig::builder()
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("BEY Transport CA")
            .with_organization_name(&config.organization_name)
            .with_country_code(&config.country_code)
            .build()
            .map_err(|e| ErrorInfo::new(2001, format!("创建证书配置失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        let _certificate_manager = Arc::new(
            CertificateManager::initialize(cert_config).await
                .map_err(|e| ErrorInfo::new(2001, format!("初始化证书管理器失败: {}", e))
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Error))?
        );

        // 创建mTLS管理器
        let mtls_config = mtls_manager::MtlsConfig {
            enabled: true,
            certificates_dir: config.certificates_dir.to_path_buf(),
            enable_config_cache: true,
            config_cache_ttl: Duration::from_secs(300), // 5分钟缓存
            max_config_cache_entries: 100,
            device_id_prefix: device_id.clone(),
            organization_name: config.organization_name.clone(),
            country_code: config.country_code.clone(),
        };

        let mtls_manager = Arc::new(
            CompleteMtlsManager::new(mtls_config, device_id.clone())
                .await
                .map_err(|e| ErrorInfo::new(2002, format!("创建mTLS管理器失败: {}", e))
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Error))?
        );

        // 创建策略引擎
        let policy_config = policy_engine::PolicyEngineConfig::default();
        let policy_engine = Arc::new(CompletePolicyEngine::new(policy_config));

        let transport = Self {
            config,
            endpoint: None,
            connections: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(RwLock::new(false)),
            device_id,
            mtls_manager,
            policy_engine,
        };

        info!("安全传输层初始化完成");
        Ok(transport)
    }

    /// 启动传输层服务器
    ///
    /// # 返回值
    ///
    /// 返回启动结果或错误信息
    pub async fn start_server(&mut self) -> TransportResult<()> {
        if self.endpoint.is_some() {
            return Err(ErrorInfo::new(2004, "传输层已经启动".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Warning));
        }

        // 设置运行状态
        {
            let mut is_running = self.is_running.write().await;
            *is_running = true;
        }

        // 获取服务器配置
        let rustls_server_config = self.mtls_manager.get_server_config().await
            .map_err(|e| ErrorInfo::new(2005, format!("获取服务器配置失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        // 直接使用mTLS管理器提供的Quinn服务器配置
        let server_config = rustls_server_config;

        // 创建服务器端点
        let server_addr = format!("0.0.0.0:{}", self.config.port())
            .parse()
            .map_err(|e| ErrorInfo::new(2006, format!("解析服务器地址失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        let endpoint = Endpoint::server(server_config, server_addr)
            .map_err(|e| ErrorInfo::new(2007, format!("创建服务器端点失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        self.endpoint = Some(endpoint.clone());

        // 启动连接接受任务
        self.start_connection_acceptor(endpoint).await;

        info!("安全传输层服务器已启动，监听端口: {}", self.config.port());

        Ok(())
    }

    /// 连接到远程设备
    ///
    /// # 参数
    ///
    /// * `remote_addr` - 远程设备地址
    ///
    /// # 返回值
    ///
    /// 返回连接对象或错误信息
    pub async fn connect(&self, remote_addr: SocketAddr) -> TransportResult<Connection> {
        // 创建策略上下文进行访问控制
        let policy_context = PolicyContext::new()
            .with_requester_id(self.device_id.clone())
            .with_resource(format!("remote-connection:{}", remote_addr))
            .with_operation("connect".to_string())
            .set_field("target_address".to_string(), serde_json::Value::String(remote_addr.to_string()));

        // 评估连接策略 - 使用默认策略集合
        let policy_result = self.policy_engine.evaluate("default", &policy_context).await
            .map_err(|e| ErrorInfo::new(2021, format!("策略评估失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        if policy_result.final_action != PolicyAction::Allow {
            return Err(ErrorInfo::new(2022, format!("连接被策略拒绝: {}", policy_result.evaluation_summary))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        }

        debug!("连接策略评估通过: {} -> {}", self.device_id, remote_addr);

        // 获取客户端配置
        let client_config = self.mtls_manager.get_client_config().await
            .map_err(|e| ErrorInfo::new(2008, format!("获取客户端配置失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        // 直接使用mTLS管理器提供的Quinn客户端配置
        let client_quinn_config = client_config;

        // 创建客户端端点
        let client_endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()
            .map_err(|e| ErrorInfo::new(2009, format!("解析客户端地址失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?)
            .map_err(|e| ErrorInfo::new(2009, format!("创建客户端端点失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 连接到远程设备
        let connecting = client_endpoint.connect_with(client_quinn_config, remote_addr, "bey-transport")
            .map_err(|e| ErrorInfo::new(2010, format!("发起连接失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        let connection = tokio::time::timeout(
            self.config.connection_timeout(),
            connecting
        ).await
            .map_err(|_| ErrorInfo::new(2011, "连接超时".to_string())
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?
            .map_err(|e| ErrorInfo::new(2012, format!("连接失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 存储连接
        {
            let mut connections = self.connections.write().await;
            connections.insert(remote_addr, connection.clone());
        }

        info!("已连接到远程设备: {}", remote_addr);

        Ok(connection)
    }

    /// 发送消息
    ///
    /// # 参数
    ///
    /// * `connection` - 连接对象
    /// * `message` - 要发送的消息
    ///
    /// # 返回值
    ///
    /// 返回发送结果或错误信息
    pub async fn send_message(&self, connection: &Connection, message: TransportMessage) -> TransportResult<()> {
        // 创建发送策略上下文
        let policy_context = PolicyContext::new()
            .with_requester_id(self.device_id.clone())
            .with_resource(format!("message:{}", message.id))
            .with_operation("send".to_string())
            .set_field("message_type".to_string(), serde_json::Value::String(message.message_type.clone()))
            .set_field("receiver_id".to_string(),
                message.receiver_id.clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null));

        // 评估发送策略
        let policy_result = self.policy_engine.evaluate("default", &policy_context).await
            .map_err(|e| ErrorInfo::new(2013, format!("发送策略评估失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        if policy_result.final_action != PolicyAction::Allow {
            return Err(ErrorInfo::new(2014, format!("消息发送被策略拒绝: {}", policy_result.evaluation_summary))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        }

        debug!("发送策略评估通过: {} -> {}", self.device_id, message.id);

        let message_data = serde_json::to_vec(&message)
            .map_err(|e| ErrorInfo::new(2011, format!("序列化消息失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        let mut stream = connection.open_uni().await
            .map_err(|e| ErrorInfo::new(2012, format!("打开单向流失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        stream.write_all(&message_data).await
            .map_err(|e| ErrorInfo::new(2013, format!("发送消息失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        stream.finish()
            .map_err(|e| ErrorInfo::new(2014, format!("完成发送失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        debug!("消息发送完成: {}", message.id);
        Ok(())
    }

    /// 接收消息
    ///
    /// # 参数
    ///
    /// * `connection` - 连接对象
    ///
    /// # 返回值
    ///
    /// 返回接收到的消息或错误信息
    pub async fn receive_message(&self, connection: &Connection) -> TransportResult<TransportMessage> {
        // 等待接收单向流
        let mut stream = connection.accept_uni().await
            .map_err(|e| ErrorInfo::new(2015, format!("接受单向流失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 读取消息数据
        let buffer = stream.read_to_end(1024 * 1024).await // 限制为1MB
            .map_err(|e| ErrorInfo::new(2016, format!("读取消息失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 反序列化消息
        let message: TransportMessage = serde_json::from_slice(&buffer)
            .map_err(|e| ErrorInfo::new(2017, format!("反序列化消息失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        // 创建接收策略上下文
        let policy_context = PolicyContext::new()
            .with_requester_id(message.sender_id.clone())
            .with_resource(format!("message:{}", message.id))
            .with_operation("receive".to_string())
            .set_field("message_type".to_string(), serde_json::Value::String(message.message_type.clone()))
            .set_field("receiver_id".to_string(), serde_json::Value::String(self.device_id.clone()));

        // 评估接收策略
        let policy_result = self.policy_engine.evaluate("default", &policy_context).await
            .map_err(|e| ErrorInfo::new(2018, format!("接收策略评估失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        if policy_result.final_action != PolicyAction::Allow {
            return Err(ErrorInfo::new(2019, format!("消息接收被策略拒绝: {}", policy_result.evaluation_summary))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        }

        debug!("接收策略评估通过: {} -> {}", message.sender_id, self.device_id);

        debug!("消息接收完成: {}", message.id);
        Ok(message)
    }

    /// 断开连接
    ///
    /// # 参数
    ///
    /// * `remote_addr` - 远程地址
    pub async fn disconnect(&self, remote_addr: SocketAddr) -> TransportResult<()> {
        let mut connections = self.connections.write().await;

        if let Some(connection) = connections.remove(&remote_addr) {
            connection.close(0u32.into(), b"disconnect");
            info!("已断开连接: {}", remote_addr);
            Ok(())
        } else {
            Err(ErrorInfo::new(2018, format!("连接不存在: {}", remote_addr))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 停止传输层
    pub async fn stop(&self) {
        info!("正在停止安全传输层");

        // 设置停止标志
        {
            let mut is_running = self.is_running.write().await;
            *is_running = false;
        }

        // 关闭所有连接
        {
            let mut connections = self.connections.write().await;
            for (addr, connection) in connections.drain() {
                connection.close(0u32.into(), b"shutdown");
                debug!("已关闭连接: {}", addr);
            }
        }

        // 关闭端点
        if let Some(endpoint) = &self.endpoint {
            endpoint.close(0u32.into(), b"shutdown");
        }

        info!("安全传输层已停止");
    }

    /// 获取活跃连接数量
    pub async fn active_connections_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// 获取所有活跃连接的地址
    pub async fn active_connections(&self) -> Vec<SocketAddr> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// 获取mTLS统计信息
    pub async fn get_mtls_stats(&self) -> crate::MtlsStats {
        self.mtls_manager.get_stats().await
    }

    /// 获取策略引擎统计信息
    pub async fn get_policy_stats(&self) -> crate::PolicyEngineStats {
        self.policy_engine.get_stats().await
    }

    /// 手动更新证书
    pub async fn update_certificates(&self) -> TransportResult<()> {
        self.mtls_manager.update_certificate().await
            .map_err(|e| ErrorInfo::new(2019, format!("更新证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        info!("证书更新完成");
        Ok(())
    }

    /// 验证远程证书
    pub async fn verify_remote_certificate(&self, cert_der: &[u8]) -> TransportResult<bool> {
        let result = self.mtls_manager.verify_remote_certificate(cert_der).await
            .map_err(|e| ErrorInfo::new(2020, format!("验证证书失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        debug!("证书验证结果: {}", result);
        Ok(result)
    }

    /// 启动连接接受器
    async fn start_connection_acceptor(&self, endpoint: Endpoint) {
        let connections = Arc::clone(&self.connections);
        let is_running = Arc::clone(&self.is_running);
        let device_id = self.device_id.clone();
        let policy_engine = Arc::clone(&self.policy_engine);

        tokio::spawn(async move {
            while *is_running.read().await {
                if let Some(incoming) = endpoint.accept().await {
                    let remote_addr = incoming.remote_address();

                    // 创建连接接受策略上下文
                    let policy_context = PolicyContext::new()
                        .with_requester_id("remote".to_string())
                        .with_resource(format!("connection:{}", remote_addr))
                        .with_operation("accept".to_string())
                        .set_field("local_device".to_string(), serde_json::Value::String(device_id.clone()));

                    // 评估连接接受策略
                    let policy_result = policy_engine.evaluate("default", &policy_context).await;
                    match policy_result {
                        Ok(result) if result.final_action == PolicyAction::Allow => {
                            debug!("连接接受策略评估通过: {} -> {}", remote_addr, device_id);
                        }
                        Ok(result) => {
                            debug!("连接接受策略被拒绝: {} -> {}, 原因: {}",
                                remote_addr, device_id, result.evaluation_summary);
                            continue;
                        }
                        Err(e) => {
                            debug!("连接接受策略评估失败: {} -> {}, 错误: {}", remote_addr, device_id, e);
                            continue;
                        }
                    }

                    // 尝试接受连接
                    match tokio::time::timeout(
                        Duration::from_secs(10),
                        incoming
                    ).await {
                        Ok(Ok(conn)) => {
                            // 存储连接
                            {
                                let mut connections = connections.write().await;
                                connections.insert(remote_addr, conn.clone());
                            }

                            info!("接受新的连接: {}", remote_addr);

                            // 为每个连接启动处理任务
                            let connections_clone = Arc::clone(&connections);
                            let is_running_clone = Arc::clone(&is_running);

                            tokio::spawn(async move {
                                while *is_running_clone.read().await {
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    // 这里可以添加连接健康检查逻辑
                                }

                                // 清理连接
                                connections_clone.write().await.remove(&remote_addr);
                                info!("连接已断开: {}", remote_addr);
                            });
                        }
                        Ok(Err(e)) => {
                            debug!("接受连接失败: {} -> {}", remote_addr, e);
                        }
                        Err(_) => {
                            debug!("接受连接超时: {}", remote_addr);
                        }
                    }
                }
            }
        });
    }
}


impl Drop for SecureTransport {
    fn drop(&mut self) {
        // 在析构时确保资源被正确释放
        if let Some(endpoint) = &self.endpoint {
            endpoint.close(0u32.into(), b"drop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    
    static INIT: Once = Once::new();

    fn init_logging() {
        INIT.call_once(|| {
            // 尝试初始化日志，如果已经初始化则忽略错误
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        });
    }

    async fn create_test_transport_config(port: u16) -> TransportResult<TransportConfig> {
        let temp_dir = std::env::temp_dir().join(format!("bey-test-{}", port));
        Ok(TransportConfig::new()
            .with_port(port)
            .with_certificates_dir(&temp_dir)
            .with_max_connections(10)
            .with_connection_timeout(Duration::from_secs(5)))
    }

    #[tokio::test]
    async fn test_transport_config_creation() {
        init_logging();

        let config = TransportConfig::new()
            .with_port(8443)
            .with_max_connections(100)
            .with_require_client_cert(true);

        assert_eq!(config.port(), 8443);
        assert_eq!(config.max_connections(), 100);
        assert!(config.require_client_cert());
    }

    #[tokio::test]
    async fn test_secure_transport_creation() {
        init_logging();

        let config = create_test_transport_config(0).await.unwrap();
        let device_id = "test-device-001".to_string();

        let transport_result = SecureTransport::new(config, device_id).await;
        assert!(transport_result.is_ok(), "安全传输层创建应该成功");

        let transport = transport_result.unwrap();
        assert_eq!(transport.active_connections_count().await, 0);
    }

    #[tokio::test]
    async fn test_certificate_operations() {
        init_logging();

        let config = create_test_transport_config(0).await.unwrap();
        let device_id = "test-device-cert".to_string();

        let transport = SecureTransport::new(config, device_id).await.unwrap();

        // 测试证书更新
        let update_result = transport.update_certificates().await;
        assert!(update_result.is_ok(), "证书更新应该成功");

        // 测试获取mTLS统计信息
        let stats = transport.get_mtls_stats().await;
        assert!(stats.certificate_renewals > 0, "应该有证书更新统计");
    }

    #[tokio::test]
    async fn test_policy_engine_integration() {
        init_logging();

        let config = create_test_transport_config(0).await.unwrap();
        let device_id = "test-device-policy".to_string();

        let transport = SecureTransport::new(config, device_id).await.unwrap();

        // 测试获取策略引擎统计信息
        let stats = transport.get_policy_stats().await;
        assert_eq!(stats.policy_sets_count, 0, "初始策略集合数量应为0");
    }

    #[tokio::test]
    async fn test_connection_management() {
        init_logging();

        let config = create_test_transport_config(0).await.unwrap();
        let device_id = "test-device-conn".to_string();

        let transport = SecureTransport::new(config, device_id).await.unwrap();

        // 初始状态应该没有活跃连接
        assert_eq!(transport.active_connections_count().await, 0);
        assert!(transport.active_connections().await.is_empty());

        // 测试断开不存在的连接
        let fake_addr = "127.0.0.1:9999".parse().unwrap();
        let disconnect_result = transport.disconnect(fake_addr).await;
        assert!(disconnect_result.is_err(), "断开不存在的连接应该失败");
    }

    #[tokio::test]
    async fn test_message_serialization() {
        init_logging();

        let message = TransportMessage {
            id: "test-msg-001".to_string(),
            message_type: "test".to_string(),
            content: serde_json::json!({"key": "value"}),
            timestamp: std::time::SystemTime::now(),
            sender_id: "test-sender".to_string(),
            receiver_id: Some("test-receiver".to_string()),
        };

        // 测试消息序列化
        let serialized = serde_json::to_vec(&message);
        assert!(serialized.is_ok(), "消息序列化应该成功");

        // 测试消息反序列化
        let serialized_data = serialized.unwrap();
        let deserialized: TransportMessage = serde_json::from_slice(&serialized_data)
            .expect("消息反序列化应该成功");

        let deserialized_msg = deserialized;
        assert_eq!(deserialized_msg.id, message.id);
        assert_eq!(deserialized_msg.message_type, message.message_type);
        assert_eq!(deserialized_msg.sender_id, message.sender_id);
    }
}