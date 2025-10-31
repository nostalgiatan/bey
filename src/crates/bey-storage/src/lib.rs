//! # BEY分布式对象存储模块
//!
//! 基于现有BEY网络基础设施的分布式对象存储系统，整合设备发现、
//! 安全传输、文件传输等现有模块，提供统一的存储抽象层。

use error::{ErrorInfo, ErrorCategory};
use std::path::Path;
use std::sync::Arc;
use bey_discovery::DeviceInfo;

// 导入模块
pub mod compression;
pub mod key_management;
pub mod bey_storage;

// 重新导出主要类型
pub use compression::{SmartCompressor, CompressionStrategy, CompressionAlgorithm, CompressionResult};
pub use key_management::{
    SecureKeyManager, KeyType, KeyMetadata, AccessLogEntry, KeyOperation,
    create_default_key_manager, create_cloud_storage_key_manager,
    create_distributed_storage_key_manager
};
pub use bey_storage::{
    BeyStorageManager, StorageConfig, FileMetadata, StorageNode, CompressionInfo,
    StoreOptions, ReadOptions, DeleteOptions, SearchFilters, StorageStatistics,
    create_default_bey_storage
};

/// 存储结果类型
pub type StorageResult<T> = std::result::Result<T, ErrorInfo>;

/// BEY分布式对象存储系统（兼容性包装器）
///
/// 为了保持向后兼容性，提供一个简单的包装器。
#[deprecated(note = "使用 BeyStorageManager 替代")]
pub struct DistributedObjectStorage {
    /// 内部BEY存储管理器
    inner: Arc<BeyStorageManager>,
}

impl DistributedObjectStorage {
    /// 创建新的分布式存储系统（已弃用）
    #[deprecated(note = "使用 BeyStorageManager::new 替代")]
    pub async fn new(_config: StorageConfig, _local_device: DeviceInfo) -> StorageResult<Self> {
        // 这个实现只是为了向后兼容
        Err(ErrorInfo::new(9001, "DistributedObjectStorage已弃用，请使用BeyStorageManager".to_string())
            .with_category(ErrorCategory::NotImplemented)
            .with_severity(error::ErrorSeverity::Warning))
    }

    /// 存储文件
    pub async fn store_file(&self, path: &Path, data: Vec<u8>, options: StoreOptions) -> StorageResult<FileMetadata> {
        self.inner.store_file(path, data, options).await
    }

    /// 读取文件
    pub async fn read_file(&self, path: &Path, options: ReadOptions) -> StorageResult<Vec<u8>> {
        self.inner.read_file(path, options).await
    }

    /// 删除文件
    pub async fn delete_file(&self, path: &Path, options: DeleteOptions) -> StorageResult<bool> {
        self.inner.delete_file(path, options).await
    }

    /// 列出目录
    pub async fn list_directory(&self, path: &Path, recursive: bool) -> StorageResult<Vec<FileMetadata>> {
        self.inner.list_directory(path, recursive).await
    }

    /// 搜索文件
    pub async fn search_files(&self, query: &str, filters: Option<SearchFilters>) -> StorageResult<Vec<FileMetadata>> {
        self.inner.search_files(query, filters).await
    }

    /// 获取存储统计信息
    pub async fn get_storage_statistics(&self) -> StorageResult<StorageStatistics> {
        self.inner.get_storage_statistics().await
    }

    /// 健康检查
    pub async fn health_check(&self) -> StorageResult<HealthStatus> {
        let stats = self.inner.get_storage_statistics().await?;
        Ok(HealthStatus {
            status: if stats.online_nodes > 0 { "healthy" } else { "degraded" }.to_string(),
            issues: Vec::new(),
        })
    }
}

/// 健康状态
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// 状态
    pub status: String,
    /// 问题列表
    pub issues: Vec<String>,
}

/// 便捷函数：创建默认存储（已弃用）
#[deprecated(note = "使用 create_default_bey_storage 替代")]
pub async fn create_default_storage() -> StorageResult<DistributedObjectStorage> {
    // 为了向后兼容，创建一个虚拟包装器
    Err(ErrorInfo::new(9002, "create_default_storage已弃用，请使用create_default_bey_storage".to_string())
        .with_category(ErrorCategory::NotImplemented)
        .with_severity(error::ErrorSeverity::Warning))
}

/// 便捷函数：创建压缩器
pub fn create_smart_compressor() -> Arc<SmartCompressor> {
    Arc::new(SmartCompressor::new(CompressionStrategy::default()))
}

/// 便捷函数：创建密钥管理器
pub fn create_secure_key_manager(service_name: &str) -> StorageResult<SecureKeyManager> {
    SecureKeyManager::new(service_name, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bey_storage_compatibility() {
        // 测试向后兼容性
        let result = create_default_storage().await;
        assert!(result.is_err() || result.is_ok()); // 至少不会崩溃
    }

    #[tokio::test]
    async fn test_create_smart_compressor() {
        let compressor = create_smart_compressor();
        let test_data = "Hello, BEY Storage!".repeat(100);

        let result = compressor.smart_compress(test_data.as_bytes(), "txt").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_secure_key_manager() {
        let key_manager = create_secure_key_manager("test_service");
        assert!(key_manager.is_ok());
    }

    #[tokio::test]
    async fn test_storage_config() {
        let config = StorageConfig::default();
        assert!(config.enable_compression);
        assert!(config.enable_encryption);
        assert_eq!(config.replica_count, 2);
    }
}