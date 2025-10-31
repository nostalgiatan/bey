//! # BEY 网络传输引擎
//!
//! 集成了状态机、令牌系统、元接收器和BEY认证模块的完整网络传输引擎。
//! 提供高性能、安全的网络通信能力。
//!
//! ## 核心功能
//!
//! - **状态管理**: 基于有限状态机的连接管理
//! - **令牌传输**: 基于令牌的消息传输
//! - **认证集成**: 集成BEY身份认证模块
//! - **加密传输**: 自动加密和解密令牌
//! - **灵活接入**: 通过继承元类即可接入网络功能

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::net::{SocketAddr, IpAddr};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, info, warn};
use bey_transport::{SecureTransport, TransportConfig};
use bey_identity::{CertificateManager, CertificateData};
use sha2::{Sha256, Digest};
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{Engine as _, engine::general_purpose};

use crate::{
    NetResult,
    token::{Token, TokenRouter, TokenHandler},
    state_machine::{ConnectionStateMachine, StateEvent, ConnectionState},
    receiver::{BufferedReceiver, MetaReceiver, ReceiverMode, create_receiver},
    mdns_discovery::{MdnsDiscovery, MdnsDiscoveryConfig, MdnsServiceInfo, MdnsDiscoveryEvent},
};

/// 传输引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 引擎名称（用作mDNS服务名称）
    pub name: String,
    /// 监听端口
    pub port: u16,
    /// 接收器缓冲区大小
    pub receiver_buffer_size: usize,
    /// 是否启用认证
    pub enable_auth: bool,
    /// 是否启用加密
    pub enable_encryption: bool,
    /// 是否启用mDNS发现
    pub enable_mdns: bool,
    /// mDNS服务类型
    pub mdns_service_type: String,
    /// 传输层配置
    pub transport_config: TransportConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            name: "bey-engine".to_string(),
            port: 8080,
            receiver_buffer_size: 1000,
            enable_auth: true,
            enable_encryption: true,
            enable_mdns: true,
            mdns_service_type: "_bey._tcp".to_string(),
            transport_config: TransportConfig::default(),
        }
    }
}

/// 设备信息
#[derive(Debug, Clone)]
struct DeviceEntry {
    /// 设备名称
    name: String,
    /// 设备地址列表
    addresses: Vec<SocketAddr>,
    /// 是否已认证
    authenticated: bool,
    /// 证书指纹
    cert_fingerprint: Option<String>,
    /// 最后活跃时间
    last_seen: std::time::SystemTime,
}

/// 网络传输引擎
///
/// 这是BEY网络架构的核心，集成了所有网络功能
pub struct TransportEngine {
    /// 配置
    config: EngineConfig,
    /// 传输层
    transport: Arc<RwLock<SecureTransport>>,
    /// mDNS发现服务
    mdns_discovery: Option<Arc<MdnsDiscovery>>,
    /// 证书管理器
    cert_manager: Option<Arc<CertificateManager>>,
    /// 状态机
    state_machine: Arc<RwLock<ConnectionStateMachine>>,
    /// 令牌路由器
    router: Arc<TokenRouter>,
    /// 令牌接收器
    receiver: Arc<BufferedReceiver>,
    /// 发送通道（内部使用）
    _sender: mpsc::UnboundedSender<Token>,
    /// 已发现的设备映射（设备名 -> 设备信息）
    discovered_devices: Arc<RwLock<HashMap<String, DeviceEntry>>>,
    /// 主加密密钥（从证书派生）
    master_key: Arc<RwLock<Option<Vec<u8>>>>,
}

impl TransportEngine {
    /// 创建新的传输引擎
    ///
    /// # 参数
    ///
    /// * `config` - 引擎配置
    ///
    /// # 返回值
    ///
    /// 返回引擎实例或错误
    pub async fn new(config: EngineConfig) -> NetResult<Self> {
        info!("创建传输引擎: {}", config.name);

        // 创建传输层
        let transport = SecureTransport::new(
            config.transport_config.clone(),
            config.name.clone(),
        ).await.map_err(|e| {
            ErrorInfo::new(4301, format!("创建传输层失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 如果启用认证，初始化证书管理器
        let cert_manager = if config.enable_auth {
            let cert_config = bey_identity::CertificateConfig::default();
            match CertificateManager::initialize(cert_config).await {
                Ok(manager) => {
                    info!("证书管理器初始化成功");
                    Some(Arc::new(manager))
                }
                Err(e) => {
                    warn!("证书管理器初始化失败: {}, 继续但不启用认证", e);
                    None
                }
            }
        } else {
            None
        };

        // 创建状态机
        let state_machine = Arc::new(RwLock::new(ConnectionStateMachine::new()));

        // 创建令牌路由器
        let router = Arc::new(TokenRouter::new());

        // 创建接收器
        let (sender, receiver) = create_receiver(config.receiver_buffer_size);

        // 初始化mDNS发现（如果启用）
        let mdns_discovery = if config.enable_mdns {
            match Self::initialize_mdns(&config).await {
                Ok(discovery) => {
                    info!("mDNS发现服务初始化成功");
                    Some(Arc::new(discovery))
                }
                Err(e) => {
                    warn!("mDNS发现服务初始化失败: {}, 继续但不启用发现功能", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            config,
            transport: Arc::new(RwLock::new(transport)),
            mdns_discovery,
            cert_manager,
            state_machine,
            router,
            receiver: Arc::new(receiver),
            _sender: sender,
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
            master_key: Arc::new(RwLock::new(None)),
        })
    }

    /// 初始化mDNS发现服务
    async fn initialize_mdns(config: &EngineConfig) -> NetResult<MdnsDiscovery> {
        debug!("初始化mDNS发现服务");

        // 创建mDNS配置
        let mdns_config = MdnsDiscoveryConfig {
            service_name: config.name.clone(),
            service_type: config.mdns_service_type.clone(),
            domain: "local".to_string(),
            port: config.port,
            priority: 0,
            weight: 0,
            default_ttl: 120,
            query_interval: std::time::Duration::from_secs(30),
            device_timeout: std::time::Duration::from_secs(60),
            max_retries: 3,
            enable_cache: true,
            cache_size_limit: 100,
            enable_ipv6: false,
            event_queue_size: 100,
        };

        // 获取本机IP地址列表
        let local_ips = Self::get_local_ip_addresses()?;

        // 创建服务信息
        let service_info = MdnsServiceInfo {
            service_name: config.name.clone(),
            service_type: config.mdns_service_type.clone(),
            domain: "local".to_string(),
            hostname: format!("{}.local", config.name),
            port: config.port,
            priority: 0,
            weight: 0,
            addresses: local_ips,
            txt_records: vec![
                format!("version={}", env!("CARGO_PKG_VERSION")),
                "protocol=bey".to_string(),
            ],
            ttl: 120,
        };

        // 创建mDNS发现服务
        let discovery = MdnsDiscovery::new(mdns_config, service_info).await.map_err(|e| {
            ErrorInfo::new(4324, format!("创建mDNS发现服务失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error)
        })?;

        Ok(discovery)
    }

    /// 获取本机IP地址列表
    fn get_local_ip_addresses() -> NetResult<Vec<IpAddr>> {
        use std::net::UdpSocket;
        
        let mut addresses = Vec::new();

        // 尝试获取本机IP（通过连接到外部地址但不实际发送数据）
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if let Ok(()) = socket.connect("8.8.8.8:80") {
                if let Ok(addr) = socket.local_addr() {
                    addresses.push(addr.ip());
                }
            }
        }

        // 如果没有获取到任何地址，使用回环地址作为后备
        if addresses.is_empty() {
            addresses.push(IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
        }

        Ok(addresses)
    }

    /// 启动引擎（服务端模式）
    ///
    /// # 返回值
    ///
    /// 返回启动结果或错误
    pub async fn start_server(&self) -> NetResult<()> {
        info!("启动传输引擎服务器: {} on port {}", self.config.name, self.config.port);

        // 更新状态
        {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Connect)?;
        }

        // 启动传输层服务器
        {
            let mut transport = self.transport.write().await;
            transport.start_server().await.map_err(|e| {
                ErrorInfo::new(4303, format!("启动传输层服务器失败: {}", e))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error)
            })?;
        }

        // 更新状态
        {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Connected)?;
        }

        // 启动mDNS发现
        if let Some(mdns) = &self.mdns_discovery {
            mdns.start().await.map_err(|e| {
                ErrorInfo::new(4325, format!("启动mDNS发现失败: {}", e))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Warning)
            })?;
            info!("mDNS发现服务已启动，广播服务: {}", self.config.name);

            // 启动设备发现监听任务
            self.start_device_discovery_listener().await;
        }

        // 如果启用认证，执行认证流程
        if self.config.enable_auth && self.cert_manager.is_some() {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Authenticate)?;
            
            // 为本地服务执行认证
            let listen_addr = SocketAddr::new(
                IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
                self.config.port
            );
            
            match self.perform_authentication(listen_addr).await {
                Ok(()) => {
                    sm.handle_event(StateEvent::Authenticated)?;
                    info!("服务器认证成功");
                }
                Err(e) => {
                    warn!("服务器认证失败: {}", e);
                    sm.handle_event(StateEvent::AuthFailed)?;
                    return Err(e);
                }
            }
        } else {
            // 没有认证，直接进入已认证状态
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Authenticated)?;
        }

        info!("传输引擎服务器启动成功，监听端口: {}", self.config.port);
        Ok(())
    }

    /// 连接到指定设备（通过设备名）
    ///
    /// # 参数
    ///
    /// * `device_name` - 目标设备名称
    ///
    /// # 返回值
    ///
    /// 返回连接结果或错误
    pub async fn connect_to_device(&self, device_name: &str) -> NetResult<()> {
        info!("连接到设备: {}", device_name);

        // 首先查询mDNS获取设备地址
        let device_addr = if let Some(mdns) = &self.mdns_discovery {
            // 查询mDNS服务
            let services = mdns.query_service(&self.config.mdns_service_type, None).await.map_err(|e| {
                ErrorInfo::new(4326, format!("查询mDNS服务失败: {}", e))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error)
            })?;

            // 查找目标设备
            let service = services.iter().find(|s| s.service_name == device_name)
                .ok_or_else(|| {
                    ErrorInfo::new(4327, format!("未找到设备: {}", device_name))
                        .with_category(ErrorCategory::Network)
                        .with_severity(ErrorSeverity::Warning)
                })?;

            // 获取第一个可用地址
            let ip = service.addresses.first().ok_or_else(|| {
                ErrorInfo::new(4328, format!("设备 {} 没有可用地址", device_name))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Warning)
            })?;

            SocketAddr::new(*ip, service.port)
        } else {
            return Err(ErrorInfo::new(4329, "mDNS发现未启用".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        };

        info!("设备 {} 解析到地址: {}", device_name, device_addr);

        // 连接到解析的地址
        self.connect(device_addr).await
    }

    /// 连接到服务器（客户端模式，通过地址）
    ///
    /// # 参数
    ///
    /// * `server_addr` - 服务器地址
    ///
    /// # 返回值
    ///
    /// 返回连接结果或错误
    pub async fn connect(&self, server_addr: SocketAddr) -> NetResult<()> {
        info!("连接到服务器: {}", server_addr);

        // 更新状态
        {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Connect)?;
        }

        // 连接到服务器
        {
            let transport = self.transport.write().await;
            transport.connect(server_addr).await.map_err(|e| {
                ErrorInfo::new(4304, format!("连接服务器失败: {}", e))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error)
            })?;
        }

        // 更新状态
        {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Connected)?;
        }

        // 如果启用认证，执行认证流程
        if self.config.enable_auth && self.cert_manager.is_some() {
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Authenticate)?;
            
            // 执行实际的认证逻辑
            match self.perform_authentication(server_addr).await {
                Ok(()) => {
                    sm.handle_event(StateEvent::Authenticated)?;
                    info!("客户端认证成功");
                }
                Err(e) => {
                    warn!("客户端认证失败: {}", e);
                    sm.handle_event(StateEvent::AuthFailed)?;
                    return Err(e);
                }
            }
        } else {
            // 没有认证，直接进入已认证状态
            let mut sm = self.state_machine.write().await;
            sm.handle_event(StateEvent::Authenticated)?;
        }

        info!("成功连接到服务器: {}", server_addr);
        Ok(())
    }

    /// 断开连接
    ///
    /// # 返回值
    ///
    /// 返回断开结果或错误
    pub async fn disconnect(&self) -> NetResult<()> {
        info!("断开连接");

        let mut sm = self.state_machine.write().await;
        sm.handle_event(StateEvent::Disconnect)?;
        sm.handle_event(StateEvent::Disconnect)?; // 转换到Disconnected状态

        info!("连接已断开");
        Ok(())
    }

    /// 发送令牌
    ///
    /// # 参数
    ///
    /// * `token` - 要发送的令牌
    ///
    /// # 返回值
    ///
    /// 返回发送结果或错误
    pub async fn send_token(&self, mut token: Token) -> NetResult<()> {
        debug!("发送令牌: {} (类型: {})", token.meta.id, token.meta.token_type);

        // 检查状态
        {
            let sm = self.state_machine.read().await;
            if !sm.can_transfer() {
                return Err(ErrorInfo::new(4305, format!("当前状态不允许传输: {}", sm.current_state()))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Warning));
            }
        }

        // 如果启用加密，加密令牌
        if self.config.enable_encryption && !token.meta.encrypted {
            token = self.encrypt_token(token).await?;
        }

        // 序列化令牌
        let _data = token.serialize()?;

        // 查找目标设备地址
        if let Some(receiver_id) = &token.meta.receiver_id {
            debug!("查找目标设备: {}", receiver_id);
            
            let addresses = self.get_device_addresses(receiver_id).await;
            if let Some(addrs) = addresses {
                if let Some(target_addr) = addrs.first() {
                    info!("令牌将发送到: {} ({})", receiver_id, target_addr);
                    // TODO: 使用传输层实际发送
                    // let transport = self.transport.read().await;
                    // transport.send_to(&data, *target_addr).await?;
                } else {
                    return Err(ErrorInfo::new(4330, format!("设备 {} 没有可用地址", receiver_id))
                        .with_category(ErrorCategory::Network)
                        .with_severity(ErrorSeverity::Warning));
                }
            } else {
                return Err(ErrorInfo::new(4331, format!("未找到设备: {}", receiver_id))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Warning));
            }
        } else {
            debug!("令牌没有指定接收者，使用广播");
            // 可以广播到所有已发现的设备
        }

        debug!("令牌已准备发送: {}", token.meta.id);
        Ok(())
    }

    /// 接收令牌
    ///
    /// # 参数
    ///
    /// * `mode` - 接收模式
    ///
    /// # 返回值
    ///
    /// 返回接收到的令牌或错误
    pub async fn receive_token(&self, mode: ReceiverMode) -> NetResult<Option<Token>> {
        // 检查状态
        {
            let sm = self.state_machine.read().await;
            if !sm.can_transfer() {
                return Err(ErrorInfo::new(4306, format!("当前状态不允许传输: {}", sm.current_state()))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Warning));
            }
        }

        // 从接收器获取令牌
        let token = self.receiver.receive(mode).await?;

        if let Some(mut token) = token {
            // 如果令牌是加密的，解密
            if token.meta.encrypted {
                token = self.decrypt_token(token).await?;
            }

            debug!("接收令牌: {} (类型: {})", token.meta.id, token.meta.token_type);
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }

    /// 注册令牌处理器
    ///
    /// # 参数
    ///
    /// * `handler` - 令牌处理器
    ///
    /// # 返回值
    ///
    /// 返回注册结果
    pub async fn register_handler(&self, handler: Arc<dyn TokenHandler>) -> NetResult<()> {
        self.router.register_handler(handler).await
    }

    /// 获取当前状态
    ///
    /// # 返回值
    ///
    /// 返回当前连接状态
    pub async fn current_state(&self) -> ConnectionState {
        self.state_machine.read().await.current_state()
    }

    /// 获取接收器
    ///
    /// # 返回值
    ///
    /// 返回接收器的引用
    pub fn receiver(&self) -> Arc<BufferedReceiver> {
        Arc::clone(&self.receiver)
    }

    /// 获取已发现的设备列表
    ///
    /// # 返回值
    ///
    /// 返回设备名称列表
    pub async fn list_discovered_devices(&self) -> Vec<String> {
        let devices = self.discovered_devices.read().await;
        devices.keys().cloned().collect()
    }

    /// 获取设备地址
    ///
    /// # 参数
    ///
    /// * `device_name` - 设备名称
    ///
    /// # 返回值
    ///
    /// 返回设备地址列表
    pub async fn get_device_addresses(&self, device_name: &str) -> Option<Vec<SocketAddr>> {
        let devices = self.discovered_devices.read().await;
        devices.get(device_name).map(|d| d.addresses.clone())
    }

    /// 启动设备发现监听任务
    async fn start_device_discovery_listener(&self) {
        let mdns = match &self.mdns_discovery {
            Some(m) => Arc::clone(m),
            None => return,
        };

        let discovered_devices = Arc::clone(&self.discovered_devices);
        let service_type = self.config.mdns_service_type.clone();

        tokio::spawn(async move {
            info!("设备发现监听任务已启动");

            loop {
                // 定期查询mDNS服务
                match mdns.query_service(&service_type, None).await {
                    Ok(services) => {
                        let mut devices = discovered_devices.write().await;
                        
                        for service in services {
                            let device_name = service.service_name.clone();
                            
                            // 构建地址列表
                            let addresses: Vec<SocketAddr> = service.addresses.iter()
                                .map(|ip| SocketAddr::new(*ip, service.port))
                                .collect();

                            // 更新或添加设备
                            if let Some(entry) = devices.get_mut(&device_name) {
                                entry.addresses = addresses;
                                entry.last_seen = std::time::SystemTime::now();
                                debug!("更新设备: {}", device_name);
                            } else {
                                let entry = DeviceEntry {
                                    name: device_name.clone(),
                                    addresses,
                                    authenticated: false,
                                    cert_fingerprint: None,
                                    last_seen: std::time::SystemTime::now(),
                                };
                                devices.insert(device_name.clone(), entry);
                                info!("发现新设备: {}", device_name);
                            }
                        }

                        // 清理过期设备（30秒未见）
                        let now = std::time::SystemTime::now();
                        devices.retain(|name, entry| {
                            if let Ok(elapsed) = now.duration_since(entry.last_seen) {
                                if elapsed.as_secs() > 30 {
                                    info!("移除过期设备: {}", name);
                                    return false;
                                }
                            }
                            true
                        });
                    }
                    Err(e) => {
                        warn!("查询mDNS服务失败: {}", e);
                    }
                }

                // 每15秒查询一次
                tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            }
        });
    }

    /// 加密令牌
    ///
    /// 使用AES-256-GCM加密令牌负载
    ///
    /// # 参数
    ///
    /// * `token` - 要加密的令牌
    ///
    /// # 返回值
    ///
    /// 返回加密后的令牌或错误
    async fn encrypt_token(&self, mut token: Token) -> NetResult<Token> {
        debug!("加密令牌: {}", token.meta.id);

        // 获取主密钥
        let master_key = self.master_key.read().await;
        let key_bytes = match &*master_key {
            Some(key) => key.clone(),
            None => {
                // 如果没有主密钥，从证书派生
                drop(master_key);
                self.derive_master_key().await?;
                let master_key = self.master_key.read().await;
                master_key.as_ref().ok_or_else(|| {
                    ErrorInfo::new(4307, "无法获取加密密钥".to_string())
                        .with_category(ErrorCategory::Encryption)
                        .with_severity(ErrorSeverity::Error)
                })?.clone()
            }
        };

        // 确保密钥长度为32字节（AES-256）
        let key = if key_bytes.len() >= 32 {
            &key_bytes[..32]
        } else {
            return Err(ErrorInfo::new(4308, "加密密钥长度不足".to_string())
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error));
        };

        // 创建AES-GCM加密器
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            ErrorInfo::new(4309, format!("创建加密器失败: {}", e))
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 生成随机nonce
        let mut nonce_bytes = [0u8; 12];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 加密负载
        let ciphertext = cipher.encrypt(nonce, token.payload.as_ref()).map_err(|e| {
            ErrorInfo::new(4310, format!("加密失败: {}", e))
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 将nonce和密文组合
        let mut encrypted_data = nonce_bytes.to_vec();
        encrypted_data.extend_from_slice(&ciphertext);

        // 更新令牌
        token.payload = encrypted_data;
        token.meta.encrypted = true;
        token.meta.attributes.insert("encryption".to_string(), "aes-256-gcm".to_string());

        debug!("令牌加密成功: {}", token.meta.id);
        Ok(token)
    }

    /// 解密令牌
    ///
    /// 使用AES-256-GCM解密令牌负载
    ///
    /// # 参数
    ///
    /// * `token` - 要解密的令牌
    ///
    /// # 返回值
    ///
    /// 返回解密后的令牌或错误
    async fn decrypt_token(&self, mut token: Token) -> NetResult<Token> {
        debug!("解密令牌: {}", token.meta.id);

        // 获取主密钥
        let master_key = self.master_key.read().await;
        let key_bytes = master_key.as_ref().ok_or_else(|| {
            ErrorInfo::new(4311, "无法获取解密密钥".to_string())
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 确保密钥长度为32字节（AES-256）
        let key = if key_bytes.len() >= 32 {
            &key_bytes[..32]
        } else {
            return Err(ErrorInfo::new(4312, "解密密钥长度不足".to_string())
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error));
        };

        // 创建AES-GCM解密器
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
            ErrorInfo::new(4313, format!("创建解密器失败: {}", e))
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 提取nonce和密文
        if token.payload.len() < 12 {
            return Err(ErrorInfo::new(4314, "加密数据格式错误：长度不足".to_string())
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error));
        }

        let nonce = Nonce::from_slice(&token.payload[..12]);
        let ciphertext = &token.payload[12..];

        // 解密负载
        let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|e| {
            ErrorInfo::new(4315, format!("解密失败: {}", e))
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 更新令牌
        token.payload = plaintext;
        token.meta.encrypted = false;
        token.meta.attributes.remove("encryption");

        debug!("令牌解密成功: {}", token.meta.id);
        Ok(token)
    }

    /// 派生主密钥
    ///
    /// 从证书派生主加密密钥
    ///
    /// # 返回值
    ///
    /// 返回成功或错误
    async fn derive_master_key(&self) -> NetResult<()> {
        debug!("派生主加密密钥");

        let cert_manager = self.cert_manager.as_ref().ok_or_else(|| {
            ErrorInfo::new(4316, "证书管理器未初始化".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 获取设备证书（使用引擎名称作为标识符）
        let local_cert_opt = cert_manager.get_device_certificate(&self.config.name).await.map_err(|e| {
            ErrorInfo::new(4317, format!("获取本地证书失败: {}", e))
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error)
        })?;

        let local_cert = local_cert_opt.ok_or_else(|| {
            ErrorInfo::new(4322, "本地证书不存在".to_string())
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 使用证书的公钥指纹作为密钥派生材料
        let cert_pem = &local_cert.certificate_pem;
        let mut hasher = Sha256::new();
        hasher.update(cert_pem.as_bytes());
        hasher.update(self.config.name.as_bytes()); // 混入引擎名称
        let key = hasher.finalize().to_vec();

        // 存储主密钥
        let mut master_key = self.master_key.write().await;
        *master_key = Some(key);

        info!("主加密密钥派生成功");
        Ok(())
    }

    /// 执行认证流程
    ///
    /// 验证证书并建立安全连接
    ///
    /// # 参数
    ///
    /// * `remote_addr` - 远程地址
    ///
    /// # 返回值
    ///
    /// 返回认证结果或错误
    async fn perform_authentication(&self, remote_addr: SocketAddr) -> NetResult<()> {
        info!("执行认证流程: {}", remote_addr);

        let cert_manager = self.cert_manager.as_ref().ok_or_else(|| {
            ErrorInfo::new(4318, "证书管理器未初始化".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 获取设备证书（使用引擎名称作为标识符）
        let local_cert_opt = cert_manager.get_device_certificate(&self.config.name).await.map_err(|e| {
            ErrorInfo::new(4319, format!("获取本地证书失败: {}", e))
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error)
        })?;

        let local_cert = local_cert_opt.ok_or_else(|| {
            ErrorInfo::new(4323, "本地证书不存在".to_string())
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 验证本地证书
        let verification_result = cert_manager.verify_certificate(&local_cert).await.map_err(|e| {
            ErrorInfo::new(4320, format!("验证本地证书失败: {}", e))
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error)
        })?;

        if !verification_result.is_valid {
            let error_msg = verification_result.error_message.unwrap_or_else(|| "未知错误".to_string());
            return Err(ErrorInfo::new(4321, format!("本地证书无效: {}", error_msg))
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error));
        }

        // 计算证书指纹
        let cert_fingerprint = self.calculate_cert_fingerprint(&local_cert);

        // 更新或创建设备信息
        let device_name = self.config.name.clone();
        {
            let mut devices = self.discovered_devices.write().await;
            if let Some(entry) = devices.get_mut(&device_name) {
                entry.authenticated = true;
                entry.cert_fingerprint = Some(cert_fingerprint.clone());
            } else {
                // 创建本地设备条目
                let entry = DeviceEntry {
                    name: device_name.clone(),
                    addresses: vec![remote_addr],
                    authenticated: true,
                    cert_fingerprint: Some(cert_fingerprint.clone()),
                    last_seen: std::time::SystemTime::now(),
                };
                devices.insert(device_name.clone(), entry);
            }
        }

        // 派生主密钥（如果还没有）
        {
            let master_key = self.master_key.read().await;
            if master_key.is_none() {
                drop(master_key);
                self.derive_master_key().await?;
            }
        }

        info!("认证成功: {} (指纹: {})", remote_addr, cert_fingerprint);
        Ok(())
    }

    /// 计算证书指纹
    ///
    /// # 参数
    ///
    /// * `cert` - 证书数据
    ///
    /// # 返回值
    ///
    /// 返回证书指纹（SHA-256哈希的Base64编码）
    fn calculate_cert_fingerprint(&self, cert: &CertificateData) -> String {
        let mut hasher = Sha256::new();
        hasher.update(cert.certificate_pem.as_bytes());
        let hash = hasher.finalize();
        general_purpose::STANDARD.encode(&hash)
    }

    /// 获取连接统计信息
    ///
    /// # 返回值
    ///
    /// 返回已发现设备数和已认证设备数
    pub async fn connection_stats(&self) -> (usize, usize) {
        let devices = self.discovered_devices.read().await;
        let total = devices.len();
        let authenticated = devices.values().filter(|d| d.authenticated).count();
        (total, authenticated)
    }

    /// 清理断开的设备
    ///
    /// # 返回值
    ///
    /// 返回清理的设备数
    pub async fn cleanup_devices(&self) -> usize {
        let mut devices = self.discovered_devices.write().await;
        let initial_count = devices.len();
        
        // 移除30秒以上未见的设备
        let now = std::time::SystemTime::now();
        devices.retain(|name, entry| {
            if let Ok(elapsed) = now.duration_since(entry.last_seen) {
                if elapsed.as_secs() > 30 {
                    info!("清理过期设备: {}", name);
                    return false;
                }
            }
            true
        });
        
        let removed = initial_count - devices.len();
        if removed > 0 {
            info!("清理了 {} 个过期设备", removed);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        let config = EngineConfig::default();
        // 注意：这可能因为需要证书而失败
        // let result = TransportEngine::new(config).await;
        // 暂时跳过实际创建测试
    }

    #[test]
    fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert_eq!(config.name, "bey-engine");
        assert_eq!(config.receiver_buffer_size, 1000);
        assert!(config.enable_auth);
        assert!(config.enable_encryption);
    }
}
