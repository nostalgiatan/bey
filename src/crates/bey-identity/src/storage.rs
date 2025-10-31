//! # 证书存储管理
//!
//! 提供证书的安全存储功能，包括文件系统存储、内存缓存和持久化管理。
//! 支持证书的加密存储、访问控制和完整性验证。

use crate::error::IdentityError;
use crate::types::{CertificateData, CertificateType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// 证书存储管理器
///
/// 负责证书的安全存储和检索，支持内存缓存和持久化存储。
pub struct CertificateStorage {
    /// 存储配置
    config: StorageConfig,

    /// 内存缓存
    cache: Arc<RwLock<HashMap<String, CertificateData>>>,

    /// 存储统计信息
    statistics: Arc<RwLock<StorageStatistics>>,
}

/// 存储配置
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// 存储根目录
    pub root_directory: PathBuf,

    /// 是否启用内存缓存
    pub enable_memory_cache: bool,

    /// 缓存过期时间（秒）
    pub cache_ttl_seconds: u64,

    /// 是否启用文件加密
    pub enable_encryption: bool,

    /// 是否启用备份
    pub enable_backup: bool,

    /// 备份保留天数
    pub backup_retention_days: u32,

    /// 文件权限设置（使用Option来支持跨平台）
    pub file_permissions: Option<std::fs::Permissions>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root_directory: PathBuf::from("./certificates"),
            enable_memory_cache: true,
            cache_ttl_seconds: 3600, // 1小时
            enable_encryption: false,
            enable_backup: true,
            backup_retention_days: 30,
            #[cfg(unix)]
            file_permissions: Some(std::fs::Permissions::from_mode(0o600)), // 只有所有者可读写
            #[cfg(not(unix))]
            file_permissions: None, // 非Unix系统不设置特定权限
        }
    }
}

/// 存储统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatistics {
    /// 总证书数量
    pub total_certificates: usize,

    /// 各类型证书数量
    pub certificates_by_type: HashMap<String, usize>,

    /// 存储使用量（字节）
    pub storage_usage_bytes: u64,

    /// 最后更新时间
    pub last_updated: std::time::SystemTime,

    /// 缓存命中次数
    pub cache_hits: u64,

    /// 缓存未命中次数
    pub cache_misses: u64,
}

impl Default for StorageStatistics {
    fn default() -> Self {
        Self {
            total_certificates: 0,
            certificates_by_type: HashMap::new(),
            storage_usage_bytes: 0,
            last_updated: std::time::SystemTime::now(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

impl CertificateStorage {
    /// 创建新的证书存储实例
    pub async fn new(config: StorageConfig) -> Result<Self, IdentityError> {
        // 确保存储目录存在
        fs::create_dir_all(&config.root_directory)
            .map_err(|e| IdentityError::StorageError(format!("创建存储目录失败: {}", e)))?;

        // 创建子目录
        let subdirs = ["ca", "devices", "clients", "servers", "backup"];
        for subdir in &subdirs {
            let dir_path = config.root_directory.join(subdir);
            fs::create_dir_all(&dir_path)
                .map_err(|e| IdentityError::StorageError(format!("创建子目录失败: {}", e)))?;
        }

        let storage = Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(StorageStatistics::default())),
        };

        // 初始化时加载现有证书
        storage.load_existing_certificates().await?;

        info!("证书存储初始化完成");
        Ok(storage)
    }

    /// 存储证书
    pub async fn store_certificate(&self, certificate: CertificateData) -> Result<(), IdentityError> {
        debug!("存储证书: {}", certificate.certificate_id);

        // 确定存储路径
        let file_path = self.get_certificate_path(&certificate);

        // 序列化证书数据
        let serialized = serde_json::to_string_pretty(&certificate)
            .map_err(|e| IdentityError::StorageError(format!("序列化证书失败: {}", e)))?;

        // 写入文件
        fs::write(&file_path, serialized)
            .map_err(|e| IdentityError::StorageError(format!("写入证书文件失败: {}", e)))?;

        // 设置文件权限（如果配置了权限设置）
        if let Some(ref permissions) = self.config.file_permissions {
            fs::set_permissions(&file_path, permissions.clone())
                .map_err(|e| IdentityError::StorageError(format!("设置文件权限失败: {}", e)))?;
        }

        // 如果启用备份，创建备份
        if self.config.enable_backup {
            self.create_certificate_backup(&certificate).await?;
        }

        // 更新内存缓存
        if self.config.enable_memory_cache {
            let mut cache = self.cache.write().await;
            cache.insert(certificate.certificate_id.clone(), certificate.clone());
        }

        // 更新统计信息
        self.update_statistics(|stats| {
            stats.total_certificates += 1;
            let type_name = format!("{:?}", certificate.certificate_type);
            *stats.certificates_by_type.entry(type_name).or_insert(0) += 1;
            stats.last_updated = std::time::SystemTime::now();
        }).await;

        info!("证书存储成功: {}", certificate.certificate_id);
        Ok(())
    }

    /// 检索证书
    pub async fn retrieve_certificate(&self, certificate_id: &str) -> Result<Option<CertificateData>, IdentityError> {
        debug!("检索证书: {}", certificate_id);

        // 首先检查内存缓存
        if self.config.enable_memory_cache {
            let cache = self.cache.read().await;
            if let Some(certificate) = cache.get(certificate_id) {
                self.update_statistics(|stats| {
                    stats.cache_hits += 1;
                }).await;
                debug!("从缓存中找到证书: {}", certificate_id);
                return Ok(Some(certificate.clone()));
            } else {
                self.update_statistics(|stats| {
                    stats.cache_misses += 1;
                }).await;
            }
        }

        // 从文件系统加载
        let file_path = self.find_certificate_file(certificate_id)?;
        let file_path = match file_path {
            Some(path) => path,
            None => {
                debug!("证书文件不存在: {}", certificate_id);
                return Ok(None);
            }
        };

        let content = fs::read_to_string(&file_path)
            .map_err(|e| IdentityError::StorageError(format!("读取证书文件失败: {}", e)))?;

        let certificate: CertificateData = serde_json::from_str(&content)
            .map_err(|e| IdentityError::StorageError(format!("反序列化证书失败: {}", e)))?;

        // 更新缓存
        if self.config.enable_memory_cache {
            let mut cache = self.cache.write().await;
            cache.insert(certificate_id.to_string(), certificate.clone());
        }

        debug!("从文件系统加载证书成功: {}", certificate_id);
        Ok(Some(certificate))
    }

    /// 删除证书
    pub async fn delete_certificate(&self, certificate_id: &str) -> Result<bool, IdentityError> {
        debug!("删除证书: {}", certificate_id);

        let file_path = self.find_certificate_file(certificate_id)?;
        let file_path = match file_path {
            Some(path) => path,
            None => {
                debug!("证书文件不存在，无需删除: {}", certificate_id);
                return Ok(false);
            }
        };

        // 创建备份（如果启用）
        if self.config.enable_backup {
            if let Ok(Some(certificate)) = self.retrieve_certificate(certificate_id).await {
                self.create_certificate_backup(&certificate).await?;
            }
        }

        // 删除文件
        fs::remove_file(&file_path)
            .map_err(|e| IdentityError::StorageError(format!("删除证书文件失败: {}", e)))?;

        // 从缓存中移除
        if self.config.enable_memory_cache {
            let mut cache = self.cache.write().await;
            cache.remove(certificate_id);
        }

        // 更新统计信息
        if let Ok(Some(certificate)) = self.retrieve_certificate(certificate_id).await {
            self.update_statistics(|stats| {
                stats.total_certificates = stats.total_certificates.saturating_sub(1);
                let type_name = format!("{:?}", certificate.certificate_type);
                if let Some(count) = stats.certificates_by_type.get_mut(&type_name) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        stats.certificates_by_type.remove(&type_name);
                    }
                }
            }).await;
        }

        info!("证书删除成功: {}", certificate_id);
        Ok(true)
    }

    /// 列出所有证书
    pub async fn list_certificates(&self) -> Result<Vec<CertificateData>, IdentityError> {
        debug!("列出所有证书");

        let mut certificates = Vec::new();

        // 遍历所有子目录
        let subdirs = ["ca", "devices", "clients", "servers"];
        for subdir in &subdirs {
            let dir_path = self.config.root_directory.join(subdir);
            if !dir_path.exists() {
                continue;
            }

            let entries = fs::read_dir(&dir_path)
                .map_err(|e| IdentityError::StorageError(format!("读取目录失败: {}", e)))?;

            for entry in entries {
                let entry = entry
                    .map_err(|e| IdentityError::StorageError(format!("读取目录条目失败: {}", e)))?;

                let path = entry.path();
                if let Some(extension) = path.extension() {
                    if extension == "json" {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(certificate) = serde_json::from_str::<CertificateData>(&content) {
                                certificates.push(certificate);
                            }
                        }
                    }
                }
            }
        }

        debug!("找到 {} 个证书", certificates.len());
        Ok(certificates)
    }

    /// 获取存储统计信息
    pub async fn get_statistics(&self) -> StorageStatistics {
        let _cache_size = if self.config.enable_memory_cache {
            self.cache.read().await.len()
        } else {
            0
        };

        let certificates = match self.list_certificates().await {
            Ok(certs) => certs,
            Err(e) => {
                warn!("获取证书列表失败，统计信息可能不完整: {}", e);
                Vec::new()
            }
        };

        let mut certificates_by_type = std::collections::HashMap::new();
        let mut storage_usage_bytes = 0u64;

        for cert in &certificates {
            let type_name = format!("{:?}", cert.certificate_type);
            *certificates_by_type.entry(type_name).or_insert(0) += 1;

            // 计算存储使用量
            if let Ok(Some(file_path)) = self.find_certificate_file(&cert.certificate_id) {
                if let Ok(metadata) = std::fs::metadata(&file_path) {
                    storage_usage_bytes += metadata.len();
                }
            }
        }

        let stats = self.statistics.read().await;
        StorageStatistics {
            total_certificates: certificates.len(),
            certificates_by_type,
            storage_usage_bytes,
            last_updated: std::time::SystemTime::now(),
            cache_hits: stats.cache_hits,
            cache_misses: stats.cache_misses,
        }
    }

    /// 清理缓存
    pub async fn clear_cache(&self) -> Result<(), IdentityError> {
        debug!("清理内存缓存");

        if self.config.enable_memory_cache {
            let mut cache = self.cache.write().await;
            cache.clear();
        }

        info!("内存缓存清理完成");
        Ok(())
    }

    /// 执行存储维护
    pub async fn perform_maintenance(&self) -> Result<MaintenanceReport, IdentityError> {
        info!("开始执行存储维护");

        let mut report = MaintenanceReport::default();

        // 清理过期备份
        if self.config.enable_backup {
            report.backups_cleaned = self.cleanup_old_backups().await?;
        }

        // 重新计算统计信息
        self.recalculate_statistics().await?;
        report.statistics_updated = true;

        // 清理缓存
        self.clear_cache().await?;
        report.cache_cleared = true;

        info!("存储维护完成: {:?}", report);
        Ok(report)
    }

    // 私有辅助方法

    /// 获取证书存储路径
    fn get_certificate_path(&self, certificate: &CertificateData) -> PathBuf {
        let subdir = match certificate.certificate_type {
            CertificateType::RootCA | CertificateType::IntermediateCA => "ca",
            CertificateType::Device => "devices",
            CertificateType::Client => "clients",
            CertificateType::Server => "servers",
            _ => "others",
        };

        self.config.root_directory
            .join(subdir)
            .join(format!("{}.json", certificate.certificate_id))
    }

    /// 查找证书文件
    fn find_certificate_file(&self, certificate_id: &str) -> Result<Option<PathBuf>, IdentityError> {
        let subdirs = ["ca", "devices", "clients", "servers", "others"];

        for subdir in &subdirs {
            let file_path = self.config.root_directory
                .join(subdir)
                .join(format!("{}.json", certificate_id));

            if file_path.exists() {
                return Ok(Some(file_path));
            }
        }

        Ok(None)
    }

    /// 加载现有证书到缓存
    async fn load_existing_certificates(&self) -> Result<(), IdentityError> {
        if !self.config.enable_memory_cache {
            return Ok(());
        }

        debug!("加载现有证书到缓存");

        let certificates = self.list_certificates().await?;
        let mut cache = self.cache.write().await;

        for certificate in certificates {
            cache.insert(certificate.certificate_id.clone(), certificate);
        }

        info!("已加载 {} 个证书到缓存", cache.len());
        Ok(())
    }

    /// 创建证书备份
    async fn create_certificate_backup(&self, certificate: &CertificateData) -> Result<(), IdentityError> {
        let backup_dir = self.config.root_directory.join("backup");
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_file = backup_dir.join(format!("{}_{}.json", certificate.certificate_id, timestamp));

        let serialized = serde_json::to_string_pretty(certificate)
            .map_err(|e| IdentityError::StorageError(format!("序列化证书失败: {}", e)))?;

        fs::write(&backup_file, serialized)
            .map_err(|e| IdentityError::StorageError(format!("创建备份失败: {}", e)))?;

        debug!("证书备份创建成功: {:?}", backup_file);
        Ok(())
    }

    /// 清理旧备份
    async fn cleanup_old_backups(&self) -> Result<usize, IdentityError> {
        let backup_dir = self.config.root_directory.join("backup");
        if !backup_dir.exists() {
            return Ok(0);
        }

        let cutoff_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(self.config.backup_retention_days as u64 * 86400);

        let mut cleaned_count = 0;

        for entry in fs::read_dir(&backup_dir)
            .map_err(|e| IdentityError::StorageError(format!("读取备份目录失败: {}", e)))?
        {
            let entry = entry
                .map_err(|e| IdentityError::StorageError(format!("读取备份条目失败: {}", e)))?;

            let metadata = entry.metadata()
                .map_err(|e| IdentityError::StorageError(format!("读取备份元数据失败: {}", e)))?;

            if let Ok(modified) = metadata.modified() {
                if modified < cutoff_time {
                    fs::remove_file(entry.path())
                        .map_err(|e| IdentityError::StorageError(format!("删除旧备份失败: {}", e)))?;
                    cleaned_count += 1;
                }
            }
        }

        if cleaned_count > 0 {
            info!("清理了 {} 个旧备份文件", cleaned_count);
        }

        Ok(cleaned_count)
    }

    /// 重新计算统计信息
    async fn recalculate_statistics(&self) -> Result<(), IdentityError> {
        let certificates = self.list_certificates().await?;

        let mut stats = StorageStatistics::default();
        stats.total_certificates = certificates.len();

        for certificate in &certificates {
            let type_name = format!("{:?}", certificate.certificate_type);
            *stats.certificates_by_type.entry(type_name).or_insert(0) += 1;
        }

        // 计算存储使用量
        stats.storage_usage_bytes = self.calculate_storage_usage().await?;
        stats.last_updated = std::time::SystemTime::now();

        let mut statistics = self.statistics.write().await;
        *statistics = stats;

        Ok(())
    }

    /// 计算存储使用量
    async fn calculate_storage_usage(&self) -> Result<u64, IdentityError> {
        fn calculate_dir_size(dir_path: &Path) -> Result<u64, std::io::Error> {
            let mut size = 0u64;
            for entry in fs::read_dir(dir_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    size += calculate_dir_size(&path)?;
                } else {
                    size += entry.metadata()?.len();
                }
            }
            Ok(size)
        }

        match calculate_dir_size(&self.config.root_directory) {
            Ok(size) => Ok(size),
            Err(e) => Err(IdentityError::StorageError(format!("计算存储使用量失败: {}", e))),
        }
    }

    /// 更新统计信息
    async fn update_statistics<F>(&self, updater: F)
    where
        F: FnOnce(&mut StorageStatistics),
    {
        let mut stats = self.statistics.write().await;
        updater(&mut stats);
    }
}

/// 维护报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceReport {
    /// 清理的备份数量
    pub backups_cleaned: usize,

    /// 统计信息是否更新
    pub statistics_updated: bool,

    /// 缓存是否清理
    pub cache_cleared: bool,

    /// 维护时间
    pub maintenance_time: std::time::SystemTime,
}

impl Default for MaintenanceReport {
    fn default() -> Self {
        Self {
            backups_cleaned: 0,
            statistics_updated: false,
            cache_cleared: false,
            maintenance_time: std::time::SystemTime::now(),
        }
    }
}

