//! # BEY网络存储服务器
//!
//! 提供基于BEY网络的分布式文件存储服务器功能。
//! 支持文件读写、目录操作、文件信息查询等功能。
//!
//! ## 核心功能
//!
//! - **网络存储服务**: 基于BEY协议的分布式文件存储
//! - **安全认证**: 支持设备认证和访问控制
//! - **并发处理**: 支持多客户端并发访问
//! - **错误恢复**: 完善的错误处理和恢复机制
//! - **性能监控**: 实时的存储性能监控

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tokio::sync::RwLock;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::{info, error, debug};
use bey_transport::{SecureTransport, TransportConfig, TransportMessage};
use quinn;
use bey_discovery::{MdnsDiscovery, MdnsDiscoveryConfig, MdnsServiceInfo};
use uuid::Uuid;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::TransferResult;

/// 存储服务器请求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum StorageRequestType {
    /// 读取文件块
    ReadChunk,
    /// 写入文件块
    WriteChunk,
    /// 获取文件信息
    GetFileInfo,
    /// 创建目录
    CreateDir,
    /// 删除文件
    DeleteFile,
    /// 检查文件是否存在
    FileExists,
}

/// 存储服务器请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct StorageRequest {
    /// 请求类型
    pub request_type: StorageRequestType,
    /// 文件路径
    pub path: String,
    /// 偏移量（用于读写操作）
    pub offset: Option<u64>,
    /// 数据大小（用于读取操作）
    pub size: Option<usize>,
    /// 数据内容（用于写入操作，base64编码）
    pub data: Option<String>,
    /// 认证令牌
    pub auth_token: String,
    /// 请求ID
    pub request_id: String,
}

/// 存储服务器响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct StorageResponse {
    /// 是否成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error: Option<String>,
    /// 响应数据
    pub data: Option<serde_json::Value>,
    /// 请求ID
    pub request_id: String,
}

/// 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerFileInfo {
    /// 文件路径
    pub path: String,
    /// 文件大小
    pub size: u64,
    /// 修改时间（Unix时间戳）
    pub modified: u64,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件权限
    pub permissions: Option<u32>,
    /// 文件哈希
    pub hash: Option<String>,
}

impl ServerFileInfo {
    /// 创建新的文件信息
    pub fn new(path: String, size: u64, modified: u64, is_dir: bool) -> Self {
        Self {
            path,
            size,
            modified,
            is_dir,
            permissions: None,
            hash: None,
        }
    }

    /// 设置文件权限
    pub fn with_permissions(mut self, permissions: u32) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// 设置文件哈希
    pub fn with_hash(mut self, hash: String) -> Self {
        self.hash = Some(hash);
        self
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.path)
            .extension()
            .and_then(|ext| ext.to_str())
    }

    /// 获取文件名
    pub fn filename(&self) -> &str {
        std::path::Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }
}

/// 存储服务器配置
#[derive(Debug, Clone)]
pub struct StorageServerConfig {
    /// 服务器基础路径
    pub base_path: PathBuf,
    /// 传输层配置
    pub transport_config: TransportConfig,
    /// 发现服务配置
    pub discovery_config: MdnsDiscoveryConfig,
    /// 服务器设备ID
    pub device_id: String,
    /// 服务器名称
    pub server_name: String,
    /// 有效认证令牌
    pub valid_tokens: Vec<String>,
    /// 最大文件大小（字节）
    pub max_file_size: u64,
    /// 是否启用认证
    pub enable_auth: bool,
}

impl StorageServerConfig {
    /// 创建新的存储服务器配置
    pub fn new(base_path: PathBuf, device_id: String) -> Self {
        Self {
            base_path,
            transport_config: TransportConfig::default(),
            discovery_config: MdnsDiscoveryConfig::default(),
            device_id,
            server_name: "BEY Storage Server".to_string(),
            valid_tokens: vec![],
            max_file_size: 10 * 1024 * 1024 * 1024, // 10GB
            enable_auth: false,
        }
    }

    /// 设置服务器名称
    pub fn with_server_name(mut self, name: String) -> Self {
        self.server_name = name;
        self
    }

    /// 添加认证令牌
    pub fn with_token(mut self, token: String) -> Self {
        self.valid_tokens.push(token);
        self.enable_auth = true;
        self
    }

    /// 设置最大文件大小
    pub fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }

    /// 启用认证
    pub fn with_auth(mut self, enable: bool) -> Self {
        self.enable_auth = enable;
        self
    }
}

impl Default for StorageServerConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("./bey-storage"),
            transport_config: TransportConfig::default(),
            discovery_config: MdnsDiscoveryConfig::default(),
            device_id: format!("storage-server-{}", Uuid::new_v4()),
            server_name: "BEY Storage Server".to_string(),
            valid_tokens: vec![],
            max_file_size: 10 * 1024 * 1024 * 1024, // 10GB
            enable_auth: false,
        }
    }
}

/// 存储服务器统计信息
#[derive(Debug, Default)]
pub struct StorageServerStats {
    /// 总请求数
    pub total_requests: AtomicU64,
    /// 成功请求数
    pub successful_requests: AtomicU64,
    /// 失败请求数
    pub failed_requests: AtomicU64,
    /// 总读取字节数
    pub total_bytes_read: AtomicU64,
    /// 总写入字节数
    pub total_bytes_written: AtomicU64,
    /// 当前连接数
    pub active_connections: AtomicU64,
}

impl StorageServerStats {
    /// 创建新的统计信息
    pub fn new() -> Self {
        Self::default()
    }

    /// 增加总请求数
    pub fn increment_total_requests(&self) {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 增加成功请求数
    pub fn increment_successful_requests(&self) {
        self.successful_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 增加失败请求数
    pub fn increment_failed_requests(&self) {
        self.failed_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 增加读取字节数
    pub fn add_bytes_read(&self, bytes: u64) {
        self.total_bytes_read.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// 增加写入字节数
    pub fn add_bytes_written(&self, bytes: u64) {
        self.total_bytes_written.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// 设置活跃连接数
    pub fn set_active_connections(&self, count: u64) {
        self.active_connections.store(count, std::sync::atomic::Ordering::Relaxed);
    }

    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(std::sync::atomic::Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            let successful = self.successful_requests.load(std::sync::atomic::Ordering::Relaxed);
            successful as f64 / total as f64
        }
    }

    /// 重置统计信息
    pub fn reset(&self) {
        self.total_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.successful_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.failed_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.total_bytes_read.store(0, std::sync::atomic::Ordering::Relaxed);
        self.total_bytes_written.store(0, std::sync::atomic::Ordering::Relaxed);
        self.active_connections.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// BEY网络存储服务器
pub struct BeyStorageServer {
    /// 配置信息
    config: StorageServerConfig,
    /// 安全传输层
    transport: Arc<RwLock<SecureTransport>>,
    /// 设备发现服务
    discovery: Arc<MdnsDiscovery>,
    /// 服务器统计信息
    stats: Arc<StorageServerStats>,
    /// 运行状态
    is_running: Arc<tokio::sync::RwLock<bool>>,
}

impl BeyStorageServer {
    /// 创建新的存储服务器实例
    ///
    /// # 参数
    ///
    /// * `config` - 服务器配置
    ///
    /// # 返回值
    ///
    /// 返回服务器实例或错误信息
    pub async fn new(config: StorageServerConfig) -> TransferResult<Self> {
        info!("创建BEY网络存储服务器，设备ID: {}", config.device_id);

        // 确保基础目录存在
        if let Err(e) = fs::create_dir_all(&config.base_path).await {
            error!("创建存储基础目录失败: {}", e);
            return Err(ErrorInfo::new(
                8001,
                format!("创建存储基础目录失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error));
        }

        // 创建安全传输层
        let transport = Arc::new(RwLock::new(SecureTransport::new(
            config.transport_config.clone(),
            config.device_id.clone()
        ).await.map_err(|e| {
            ErrorInfo::new(8002, format!("创建传输层失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error)
        })?));

        // 创建设备发现服务
        let service_info = MdnsServiceInfo {
            service_name: config.server_name.clone(),
            service_type: "_bey._tcp".to_string(),
            domain: "local".to_string(),
            hostname: "storage-server".to_string(),
            port: 8443,
            addresses: vec!["0.0.0.0".parse().unwrap()],
            txt_records: vec![
                format!("device_id={}", config.device_id),
                "type=storage-server".to_string(),
                "capabilities=file-storage,file-read,file-write,directory-operations".to_string(),
                format!("max_file_size={}", config.max_file_size),
                format!("auth_required={}", config.enable_auth),
            ],
            ttl: 120,
            priority: 0,
            weight: 0,
        };

        let discovery = Arc::new(MdnsDiscovery::new(
            config.discovery_config.clone(),
            service_info
        ).await.map_err(|e| {
            ErrorInfo::new(8003, format!("创建发现服务失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error)
        })?);

        Ok(Self {
            config,
            transport,
            discovery,
            stats: Arc::new(StorageServerStats::default()),
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
        })
    }

    /// 启动存储服务器
    ///
    /// # 返回值
    ///
    /// 返回启动结果或错误信息
    pub async fn start(&mut self) -> TransferResult<()> {
        info!("启动BEY网络存储服务器");

        // 检查是否已经运行
        {
            let is_running = self.is_running.read().await;
            if *is_running {
                return Err(ErrorInfo::new(8004, "存储服务器已经在运行".to_string())
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Warning));
            }
        }

        // 启动传输层服务器
        {
            let mut transport = self.transport.write().await;
            transport.start_server().await.map_err(|e| {
                ErrorInfo::new(8005, format!("启动传输层服务器失败: {}", e))
                    .with_category(ErrorCategory::Network)
                    .with_severity(ErrorSeverity::Error)
            })?;
        }

        // 设置运行状态
        {
            let mut is_running = self.is_running.write().await;
            *is_running = true;
        }

        // 启动消息处理循环
        self.start_message_handler().await;

        info!("BEY网络存储服务器启动成功");
        Ok(())
    }

    /// 停止存储服务器
    pub async fn stop(&self) {
        info!("停止BEY网络存储服务器");

        // 设置停止状态
        {
            let mut is_running = self.is_running.write().await;
            *is_running = false;
        }

        // 停止传输层
        let transport = self.transport.write().await;
        transport.stop().await;

        info!("BEY网络存储服务器已停止");
    }

    /// 获取服务器统计信息快照
    pub async fn get_stats_snapshot(&self) -> StorageServerStatsSnapshot {
        StorageServerStatsSnapshot {
            total_requests: self.stats.total_requests.load(Ordering::Relaxed),
            successful_requests: self.stats.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.stats.failed_requests.load(Ordering::Relaxed),
            total_bytes_read: self.stats.total_bytes_read.load(Ordering::Relaxed),
            total_bytes_written: self.stats.total_bytes_written.load(Ordering::Relaxed),
            active_connections: self.stats.active_connections.load(Ordering::Relaxed),
        }
    }

    /// 启动消息处理循环
    async fn start_message_handler(&self) {
        let _transport = Arc::clone(&self.transport);
        let _stats = Arc::clone(&self.stats);
        let _config = self.config.clone();
        let is_running = Arc::clone(&self.is_running);

        tokio::spawn(async move {
            info!("启动消息处理循环");

            while *is_running.read().await {
                // TODO: 实现消息接收逻辑
                // 需要等待 SecureTransport 提供接收消息的API
                tokio::time::sleep(Duration::from_secs(1)).await;
                /*
                match transport.receive_message_from_any().await {
                    Ok((connection, message)) => {
                        stats.active_connections.fetch_add(1, Ordering::Relaxed);
                        stats.total_requests.fetch_add(1, Ordering::Relaxed);

                        // 处理消息
                        let result = Self::handle_storage_request(
                            &config,
                            &message,
                            &stats,
                        ).await;

                        // 发送响应
                        let response = match result {
                            Ok(response_data) => TransportMessage {
                                id: Uuid::new_v4().to_string(),
                                message_type: "storage_response".to_string(),
                                content: response_data,
                                timestamp: SystemTime::now(),
                                sender_id: config.device_id.clone(),
                                receiver_id: Some(message.sender_id.clone()),
                            },
                            Err(e) => TransportMessage {
                                id: Uuid::new_v4().to_string(),
                                message_type: "storage_response".to_string(),
                                content: serde_json::json!({
                                    "success": false,
                                    "error": e.to_string(),
                                    "request_id": message.id
                                }),
                                timestamp: SystemTime::now(),
                                sender_id: config.device_id.clone(),
                                receiver_id: Some(message.sender_id.clone()),
                            },
                        };

                        if let Err(e) = transport.send_message(&connection, response).await {
                            error!("发送响应失败: {}", e);
                            stats.failed_requests.fetch_add(1, Ordering::Relaxed);
                        } else {
                            stats.successful_requests.fetch_add(1, Ordering::Relaxed);
                        }

                        stats.active_connections.fetch_sub(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        debug!("接收消息失败: {}", e);
                        // 短暂等待后继续
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                */
            }

            info!("消息处理循环已停止");
        });
    }

    /// 处理存储请求
    async fn handle_storage_request(
        config: &StorageServerConfig,
        message: &TransportMessage,
        stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        debug!("处理存储请求: {}", message.message_type);

        // 解析请求
        let request: StorageRequest = serde_json::from_value(message.content.clone()).map_err(|e| {
            ErrorInfo::new(8010, format!("解析存储请求失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 验证认证令牌
        if config.enable_auth && !config.valid_tokens.contains(&request.auth_token) {
            stats.failed_requests.fetch_add(1, Ordering::Relaxed);
            return Err(ErrorInfo::new(8011, "认证失败: 无效的认证令牌".to_string())
                .with_category(ErrorCategory::Authentication)
                .with_severity(ErrorSeverity::Error));
        }

        // 根据请求类型处理
        let result = match request.request_type {
            StorageRequestType::ReadChunk => {
                Self::handle_read_chunk(config, &request, stats).await
            }
            StorageRequestType::WriteChunk => {
                Self::handle_write_chunk(config, &request, stats).await
            }
            StorageRequestType::GetFileInfo => {
                Self::handle_get_file_info(config, &request, stats).await
            }
            StorageRequestType::CreateDir => {
                Self::handle_create_dir(config, &request, stats).await
            }
            StorageRequestType::DeleteFile => {
                Self::handle_delete_file(config, &request, stats).await
            }
            StorageRequestType::FileExists => {
                Self::handle_file_exists(config, &request, stats).await
            }
        };

        match result {
            Ok(data) => {
                stats.successful_requests.fetch_add(1, Ordering::Relaxed);
                Ok(serde_json::json!({
                    "success": true,
                    "data": data,
                    "request_id": request.request_id
                }))
            }
            Err(e) => {
                stats.failed_requests.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// 处理读取文件块请求
    async fn handle_read_chunk(
        config: &StorageServerConfig,
        request: &StorageRequest,
        stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));
        let offset = request.offset.unwrap_or(0);
        let size = request.size.unwrap_or(4096);

        debug!("读取文件块: {} (偏移: {}, 大小: {})", full_path.display(), offset, size);

        // 打开文件
        let mut file = fs::File::open(&full_path).await.map_err(|e| {
            ErrorInfo::new(8020, format!("打开文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 定位到指定偏移
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| {
            ErrorInfo::new(8021, format!("文件定位失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 读取数据
        let mut buffer = vec![0u8; size];
        let bytes_read = file.read(&mut buffer).await.map_err(|e| {
            ErrorInfo::new(8022, format!("读取文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 更新统计信息
        stats.total_bytes_read.fetch_add(bytes_read as u64, Ordering::Relaxed);

        // Base64编码数据
        #[allow(deprecated)]
        let data_base64 = base64::encode(&buffer[..bytes_read]);

        Ok(serde_json::json!({
            "data": data_base64,
            "bytes_read": bytes_read
        }))
    }

    /// 处理写入文件块请求
    async fn handle_write_chunk(
        config: &StorageServerConfig,
        request: &StorageRequest,
        stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));
        let offset = request.offset.unwrap_or(0);

        // 解码数据
        #[allow(deprecated)]
        let data = base64::decode(request.data.as_ref().ok_or_else(|| {
            ErrorInfo::new(8030, "缺少写入数据".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error)
        })?).map_err(|e| {
            ErrorInfo::new(8031, format!("解码数据失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
        })?;

        debug!("写入文件块: {} (偏移: {}, 大小: {})", full_path.display(), offset, data.len());

        // 检查文件大小限制
        if data.len() as u64 > config.max_file_size {
            return Err(ErrorInfo::new(8032, "文件大小超过限制".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                ErrorInfo::new(8033, format!("创建父目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;
        }

        // 打开文件
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&full_path)
            .await
            .map_err(|e| {
                ErrorInfo::new(8034, format!("打开文件失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;

        // 定位到指定偏移
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| {
            ErrorInfo::new(8035, format!("文件定位失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 写入数据
        file.write_all(&data).await.map_err(|e| {
            ErrorInfo::new(8036, format!("写入文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 同步到磁盘
        file.sync_all().await.map_err(|e| {
            ErrorInfo::new(8037, format!("同步文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 更新统计信息
        stats.total_bytes_written.fetch_add(data.len() as u64, Ordering::Relaxed);

        Ok(serde_json::json!({
            "bytes_written": data.len()
        }))
    }

    /// 处理获取文件信息请求
    async fn handle_get_file_info(
        config: &StorageServerConfig,
        request: &StorageRequest,
        _stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));

        debug!("获取文件信息: {}", full_path.display());

        let metadata = fs::metadata(&full_path).await.map_err(|e| {
            ErrorInfo::new(8040, format!("获取文件元数据失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        let file_info = ServerFileInfo {
            path: request.path.clone(),
            size: metadata.len(),
            modified: metadata.modified()
                .unwrap_or(UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            is_dir: metadata.is_dir(),
            permissions: Some(metadata.permissions().mode()),
            hash: None, // 可以后续实现哈希计算
        };

        serde_json::to_value(file_info).map_err(|e| ErrorInfo::new(5801, format!("序列化文件信息失败: {}", e))
            .with_category(ErrorCategory::Parse)
            .with_severity(ErrorSeverity::Error))
    }

    /// 处理创建目录请求
    async fn handle_create_dir(
        config: &StorageServerConfig,
        request: &StorageRequest,
        _stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));

        debug!("创建目录: {}", full_path.display());

        fs::create_dir_all(&full_path).await.map_err(|e| {
            ErrorInfo::new(8050, format!("创建目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        Ok(serde_json::json!({
            "created": true
        }))
    }

    /// 处理删除文件请求
    async fn handle_delete_file(
        config: &StorageServerConfig,
        request: &StorageRequest,
        _stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));

        debug!("删除文件: {}", full_path.display());

        fs::remove_file(&full_path).await.map_err(|e| {
            ErrorInfo::new(8060, format!("删除文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        Ok(serde_json::json!({
            "deleted": true
        }))
    }

    /// 处理检查文件是否存在请求
    async fn handle_file_exists(
        config: &StorageServerConfig,
        request: &StorageRequest,
        _stats: &StorageServerStats,
    ) -> TransferResult<serde_json::Value> {
        let path = Path::new(&request.path);
        let full_path = config.base_path.join(path.strip_prefix("/").unwrap_or(path));

        debug!("检查文件是否存在: {}", full_path.display());

        let exists = fs::metadata(&full_path).await.is_ok();

        Ok(serde_json::json!({
            "exists": exists
        }))
    }
}

/// 服务器统计信息快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageServerStatsSnapshot {
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数
    pub successful_requests: u64,
    /// 失败请求数
    pub failed_requests: u64,
    /// 总读取字节数
    pub total_bytes_read: u64,
    /// 总写入字节数
    pub total_bytes_written: u64,
    /// 当前连接数
    pub active_connections: u64,
}

/// 为了支持从任何连接接收消息，我们需要扩展SecureTransport
trait SecureTransportExt {
    async fn receive_message_from_any(&self) -> TransferResult<(quinn::Connection, TransportMessage)>;
}

impl SecureTransportExt for SecureTransport {
    async fn receive_message_from_any(&self) -> TransferResult<(quinn::Connection, TransportMessage)> {
        // 这是一个简化的实现
        // 在实际应用中，需要更复杂的连接管理
        use std::time::Duration;

        // 等待连接和消息
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 这里应该实现真正的消息接收逻辑
        // 暂时返回错误，表示需要实现
        Err(ErrorInfo::new(8099, "receive_message_from_any 需要实现".to_string())
            .with_category(ErrorCategory::NotImplemented)
            .with_severity(ErrorSeverity::Error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageServerConfig {
            base_path: temp_dir.path().to_path_buf(),
            enable_auth: false,
            ..Default::default()
        };

        let server = BeyStorageServer::new(config).await;
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_storage_request_serialization() {
        let request = StorageRequest {
            request_type: StorageRequestType::ReadChunk,
            path: "/test/file.txt".to_string(),
            offset: Some(0),
            size: Some(1024),
            data: None,
            auth_token: "test-token".to_string(),
            request_id: "req-123".to_string(),
        };

        let json = serde_json::to_value(&request).unwrap();
        let deserialized: StorageRequest = serde_json::from_value(json).unwrap();

        assert_eq!(deserialized.path, "/test/file.txt");
        assert_eq!(deserialized.offset, Some(0));
        assert_eq!(deserialized.size, Some(1024));
    }

    #[tokio::test]
    async fn test_storage_response_serialization() {
        let response = StorageResponse {
            success: true,
            error: None,
            data: Some(serde_json::json!({"test": "data"})),
            request_id: "req-123".to_string(),
        };

        let json = serde_json::to_value(&response).unwrap();
        let deserialized: StorageResponse = serde_json::from_value(json).unwrap();

        assert!(deserialized.success);
        assert_eq!(deserialized.request_id, "req-123");
        assert!(deserialized.error.is_none());
    }
}