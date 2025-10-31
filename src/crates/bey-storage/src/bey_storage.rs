//! # BEY分布式存储系统
//!
//! 基于现有BEY网络基础设施的分布式对象存储系统，整合设备发现、
//! 安全传输、文件传输等现有模块，提供统一的存储抽象层。
//!
//! ## 架构设计
//!
//! ```
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    BEY分布式存储系统                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │ BeyStorageManager (统一存储管理器)                                │
//! │  ├─ DeviceAwareStorage (设备感知存储)                              │
//! │  ├─ DistributedFileCache (分布式文件缓存)                         │
//! │  └─ StoragePolicyEngine (存储策略引擎)                            │
//! │                                                                 │
//! │ 整合现有BEY模块:                                                   │
//! │  ├─ bey_discovery (设备发现)                                     │
//! │  ├─ bey_transport (安全传输)                                     │
//! │  ├─ bey_file_transfer (文件传输)                                 │
//! │  ├─ bey_identity (身份验证)                                      │
//! │  └─ bey_permissions (权限管理)                                    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

// 导入BEY现有模块
use bey_discovery::{DiscoveryService, DiscoveryConfig, DeviceEvent, DeviceInfo};
use bey_file_transfer::{TransferManager, TransferConfig};
use bey_identity::{CertificateManager, CertificateConfig};
use bey_permissions::PermissionManager;

// 导入现有压缩功能
use crate::compression::{SmartCompressor, CompressionStrategy};
use crate::key_management::SecureKeyManager;

/// 存储结果类型
pub type StorageResult<T> = std::result::Result<T, ErrorInfo>;

/// 文件元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// 文件唯一标识符
    pub file_id: String,
    /// 文件名
    pub filename: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 文件哈希（用于完整性校验）
    pub hash: String,
    /// 创建时间
    pub created_at: SystemTime,
    /// 修改时间
    pub modified_at: SystemTime,
    /// MIME类型
    pub mime_type: Option<String>,
    /// 自定义标签
    pub tags: Vec<String>,
    /// 存储节点列表
    pub storage_nodes: Vec<StorageNode>,
    /// 压缩信息
    pub compression_info: Option<CompressionInfo>,
    /// 虚拟路径（用于映射到存储路径）
    pub virtual_path: Option<String>,
}

impl FileMetadata {
    /// 获取虚拟路径
    pub fn virtual_path(&self) -> PathBuf {
        match &self.virtual_path {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(&format!("/{}", self.filename)),
        }
    }

    /// 检查文件是否过期
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.modified_at.elapsed().unwrap_or_default() > ttl
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> Option<&str> {
        Path::new(&self.filename)
            .extension()
            .and_then(|ext| ext.to_str())
    }

    /// 更新修改时间
    pub fn update_modified_time(&mut self) {
        self.modified_at = SystemTime::now();
    }

    /// 添加标签
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// 移除标签
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }

    /// 检查是否有指定标签
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// 计算文件年龄
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }
}

/// 压缩信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionInfo {
    /// 压缩算法
    pub algorithm: String,
    /// 原始大小
    pub original_size: u64,
    /// 压缩后大小
    pub compressed_size: u64,
    /// 压缩率
    pub compression_ratio: f64,
    /// 压缩耗时（毫秒）
    pub compression_time_ms: u64,
}

/// 存储节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageNode {
    /// 节点ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 网络地址
    pub address: String,
    /// 可用存储空间（字节）
    pub available_space: u64,
    /// 是否在线
    pub online: bool,
    /// 最后心跳时间
    pub last_heartbeat: SystemTime,
    /// 节点权重（用于负载均衡）
    pub weight: f64,
}

/// 存储配置
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// 存储根目录
    pub storage_root: PathBuf,
    /// 是否启用压缩
    pub enable_compression: bool,
    /// 压缩策略
    pub compression_strategy: CompressionStrategy,
    /// 副本数量
    pub replica_count: u32,
    /// 是否启用加密
    pub enable_encryption: bool,
    /// 缓存大小限制（字节）
    pub cache_size_limit: u64,
    /// 自动清理间隔
    pub cleanup_interval: Duration,
    /// 设备发现配置
    pub discovery_config: DiscoveryConfig,
    /// 传输配置
    pub transfer_config: TransferConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_root: PathBuf::from("./bey_storage"),
            enable_compression: true,
            compression_strategy: CompressionStrategy::default(),
            replica_count: 2,
            enable_encryption: true,
            cache_size_limit: 1024 * 1024 * 1024, // 1GB
            cleanup_interval: Duration::from_secs(3600), // 1小时
            discovery_config: DiscoveryConfig::default(),
            transfer_config: TransferConfig::default(),
        }
    }
}

/// BEY分布式存储管理器
pub struct BeyStorageManager {
    /// 配置信息
    config: StorageConfig,
    /// 本地设备信息
    local_device: DeviceInfo,
    /// 设备发现服务
    discovery_service: Arc<RwLock<DiscoveryService>>,
    /// 文件传输管理器
    transfer_manager: Arc<TransferManager>,
    /// 证书管理器
    certificate_manager: Arc<CertificateManager>,
    /// 权限管理器
    permission_manager: Arc<PermissionManager>,
    /// 密钥管理器
    key_manager: Arc<SecureKeyManager>,
    /// 智能压缩器
    compressor: Arc<SmartCompressor>,
    /// 存储节点列表
    storage_nodes: Arc<RwLock<HashMap<String, StorageNode>>>,
    /// 文件元数据缓存
    file_metadata: Arc<RwLock<HashMap<String, FileMetadata>>>,
    /// 本地存储路径映射
    local_storage_path: PathBuf,
}

impl BeyStorageManager {
    /// 创建新的存储管理器
    pub async fn new(
        config: StorageConfig,
        local_device: DeviceInfo,
    ) -> StorageResult<Self> {
        info!("初始化BEY分布式存储管理器");

        // 创建存储根目录
        std::fs::create_dir_all(&config.storage_root)
            .map_err(|e| ErrorInfo::new(5001, format!("创建存储目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem))?;

        // 初始化设备发现服务
        let discovery_service = DiscoveryService::new(config.discovery_config.clone(), local_device.clone())
            .await
            .map_err(|e| ErrorInfo::new(5002, format!("创建发现服务失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        // 初始化文件传输管理器
        let transfer_manager = TransferManager::new(config.transfer_config.clone())
            .await
            .map_err(|e| ErrorInfo::new(5003, format!("创建传输管理器失败: {}", e))
                .with_category(ErrorCategory::System))?;

        // 初始化证书管理器
        let certificate_manager = CertificateManager::initialize(CertificateConfig::default()).await
            .map_err(|e| ErrorInfo::new(5004, format!("创建证书管理器失败: {}", e))
                .with_category(ErrorCategory::Authentication))?;

        // 初始化权限管理器
        let permission_manager = PermissionManager::new().await
            .map_err(|e| ErrorInfo::new(5005, format!("创建权限管理器失败: {}", e))
                .with_category(ErrorCategory::Authorization))?;

        // 初始化密钥管理器
        let key_manager = SecureKeyManager::new("bey_storage", true)
            .map_err(|e| ErrorInfo::new(5006, format!("创建密钥管理器失败: {}", e))
                .with_category(ErrorCategory::Authentication))?;

        // 初始化智能压缩器
        let compressor = Arc::new(SmartCompressor::new(config.compression_strategy.clone()));

        // 创建本地存储路径
        let local_storage_path = config.storage_root.join(format!("device_{}", local_device.device_id));
        std::fs::create_dir_all(&local_storage_path)
            .map_err(|e| ErrorInfo::new(5007, format!("创建本地存储路径失败: {}", e))
                .with_category(ErrorCategory::FileSystem))?;

        let storage_manager = Self {
            config,
            local_device,
            discovery_service: Arc::new(RwLock::new(discovery_service)),
            transfer_manager: Arc::new(transfer_manager),
            certificate_manager: Arc::new(certificate_manager),
            permission_manager: Arc::new(permission_manager),
            key_manager: Arc::new(key_manager),
            compressor,
            storage_nodes: Arc::new(RwLock::new(HashMap::new())),
            file_metadata: Arc::new(RwLock::new(HashMap::new())),
            local_storage_path,
        };

        // 启动后台服务
        storage_manager.start_background_services().await?;

        Ok(storage_manager)
    }

    /// 启动后台服务
    async fn start_background_services(&self) -> StorageResult<()> {
        info!("启动存储后台服务");

        // 启动设备发现服务
        {
            let mut discovery = self.discovery_service.write().await;
            discovery.start().await
                .map_err(|e| ErrorInfo::new(5008, format!("启动发现服务失败: {}", e))
                    .with_category(ErrorCategory::Network))?;
        }

        // 启动设备监听任务
        self.start_device_listener().await;

        // 启动存储节点管理任务
        self.start_storage_node_manager().await;

        // 启动清理任务
        self.start_cleanup_task().await;

        info!("存储后台服务启动完成");
        Ok(())
    }

    /// 存储文件
    pub async fn store_file(
        &self,
        virtual_path: &Path,
        data: Vec<u8>,
        options: StoreOptions,
    ) -> StorageResult<FileMetadata> {
        info!("存储文件: {:?}", virtual_path);

        // 生成文件ID
        let file_id = self.generate_file_id().await;

        // 检查权限
        if !self.check_storage_permission(virtual_path, &options).await? {
            return Err(ErrorInfo::new(5009, "存储权限不足".to_string())
                .with_category(ErrorCategory::Authorization));
        }

        // 压缩数据（如果启用）
        let (processed_data, compression_info) = if self.config.enable_compression {
            self.compress_data(&data).await?
        } else {
            (data, None)
        };

        // 加密数据（如果启用）
        let final_data = if self.config.enable_encryption {
            self.encrypt_data(&processed_data).await?
        } else {
            processed_data
        };

        // 计算文件哈希
        let hash = self.calculate_hash(&final_data).await;

        // 创建文件元数据
        let now = SystemTime::now();
        let metadata = FileMetadata {
            file_id: file_id.clone(),
            filename: virtual_path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string(),
            size: final_data.len() as u64,
            hash,
            created_at: now,
            modified_at: now,
            mime_type: self.detect_mime_type(virtual_path),
            tags: options.tags.clone(),
            storage_nodes: Vec::new(), // 将在存储过程中填充
            compression_info,
            virtual_path: Some(virtual_path.to_string_lossy().to_string()),
        };

        // 存储到本地
        self.store_locally(&file_id, &final_data, &metadata).await?;

        // 创建副本（如果启用）
        if self.config.replica_count > 1 {
            self.create_replicas(&file_id, &final_data, &metadata).await?;
        }

        // 缓存元数据
        {
            let mut metadata_cache = self.file_metadata.write().await;
            metadata_cache.insert(file_id.clone(), metadata.clone());
        }

        info!("文件存储完成: {} ({} bytes)", file_id, final_data.len());
        Ok(metadata)
    }

    /// 读取文件
    pub async fn read_file(
        &self,
        virtual_path: &Path,
        options: ReadOptions,
    ) -> StorageResult<Vec<u8>> {
        debug!("读取文件: {:?}", virtual_path);

        // 检查权限
        if !self.check_read_permission(virtual_path, &options).await? {
            return Err(ErrorInfo::new(5010, "读取权限不足".to_string())
                .with_category(ErrorCategory::Authorization));
        }

        // 从缓存查找文件
        let file_id = self.find_file_id(virtual_path).await?;
        let metadata = {
            let metadata_cache = self.file_metadata.read().await;
            metadata_cache.get(&file_id).cloned()
                .ok_or_else(|| ErrorInfo::new(5011, "文件元数据不存在".to_string())
                    .with_category(ErrorCategory::FileSystem))?
        };

        // 尝试从本地读取
        if let Ok(data) = self.read_locally(&file_id).await {
            return self.process_data_for_reading(data, &metadata).await;
        }

        // 从远程节点读取
        for storage_node in &metadata.storage_nodes {
            if let Ok(data) = self.read_from_remote_node(&file_id, storage_node).await {
                return self.process_data_for_reading(data, &metadata).await;
            }
        }

        Err(ErrorInfo::new(5012, "文件不存在或无法访问".to_string())
            .with_category(ErrorCategory::FileSystem))
    }

    /// 删除文件
    pub async fn delete_file(
        &self,
        virtual_path: &Path,
        options: DeleteOptions,
    ) -> StorageResult<bool> {
        info!("删除文件: {:?}", virtual_path);

        // 检查权限
        if !self.check_delete_permission(virtual_path, &options).await? {
            return Err(ErrorInfo::new(5013, "删除权限不足".to_string())
                .with_category(ErrorCategory::Authorization));
        }

        // 查找文件
        let file_id = self.find_file_id(virtual_path).await?;
        let metadata = {
            let metadata_cache = self.file_metadata.read().await;
            metadata_cache.get(&file_id).cloned()
                .ok_or_else(|| ErrorInfo::new(5014, "文件不存在".to_string())
                    .with_category(ErrorCategory::FileSystem))?
        };

        // 从本地删除
        let mut success = self.delete_locally(&file_id).await.is_ok();

        // 从远程节点删除
        for storage_node in &metadata.storage_nodes {
            if self.delete_from_remote_node(&file_id, storage_node).await.is_ok() {
                success = true;
            }
        }

        // 删除元数据缓存
        {
            let mut metadata_cache = self.file_metadata.write().await;
            metadata_cache.remove(&file_id);
        }

        if success {
            info!("文件删除成功: {}", file_id);
        } else {
            warn!("文件删除失败: {}", file_id);
        }

        Ok(success)
    }

    /// 列出目录中的文件
    pub async fn list_directory(
        &self,
        virtual_path: &Path,
        recursive: bool,
    ) -> StorageResult<Vec<FileMetadata>> {
        debug!("列出目录: {:?} (递归: {})", virtual_path, recursive);

        // 检查权限
        if !self.check_list_permission(virtual_path).await? {
            return Err(ErrorInfo::new(5015, "列表权限不足".to_string())
                .with_category(ErrorCategory::Authorization));
        }

        let metadata_cache = self.file_metadata.read().await;
        let mut files = Vec::new();

        for metadata in metadata_cache.values() {
            // 这里应该有虚拟路径映射的逻辑
            // 暂时返回所有文件
            files.push(metadata.clone());
        }

        // 按修改时间排序
        files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        Ok(files)
    }

    /// 搜索文件
    pub async fn search_files(
        &self,
        query: &str,
        filters: Option<SearchFilters>,
    ) -> StorageResult<Vec<FileMetadata>> {
        debug!("搜索文件: {}", query);

        let metadata_cache = self.file_metadata.read().await;
        let mut results = Vec::new();

        for metadata in metadata_cache.values() {
            let mut matches = false;

            // 文件名匹配
            if metadata.filename.to_lowercase().contains(&query.to_lowercase()) {
                matches = true;
            }

            // 标签匹配
            if !matches {
                for tag in &metadata.tags {
                    if tag.to_lowercase().contains(&query.to_lowercase()) {
                        matches = true;
                        break;
                    }
                }
            }

            // 应用过滤器
            if matches {
                if let Some(ref filters) = filters {
                    if self.apply_filters(metadata, filters).await {
                        results.push(metadata.clone());
                    }
                } else {
                    results.push(metadata.clone());
                }
            }
        }

        Ok(results)
    }

    /// 获取存储统计信息
    pub async fn get_storage_statistics(&self) -> StorageResult<StorageStatistics> {
        let metadata_cache = self.file_metadata.read().await;
        let storage_nodes = self.storage_nodes.read().await;

        let total_files = metadata_cache.len();
        let total_size = metadata_cache.values()
            .map(|m| m.size)
            .sum();

        let online_nodes = storage_nodes.values()
            .filter(|n| n.online)
            .count();

        let available_space = storage_nodes.values()
            .filter(|n| n.online)
            .map(|n| n.available_space)
            .sum();

        Ok(StorageStatistics {
            total_files,
            total_size,
            online_nodes,
            available_space,
            compression_enabled: self.config.enable_compression,
            encryption_enabled: self.config.enable_encryption,
            replica_count: self.config.replica_count,
        })
    }

    // 私有辅助方法将在下面实现...

    /// 启动设备监听任务
    async fn start_device_listener(&self) {
        let discovery = Arc::clone(&self.discovery_service);
        let storage_nodes = Arc::clone(&self.storage_nodes);

        tokio::spawn(async move {
            let mut discovery_service = discovery.write().await;
            while let Some(event) = discovery_service.next_event().await {
                match event {
                    DeviceEvent::DeviceOnline(device_info) => {
                        info!("设备上线: {}", device_info.device_name);

                        // 检查设备是否支持存储功能
                        if device_info.capabilities.iter().any(|c| c == "storage_contribution") {
                            let storage_node = StorageNode {
                                device_id: device_info.device_id.clone(),
                                device_name: device_info.device_name.clone(),
                                address: device_info.address.to_string(),
                                available_space: 0, // 需要后续查询
                                online: true,
                                last_heartbeat: device_info.last_active,
                                weight: 1.0,
                            };

                            let mut nodes = storage_nodes.write().await;
                            nodes.insert(device_info.device_id.clone(), storage_node);
                        }
                    }
                    DeviceEvent::DeviceOffline(device_id) => {
                        info!("设备下线: {}", device_id);
                        let mut nodes = storage_nodes.write().await;
                        nodes.remove(&device_id);
                    }
                    DeviceEvent::DeviceUpdated(device_info) => {
                        debug!("设备更新: {}", device_info.device_name);
                        let mut nodes = storage_nodes.write().await;
                        if let Some(node) = nodes.get_mut(&device_info.device_id) {
                            node.last_heartbeat = device_info.last_active;
                            node.online = true;
                        }
                    }
                }
            }
        });
    }

    /// 启动存储节点管理任务
    async fn start_storage_node_manager(&self) {
        let storage_nodes = Arc::clone(&self.storage_nodes);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 每分钟检查一次

            loop {
                interval.tick().await;

                // 检查节点健康状态
                let mut nodes = storage_nodes.write().await;
                let now = SystemTime::now();

                for node in nodes.values_mut() {
                    if let Ok(elapsed) = now.duration_since(node.last_heartbeat) {
                        if elapsed > Duration::from_secs(120) { // 2分钟无心跳认为离线
                            node.online = false;
                        }
                    }
                }
            }
        });
    }

    /// 启动清理任务
    async fn start_cleanup_task(&self) {
        let cleanup_interval = self.config.cleanup_interval;
        let _file_metadata = Arc::clone(&self.file_metadata);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            loop {
                interval.tick().await;

                // 清理过期的临时文件
                // 这里可以实现更复杂的清理逻辑
                info!("执行存储清理任务");
            }
        });
    }

    // 其他辅助方法的实现...
}

/// 存储选项
#[derive(Debug, Clone, Default)]
pub struct StoreOptions {
    /// 是否覆盖现有文件
    pub overwrite: bool,
    /// 自定义标签
    pub tags: Vec<String>,
    /// 过期时间（可选）
    pub expires_at: Option<SystemTime>,
}

/// 读取选项
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// 指定版本（可选）
    pub version: Option<String>,
    /// 是否验证完整性
    pub verify_integrity: bool,
}

/// 删除选项
#[derive(Debug, Clone, Default)]
pub struct DeleteOptions {
    /// 是否强制删除
    pub force: bool,
    /// 是否仅删除本地副本
    pub local_only: bool,
}

/// 搜索过滤器
#[derive(Debug, Clone)]
pub struct SearchFilters {
    /// 文件类型过滤
    pub mime_types: Vec<String>,
    /// 标签过滤
    pub tags: Vec<String>,
    /// 大小范围
    pub size_range: Option<(u64, u64)>,
    /// 时间范围
    pub time_range: Option<(SystemTime, SystemTime)>,
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StorageStatistics {
    /// 总文件数
    pub total_files: usize,
    /// 总存储量
    pub total_size: u64,
    /// 在线节点数
    pub online_nodes: usize,
    /// 可用空间
    pub available_space: u64,
    /// 压缩功能是否启用
    pub compression_enabled: bool,
    /// 加密功能是否启用
    pub encryption_enabled: bool,
    /// 副本数量
    pub replica_count: u32,
}

/// 便捷函数：创建默认的BEY存储管理器
pub async fn create_default_bey_storage() -> StorageResult<BeyStorageManager> {
    let config = StorageConfig::default();
    let device_info = DeviceInfo::new(
        "default_device".to_string(),
        "Default BEY Device".to_string(),
        bey_types::DeviceType::Desktop,
        "127.0.0.1:8080".parse().unwrap(),
    );

    BeyStorageManager::new(config, device_info).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bey_storage_manager_creation() {
        let final_size = processed_data.len() as u64;

        // 创建文件元数据
        let mut metadata = FileMetadata {
            file_id: file_id.clone(),
            filename: virtual_path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown").to_string(),
            size: final_size,
            hash: self.calculate_hash(&processed_data),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            mime_type: options.mime_type.or_else(|| {
                // 根据文件扩展名推断MIME类型
                self.infer_mime_type(virtual_path)
            }),
            tags: options.tags.clone().unwrap_or_default(),
            storage_nodes: Vec::new(),
            compression_info: self.get_compression_info(&options),
            virtual_path: Some(virtual_path.to_string_lossy().to_string()),
        };

        // 在本地存储文件
        let local_path = self.get_local_file_path(&file_id);
        self.store_file_locally(&local_path, &processed_data).await?;

        // 创建副本（如果需要）
        if self.config.replica_count > 1 {
            self.create_replicas(&file_id, &processed_data, &metadata).await?;
        }

        // 添加存储节点信息
        metadata.storage_nodes.push(StorageNode {
            device_id: self.local_device.device_id.clone(),
            device_name: self.local_device.device_name.clone(),
            address: self.local_device.address.to_string(),
            available_space: self.get_available_space().await,
            online: true,
        });

        // 缓存文件元数据
        {
            let mut file_cache = self.file_metadata.write().await;
            file_cache.insert(file_id.clone(), metadata.clone());
        }

        info!("文件存储成功: {} -> {}", virtual_path.display(), file_id);
        Ok(file_id)
    }

    /// 读取文件
    pub async fn read_file(
        &self,
        virtual_path: &Path,
        options: ReadOptions,
    ) -> StorageResult<Vec<u8>> {
        info!("读取文件: {}", virtual_path.display());

        // 检查读取权限
        if !self.check_read_permission(virtual_path, &options).await? {
            return Err(ErrorInfo::new(5011, "读取权限被拒绝".to_string())
                .with_category(ErrorCategory::Permission)
                .with_severity(ErrorSeverity::Warning));
        }

        // 获取文件元数据
        let metadata = self.get_file_metadata_by_path(virtual_path).await?;
        if !metadata.storage_nodes.iter().any(|node| node.device_id == self.local_device.device_id) {
            // 文件不在本地，尝试从远程节点获取
            return self.read_file_from_remote(&metadata, options).await;
        }

        // 从本地读取
        let local_path = self.get_local_file_path(&metadata.file_id);
        let raw_data = self.read_file_locally(&local_path).await?;

        // 处理读取的数据（解密、解压缩等）
        let processed_data = self.process_data_for_reading(raw_data, &metadata).await?;

        info!("文件读取成功: {} ({} bytes)", virtual_path.display(), processed_data.len());
        Ok(processed_data)
    }

    /// 删除文件
    pub async fn delete_file(
        &self,
        virtual_path: &Path,
        options: DeleteOptions,
    ) -> StorageResult<()> {
        info!("删除文件: {}", virtual_path.display());

        // 检查删除权限
        if !self.check_delete_permission(virtual_path, &options).await? {
            return Err(ErrorInfo::new(5012, "删除权限被拒绝".to_string())
                .with_category(ErrorCategory::Permission)
                .with_severity(ErrorSeverity::Warning));
        }

        // 获取文件元数据
        let metadata = self.get_file_metadata_by_path(virtual_path).await?;

        // 从本地删除
        if metadata.storage_nodes.iter().any(|node| node.device_id == self.local_device.device_id) {
            let local_path = self.get_local_file_path(&metadata.file_id);
            self.delete_file_locally(&local_path).await?;
        }

        // 通知其他节点删除
        self.notify_remote_deletion(&metadata, options.force_remote).await?;

        // 删除元数据
        {
            let mut file_cache = self.file_metadata.write().await;
            // 查找并删除元数据
            file_cache.retain(|_, meta| meta.file_id != metadata.file_id);
        }

        info!("文件删除成功: {}", virtual_path.display());
        Ok(())
    }

    /// 列出目录中的文件
    pub async fn list_directory(
        &self,
        virtual_path: &Path,
        recursive: bool,
    ) -> StorageResult<Vec<FileMetadata>> {
        info!("列出目录: {} (递归: {})", virtual_path.display(), recursive);

        let mut result = Vec::new();

        // 获取所有文件元数据
        let file_cache = self.file_metadata.read().await;
        for metadata in file_cache.values() {
            // 检查文件是否在指定路径下
            if self.is_file_in_directory(metadata, virtual_path, recursive) {
                // 检查列表权限
                if self.check_list_permission(&metadata.virtual_path()).await.unwrap_or(true) {
                    result.push(metadata.clone());
                }
            }
        }

        // 按修改时间排序
        result.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

        info!("找到 {} 个文件", result.len());
        Ok(result)
    }

    /// 搜索文件
    pub async fn search_files(
        &self,
        query: &str,
        filters: SearchFilters,
    ) -> StorageResult<Vec<FileMetadata>> {
        info!("搜索文件: '{}'，使用过滤器: {:?}", query, filters);

        let mut result = Vec::new();
        let query_lower = query.to_lowercase();

        // 获取所有文件元数据
        let file_cache = self.file_metadata.read().await;
        for metadata in file_cache.values() {
            // 检查文件名匹配
            let name_matches = metadata.filename.to_lowercase().contains(&query_lower);

            // 检查标签匹配
            let tag_matches = metadata.tags.iter()
                .any(|tag| tag.to_lowercase().contains(&query_lower));

            // 应用过滤器
            if (name_matches || tag_matches) && self.apply_filters(metadata, &filters).await {
                // 检查搜索权限
                if self.check_search_permission(&metadata.virtual_path()).await.unwrap_or(true) {
                    result.push(metadata.clone());
                }
            }
        }

        // 按相关性排序（文件名匹配优先）
        result.sort_by(|a, b| {
            let a_name_score = if a.filename.to_lowercase().contains(&query_lower) { 2 } else { 0 };
            let b_name_score = if b.filename.to_lowercase().contains(&query_lower) { 2 } else { 0 };
            let a_tag_score = if a.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower)) { 1 } else { 0 };
            let b_tag_score = if b.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower)) { 1 } else { 0 };

            let a_score = a_name_score + a_tag_score;
            let b_score = b_name_score + b_tag_score;

            b_score.cmp(&a_score).then(b.modified_at.cmp(&a.modified_at))
        });

        info!("搜索找到 {} 个匹配文件", result.len());
        Ok(result)
    }

    /// 获取存储统计信息
    pub async fn get_storage_statistics(&self) -> StorageResult<StorageStatistics> {
        let file_cache = self.file_metadata.read().await;
        let storage_nodes = self.storage_nodes.read().await;

        let total_files = file_cache.len();
        let total_size: u64 = file_cache.values().map(|meta| meta.size).sum();
        let online_nodes = storage_nodes.values().filter(|node| node.online).count();
        let available_space = self.get_available_space().await;

        Ok(StorageStatistics {
            total_files,
            total_size,
            online_nodes,
            available_space,
            compression_enabled: self.config.enable_compression,
            encryption_enabled: self.config.enable_encryption,
            replica_count: self.config.replica_count,
        })
    }

    /// 检查文件是否存在
    pub async fn file_exists(&self, virtual_path: &Path) -> StorageResult<bool> {
        // 检查权限
        if !self.check_exists_permission(virtual_path).await.unwrap_or(true) {
            return Ok(false);
        }

        let file_cache = self.file_metadata.read().await;
        Ok(file_cache.values().any(|meta| {
            meta.virtual_path() == virtual_path
        }))
    }

    /// 获取文件元数据
    pub async fn get_file_metadata(&self, virtual_path: &Path) -> StorageResult<FileMetadata> {
        self.get_file_metadata_by_path(virtual_path).await
    }

    /// 更新文件标签
    pub async fn update_file_tags(
        &self,
        virtual_path: &Path,
        tags: Vec<String>,
    ) -> StorageResult<()> {
        // 检查更新权限
        if !self.check_update_permission(virtual_path).await? {
            return Err(ErrorInfo::new(5013, "更新权限被拒绝".to_string())
                .with_category(ErrorCategory::Permission)
                .with_severity(ErrorSeverity::Warning));
        }

        let mut file_cache = self.file_metadata.write().await;
        for metadata in file_cache.values_mut() {
            if metadata.virtual_path() == virtual_path {
                metadata.tags = tags;
                metadata.modified_at = SystemTime::now();
                break;
            }
        }

        info!("文件标签更新成功: {}", virtual_path.display());
        Ok(())
    }
    /// 生成文件ID
    async fn generate_file_id(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.local_device.device_id.hash(&mut hasher);
        std::time::SystemTime::now().hash(&mut hasher);
        fastrand::u64(..).hash(&mut hasher);

        format!("file_{:016x}", hasher.finish())
    }

    
    /// 压缩数据
    async fn compress_data(&self, data: &[u8]) -> StorageResult<(Vec<u8>, Option<CompressionInfo>)> {
        let start_time = std::time::Instant::now();

        let compression_result = self.compressor.smart_compress(data, "bin").await
            .map_err(|e| ErrorInfo::new(5020, format!("压缩失败: {}", e))
                .with_category(ErrorCategory::Compression))?;

        let compression_time_ms = start_time.elapsed().as_millis() as u64;

        if compression_result.is_beneficial {
            let compression_info = CompressionInfo {
                algorithm: format!("{:?}", compression_result.algorithm),
                original_size: data.len() as u64,
                compressed_size: compression_result.compressed_size as u64,
                compression_ratio: compression_result.compression_ratio as f64,
                compression_time_ms,
            };

            Ok((compression_result.get_compressed_data(), Some(compression_info)))
        } else {
            Ok((data.to_vec(), None))
        }
    }

    
    /// 检测MIME类型
    fn detect_mime_type(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "txt" => Some("text/plain".to_string()),
                "json" => Some("application/json".to_string()),
                "bin" => Some("application/octet-stream".to_string()),
                "jpg" | "jpeg" => Some("image/jpeg".to_string()),
                "png" => Some("image/png".to_string()),
                "pdf" => Some("application/pdf".to_string()),
                _ => None,
            })
    }

    /// 存储到本地
    async fn store_locally(&self, file_id: &str, data: &[u8], _metadata: &FileMetadata) -> StorageResult<()> {
        let file_path = self.local_storage_path.join(file_id);

        tokio::fs::write(&file_path, data).await
            .map_err(|e| ErrorInfo::new(5030, format!("本地存储失败: {}", e))
                .with_category(ErrorCategory::FileSystem))?;

        Ok(())
    }

    /// 创建副本
    async fn create_replicas(&self, _file_id: &str, _data: &[u8], _metadata: &FileMetadata) -> StorageResult<()> {
        let storage_nodes = self.storage_nodes.read().await;
        let online_nodes: Vec<_> = storage_nodes.values()
            .filter(|n| n.online)
            .take((self.config.replica_count - 1) as usize)
            .collect();

        for node in online_nodes {
            // 这里应该使用bey_file_transfer模块传输到远程节点
            debug!("创建副本到节点: {}", node.device_name);
            // TODO: 实现远程存储逻辑
        }

        Ok(())
    }

    /// 从本地读取
    async fn read_locally(&self, file_id: &str) -> StorageResult<Vec<u8>> {
        let file_path = self.local_storage_path.join(file_id);

        tokio::fs::read(&file_path).await
            .map_err(|e| ErrorInfo::new(5040, format!("本地读取失败: {}", e))
                .with_category(ErrorCategory::FileSystem))
    }

    /// 从远程节点读取
    async fn read_from_remote_node(&self, file_id: &str, node: &StorageNode) -> StorageResult<Vec<u8>> {
        // 这里应该使用bey_file_transfer模块从远程节点读取
        debug!("从远程节点读取: {} -> {}", node.device_name, file_id);
        // TODO: 实现远程读取逻辑
        Err(ErrorInfo::new(5041, "远程读取功能待实现".to_string())
            .with_category(ErrorCategory::NotImplemented))
    }

    /// 从本地删除
    async fn delete_locally(&self, file_id: &str) -> StorageResult<()> {
        let file_path = self.local_storage_path.join(file_id);

        tokio::fs::remove_file(&file_path).await
            .map_err(|e| ErrorInfo::new(5050, format!("本地删除失败: {}", e))
                .with_category(ErrorCategory::FileSystem))
    }

    /// 从远程节点删除
    async fn delete_from_remote_node(&self, file_id: &str, node: &StorageNode) -> StorageResult<()> {
        // 这里应该使用bey_file_transfer模块从远程节点删除
        debug!("从远程节点删除: {} -> {}", node.device_name, file_id);
        // TODO: 实现远程删除逻辑
        Err(ErrorInfo::new(5051, "远程删除功能待实现".to_string())
            .with_category(ErrorCategory::NotImplemented))
    }

    /// 查找文件ID
    async fn find_file_id(&self, virtual_path: &Path) -> StorageResult<String> {
        // 这里应该实现虚拟路径到文件ID的映射
        // 暂时使用简单的路径哈希
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        virtual_path.hash(&mut hasher);
        Ok(format!("file_{:016x}", hasher.finish()))
    }

    /// 处理读取的数据（解密、解压缩等）
    async fn process_data_for_reading(&self, data: Vec<u8>, metadata: &FileMetadata) -> StorageResult<Vec<u8>> {
        let mut processed_data = data;

        // 解密（如果需要）
        if self.config.enable_encryption {
            processed_data = self.decrypt_data(&processed_data).await?;
        }

        // 解压缩（如果需要）
        if let Some(ref compression_info) = metadata.compression_info {
            processed_data = self.decompress_data(&processed_data, compression_info).await?;
        }

        Ok(processed_data)
    }

    /// 解密数据
    async fn decrypt_data(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        if !self.config.enable_encryption {
            return Ok(data.to_vec());
        }

        // 使用密钥管理器进行解密
        let encryption_key_id = "default_file_encryption";

        match self.key_manager.get_key(encryption_key_id).await {
            Ok(key_data) => {
                // 使用AES-GCM进行解密
                use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
                use aes_gcm::aead::Aead;

                if key_data.len() < 32 {
                    return Err(ErrorInfo::new(7502, "加密密钥长度不足".to_string())
                        .with_category(ErrorCategory::Encryption)
                        .with_severity(ErrorSeverity::Error));
                }

                let key = Key::<Aes256Gcm>::from_slice(&key_data[..32]);
                let cipher = Aes256Gcm::new(key);

                // 从数据中提取nonce（前12字节）
                if data.len() < 12 {
                    return Err(ErrorInfo::new(7503, "加密数据格式错误".to_string())
                        .with_category(ErrorCategory::Encryption)
                        .with_severity(ErrorSeverity::Error));
                }

                let nonce = Nonce::from_slice(&data[..12]);
                let ciphertext = &data[12..];

                cipher.decrypt(nonce, ciphertext)
                    .map_err(|e| ErrorInfo::new(7504, format!("解密失败: {}", e))
                        .with_category(ErrorCategory::Encryption)
                        .with_severity(ErrorSeverity::Error))
            }
            Err(_) => {
                // 如果没有密钥，可能是未加密的数据
                warn!("未找到加密密钥，返回原始数据");
                Ok(data.to_vec())
            }
        }
    }

    /// 解压缩数据
    async fn decompress_data(&self, data: &[u8], compression_info: &CompressionInfo) -> StorageResult<Vec<u8>> {
        use crate::compression::{SmartCompressor, CompressionStrategy, CompressionAlgorithm};

        // 将字符串转换为压缩算法枚举
        let algorithm = match compression_info.algorithm.as_str() {
            "none" | "None" => CompressionAlgorithm::None,
            "lz4" | "Lz4" => CompressionAlgorithm::Lz4,
            "zstd" | "Zstd" => CompressionAlgorithm::Zstd,
            "zstd_max" | "ZstdMax" => CompressionAlgorithm::ZstdMax,
            _ => {
                // 未知算法，返回原始数据
                return Ok(data.to_vec());
            }
        };

        if algorithm == CompressionAlgorithm::None {
            return Ok(data.to_vec());
        }

        let compressor = SmartCompressor::new(CompressionStrategy::default());

        match compressor.decompress_sync(data, algorithm) {
            Ok(decompressed_data) => Ok(decompressed_data),
            Err(e) => {
                Err(ErrorInfo::new(7501, format!("解压缩失败: {}", e))
                    .with_category(ErrorCategory::Compression)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 应用搜索过滤器
    async fn apply_filters(&self, metadata: &FileMetadata, filters: &SearchFilters) -> bool {
        // MIME类型过滤
        if !filters.mime_types.is_empty() {
            if let Some(ref mime_type) = metadata.mime_type {
                if !filters.mime_types.contains(mime_type) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // 标签过滤
        if !filters.tags.is_empty() {
            if !filters.tags.iter().any(|tag| metadata.tags.contains(tag)) {
                return false;
            }
        }

        // 大小过滤
        if let Some((min_size, max_size)) = filters.size_range {
            if metadata.size < min_size || metadata.size > max_size {
                return false;
            }
        }

        // 时间过滤
        if let Some((start_time, end_time)) = filters.time_range {
            if metadata.created_at < start_time || metadata.created_at > end_time {
                return false;
            }
        }

        true
    }

    // ===== 以下为BeyStorageManager的辅助方法 =====

    /// 处理存储时的数据（压缩、加密等）
    async fn process_data_for_storing(&self, data: Vec<u8>, options: &StoreOptions) -> StorageResult<Vec<u8>> {
        let mut processed_data = data;

        // 压缩数据（如果需要）
        if self.config.enable_compression {
            processed_data = self.compress_data(&processed_data).await?;
        }

        // 加密数据（如果需要）
        if self.config.enable_encryption {
            processed_data = self.encrypt_data(&processed_data).await?;
        }

        Ok(processed_data)
    }

    /// 压缩数据
    async fn compress_data(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        // 使用智能压缩器
        let algorithm = self.compressor.select_algorithm(data.len() as u64, "application/octet-stream");
        let result = self.compressor.compress_sync(data, algorithm)
            .map_err(|e| ErrorInfo::new(7001, format!("压缩失败: {}", e))
                .with_category(ErrorCategory::Compression)
                .with_severity(ErrorSeverity::Error))?;

        Ok(result.compressed_data.unwrap_or_else(|| data.to_vec()))
    }

    /// 加密数据
    async fn encrypt_data(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        // 生成随机nonce
        let nonce_bytes = fastrand::u128(..).to_be_bytes();

        // 获取加密密钥
        let encryption_key_id = "default_file_encryption";
        let key_data = self.key_manager.get_key(encryption_key_id).await
            .map_err(|_| ErrorInfo::new(7505, "未找到加密密钥".to_string())
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error))?;

        if key_data.len() < 32 {
            return Err(ErrorInfo::new(7506, "加密密钥长度不足".to_string())
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error));
        }

        use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
        use aes_gcm::aead::{Aead, AeadCore, OsRng};

        let key = Key::<Aes256Gcm>::from_slice(&key_data[..32]);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(&nonce_bytes[..12]);

        cipher.encrypt(nonce, data)
            .map(|encrypted| {
                let mut result = nonce_bytes.to_vec();
                result.extend_from_slice(&encrypted);
                result
            })
            .map_err(|e| ErrorInfo::new(7507, format!("加密失败: {}", e))
                .with_category(ErrorCategory::Encryption)
                .with_severity(ErrorSeverity::Error))
    }

    /// 计算文件哈希
    fn calculate_hash(&self, data: &[u8]) -> String {
        use sha2::Sha256;
        use sha2::Digest;

        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// 推断MIME类型
    fn infer_mime_type(&self, path: &Path) -> Option<String> {
        let extension = path.extension()?.to_str()?;
        match extension.to_lowercase().as_str() {
            "txt" => Some("text/plain".to_string()),
            "json" => Some("application/json".to_string()),
            "xml" => Some("application/xml".to_string()),
            "pdf" => Some("application/pdf".to_string()),
            "jpg" | "jpeg" => Some("image/jpeg".to_string()),
            "png" => Some("image/png".to_string()),
            "gif" => Some("image/gif".to_string()),
            "mp3" => Some("audio/mpeg".to_string()),
            "mp4" => Some("video/mp4".to_string()),
            "zip" => Some("application/zip".to_string()),
            _ => None,
        }
    }

    /// 获取压缩信息
    fn get_compression_info(&self, options: &StoreOptions) -> Option<CompressionInfo> {
        if self.config.enable_compression {
            Some(CompressionInfo {
                algorithm: "zstd".to_string(),
                original_size: 0, // 将在存储时更新
                compressed_size: 0, // 将在存储时更新
                compression_ratio: 0.0, // 将在存储时更新
                compression_time_ms: 0, // 将在存储时更新
            })
        } else {
            None
        }
    }

    /// 获取本地文件路径
    fn get_local_file_path(&self, file_id: &str) -> PathBuf {
        self.local_storage_path.join(format!("{}.dat", file_id))
    }

    /// 在本地存储文件
    async fn store_file_locally(&self, path: &Path, data: &[u8]) -> StorageResult<()> {
        // 确保目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ErrorInfo::new(5005, format!("创建存储目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))?;
        }

        // 写入文件
        std::fs::write(path, data)
            .map_err(|e| ErrorInfo::new(5006, format!("写入文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        Ok(())
    }

    /// 从本地读取文件
    async fn read_file_locally(&self, path: &Path) -> StorageResult<Vec<u8>> {
        std::fs::read(path)
            .map_err(|e| ErrorInfo::new(5007, format!("读取文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))
    }

    /// 从本地删除文件
    async fn delete_file_locally(&self, path: &Path) -> StorageResult<()> {
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| ErrorInfo::new(5008, format!("删除文件失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))?;
        }
        Ok(())
    }

    /// 获取可用空间
    async fn get_available_space(&self) -> u64 {
        // 简单实现：返回固定值，实际应该查询磁盘空间
        10 * 1024 * 1024 * 1024 // 10GB
    }

    /// 根据路径获取文件元数据
    async fn get_file_metadata_by_path(&self, virtual_path: &Path) -> StorageResult<FileMetadata> {
        let file_cache = self.file_metadata.read().await;
        let path_str = virtual_path.to_string_lossy();

        file_cache.values()
            .find(|meta| {
                meta.virtual_path.as_ref().map_or(false, |vp| vp == &*path_str)
            })
            .cloned()
            .ok_or_else(|| ErrorInfo::new(5009, format!("文件不存在: {}", virtual_path.display()))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Warning))
    }

    /// 检查文件是否在指定目录下
    fn is_file_in_directory(&self, metadata: &FileMetadata, dir_path: &Path, recursive: bool) -> bool {
        let file_path = metadata.virtual_path();

        if recursive {
            file_path.starts_with(dir_path)
        } else {
            // 检查是否在同一目录
            file_path.parent().map_or(false, |parent| parent == dir_path)
        }
    }

    /// 从远程节点读取文件
    async fn read_file_from_remote(&self, metadata: &FileMetadata, _options: ReadOptions) -> StorageResult<Vec<u8>> {
        // 简化实现：返回错误，实际应该从其他节点获取文件
        Err(ErrorInfo::new(5010, "文件不在本地存储中".to_string())
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Warning))
    }

    /// 通知远程节点删除
    async fn notify_remote_deletion(&self, _metadata: &FileMetadata, _force_remote: bool) -> StorageResult<()> {
        // 简化实现：实际应该通知其他节点删除文件
        Ok(())
    }

    // ===== 权限检查方法（简化实现，实际应该调用权限模块） =====

    async fn check_storage_permission(&self, _virtual_path: &Path, _options: &StoreOptions) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有存储操作
    }

    async fn check_read_permission(&self, _virtual_path: &Path, _options: &ReadOptions) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有读取操作
    }

    async fn check_delete_permission(&self, _virtual_path: &Path, _options: &DeleteOptions) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有删除操作
    }

    async fn check_list_permission(&self, _virtual_path: &Path) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有列表操作
    }

    async fn check_search_permission(&self, _virtual_path: &Path) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有搜索操作
    }

    async fn check_exists_permission(&self, _virtual_path: &Path) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有存在检查操作
    }

    async fn check_update_permission(&self, _virtual_path: &Path) -> StorageResult<bool> {
        Ok(true) // 简化实现：允许所有更新操作
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bey_storage_manager_creation() {
        let device_info = DeviceInfo {
            device_id: "test_device".to_string(),
            device_name: "Test Storage Node".to_string(),
            device_type: "storage".to_string(),
            address: "127.0.0.1:8080".parse().unwrap(),
            capabilities: vec!["storage_contribution".to_string()],
            last_active: SystemTime::now(),
        };
        let config = StorageConfig::default();

        let storage_result = BeyStorageManager::new(config, device_info).await;
        assert!(storage_result.is_ok(), "BEY存储管理器创建应该成功");
    }

    #[tokio::test]
    async fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert!(config.enable_compression);
        assert!(config.enable_encryption);
        assert_eq!(config.replica_count, 2);
        assert_eq!(config.cache_size_limit, 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_file_metadata_serialization() {
        let metadata = FileMetadata {
            file_id: "test_file".to_string(),
            filename: "test.txt".to_string(),
            size: 1024,
            hash: "abc123".to_string(),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            mime_type: Some("text/plain".to_string()),
            tags: vec!["test".to_string()],
            storage_nodes: Vec::new(),
            compression_info: None,
        };

        let serialized = serde_json::to_string(&metadata).unwrap();
        let deserialized: FileMetadata = serde_json::from_str(&serialized).unwrap();

        assert_eq!(metadata.file_id, deserialized.file_id);
        assert_eq!(metadata.filename, deserialized.filename);
    }

    #[tokio::test]
    async fn test_storage_node_creation() {
        let node = StorageNode {
            device_id: "device1".to_string(),
            device_name: "Test Device".to_string(),
            address: "192.168.1.100:8080".to_string(),
            available_space: 1024 * 1024 * 1024,
            online: true,
            last_heartbeat: SystemTime::now(),
            weight: 1.0,
        };

        assert_eq!(node.device_id, "device1");
        assert!(node.online);
        assert_eq!(node.available_space, 1024 * 1024 * 1024);
    }
}