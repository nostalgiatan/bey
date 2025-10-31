//! # 安全密钥管理模块
//!
//! 提供基于系统密钥环的安全密钥存储和管理功能，支持加密密钥、证书密钥等的保护。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};
use keyring::{Entry, Error as KeyringError};
use base64::{Engine as _, engine::general_purpose};

/// 密钥类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyType {
    /// AES加密密钥
    AesEncryption,
    /// HMAC密钥
    Hmac,
    /// RSA私钥
    RsaPrivate,
    /// EC私钥
    EcPrivate,
    /// 证书密钥
    Certificate,
    /// API密钥
    ApiKey,
    /// 数据库密钥
    Database,
    /// 自定义密钥
    Custom,
}

/// 密钥元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// 密钥类型
    pub key_type: KeyType,
    /// 密钥用途描述
    pub description: String,
    /// 创建时间
    pub created_at: std::time::SystemTime,
    /// 最后访问时间
    pub last_accessed: std::time::SystemTime,
    /// 密钥版本
    pub version: u32,
    /// 是否启用
    pub enabled: bool,
    /// 过期时间（可选）
    pub expires_at: Option<std::time::SystemTime>,
    /// 自定义属性
    pub attributes: HashMap<String, String>,
}

impl KeyMetadata {
    /// 创建新的密钥元数据
    pub fn new(key_type: KeyType, description: String) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            key_type,
            description,
            created_at: now,
            last_accessed: now,
            version: 1,
            enabled: true,
            expires_at: None,
            attributes: HashMap::new(),
        }
    }

    /// 检查密钥是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            std::time::SystemTime::now() > expires_at
        } else {
            false
        }
    }

    /// 更新访问时间
    pub fn update_access_time(&mut self) {
        self.last_accessed = std::time::SystemTime::now();
    }

    /// 增加版本号
    pub fn increment_version(&mut self) {
        self.version += 1;
    }
}

/// 密钥条目
#[derive(Debug, Clone)]
struct KeyEntry {
    /// 密钥ID
    key_id: String,
    /// 密钥数据
    key_data: Vec<u8>,
    /// 密钥元数据
    metadata: KeyMetadata,
}

impl KeyEntry {
    /// 创建新的密钥条目
    fn new(key_id: String, key_data: Vec<u8>, key_type: KeyType) -> Self {
        let description = format!("Key for {}", key_id);
        Self {
            key_id,
            key_data,
            metadata: KeyMetadata::new(key_type, description),
        }
    }

    /// 获取密钥ID
    fn get_key_id(&self) -> &str {
        &self.key_id
    }

    /// 检查密钥是否过期
    fn is_expired(&self) -> bool {
        self.metadata.is_expired()
    }

    /// 更新访问时间
    fn update_access_time(&mut self) {
        self.metadata.update_access_time();
    }

    /// 验证密钥完整性
    fn verify_integrity(&self) -> bool {
        // 简单的完整性检查，实际应该使用更复杂的方法
        !self.key_data.is_empty() && !self.key_id.is_empty()
    }
}

/// 跨平台密钥存储后端
#[derive(Debug, Clone)]
pub enum KeyStorageBackend {
    /// 系统密钥环（推荐）
    SystemKeyring,
    /// 加密文件存储（回退方案）
    EncryptedFile { storage_path: PathBuf },
    /// 内存存储（临时方案）
    Memory,
}

/// 安全密钥管理器
pub struct SecureKeyManager {
    /// 服务名称前缀
    service_prefix: String,
    /// 内存中的密钥缓存
    key_cache: Arc<RwLock<HashMap<String, KeyEntry>>>,
    /// 缓存启用标志
    enable_cache: bool,
    /// 密钥访问日志
    access_log: Arc<RwLock<Vec<AccessLogEntry>>>,
    /// 存储后端
    backend: KeyStorageBackend,
    /// 文件加密密钥（用于文件后端）
    file_encryption_key: Option<Vec<u8>>,
}

/// 访问日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEntry {
    /// 密钥ID
    pub key_id: String,
    /// 操作类型
    pub operation: KeyOperation,
    /// 操作时间
    pub timestamp: std::time::SystemTime,
    /// 操作结果
    pub success: bool,
    /// 错误信息（如果有）
    pub error: Option<String>,
}

/// 密钥操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyOperation {
    /// 创建密钥
    Create,
    /// 读取密钥
    Read,
    /// 更新密钥
    Update,
    /// 删除密钥
    Delete,
    /// 列出密钥
    List,
}

impl SecureKeyManager {
    /// 创建新的密钥管理器（自动选择最佳后端）
    pub fn new(service_name: &str, enable_cache: bool) -> Result<Self, ErrorInfo> {
        info!("创建安全密钥管理器，服务: {}", service_name);

        // 尝试检测并选择最佳后端
        let backend = Self::detect_best_backend();
        info!("选择的密钥存储后端: {:?}", backend);

        Self::new_with_backend(service_name, enable_cache, backend)
    }

    /// 使用指定后端创建密钥管理器
    pub fn new_with_backend(
        service_name: &str,
        enable_cache: bool,
        backend: KeyStorageBackend,
    ) -> Result<Self, ErrorInfo> {
        info!("创建安全密钥管理器，服务: {}, 后端: {:?}", service_name, backend);

        // 生成或获取文件加密密钥（如果使用文件后端）
        let file_encryption_key = if matches!(backend, KeyStorageBackend::EncryptedFile { .. }) {
            Some(Self::generate_encryption_key()?)
        } else {
            None
        };

        Ok(Self {
            service_prefix: format!("bey_storage_{}", service_name),
            key_cache: Arc::new(RwLock::new(HashMap::new())),
            enable_cache,
            access_log: Arc::new(RwLock::new(Vec::new())),
            backend,
            file_encryption_key,
        })
    }

    /// 检测最佳密钥存储后端
    fn detect_best_backend() -> KeyStorageBackend {
        // 尝试测试系统密钥环是否可用
        match Entry::new("bey_test", "test_key") {
            Ok(entry) => {
                // 尝试一个小测试操作
                if entry.set_password("test").is_ok() {
                    // 尝试删除测试密码，如果失败也没关系
                    let _ = entry.delete_credential();
                    info!("系统密钥环可用，使用SystemKeyring后端");
                    KeyStorageBackend::SystemKeyring
                } else {
                    warn!("系统密钥环不可用，回退到文件存储");
                    KeyStorageBackend::EncryptedFile {
                        storage_path: Self::get_default_keyfile_path(),
                    }
                }
            }
            Err(e) => {
                warn!("无法访问系统密钥环: {}, 回退到文件存储", e);
                KeyStorageBackend::EncryptedFile {
                    storage_path: Self::get_default_keyfile_path(),
                }
            }
        }
    }

    /// 获取默认的密钥文件路径
    fn get_default_keyfile_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("bey");
        path.push("keys");
        std::fs::create_dir_all(&path).ok(); // 确保目录存在
        path.push("secure_keys.enc");
        path
    }

    /// 生成文件加密密钥
    fn generate_encryption_key() -> Result<Vec<u8>, ErrorInfo> {
        use rand::RngCore;

        let mut key = vec![0u8; 32]; // AES-256
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut key);

        Ok(key)
    }

    /// 获取操作系统信息
    fn get_os_info() -> &'static str {
        #[cfg(target_os = "windows")]
        return "Windows";
        #[cfg(target_os = "macos")]
        return "macOS";
        #[cfg(target_os = "linux")]
        return "Linux";
        #[cfg(target_os = "android")]
        return "Android";
        #[cfg(target_os = "ios")]
        return "iOS";
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "android", target_os = "ios")))]
        return "Unknown";
    }

    /// 创建并存储密钥
    pub async fn create_key(
        &self,
        key_id: &str,
        key_data: Vec<u8>,
        key_type: KeyType,
        description: String,
    ) -> Result<(), ErrorInfo> {
        info!("创建密钥: {} (类型: {:?})", key_id, key_type);

        let metadata = KeyMetadata::new(key_type, description);
        let entry = KeyEntry {
            key_id: key_id.to_string(),
            key_data: key_data.clone(),
            metadata: metadata.clone(),
        };

        // 存储到系统密钥环
        let service_name = format!("{}_{}", self.service_prefix, key_id);
        let entry_result = Entry::new(&service_name, key_id);

        match entry_result {
            Ok(keyring_entry) => {
                // 序列化密钥数据
                let serialized_data = serde_json::json!({
                    "data": general_purpose::STANDARD.encode(&key_data),
                    "metadata": metadata
                });

                let json_str = serde_json::to_string(&serialized_data)
                    .map_err(|e| ErrorInfo::new(1001, format!("序列化密钥数据失败: {}", e))
                        .with_category(ErrorCategory::Authentication)
                        .with_severity(ErrorSeverity::Error))?;

                // 存储到密钥环
                if let Err(e) = keyring_entry.set_password(&json_str) {
                    self.log_access(key_id, KeyOperation::Create, false, Some(format!("密钥环存储失败: {}", e))).await;
                    return Err(ErrorInfo::new(1002, format!("存储密钥到密钥环失败: {}", e))
                        .with_category(ErrorCategory::Authentication)
                        .with_severity(ErrorSeverity::Error));
                }

                // 更新缓存
                if self.enable_cache {
                    let mut cache = self.key_cache.write().await;
                    cache.insert(key_id.to_string(), entry);
                }

                self.log_access(key_id, KeyOperation::Create, true, None).await;
                info!("密钥创建成功: {}", key_id);
                Ok(())
            }
            Err(e) => {
                self.log_access(key_id, KeyOperation::Create, false, Some(format!("创建密钥环条目失败: {}", e))).await;
                Err(ErrorInfo::new(1003, format!("创建密钥环条目失败: {}", e))
                    .with_category(ErrorCategory::Authentication)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 获取密钥
    pub async fn get_key(&self, key_id: &str) -> Result<Vec<u8>, ErrorInfo> {
        debug!("获取密钥: {}", key_id);

        // 首先检查缓存
        if self.enable_cache {
            let cache = self.key_cache.read().await;
            if let Some(entry) = cache.get(key_id) {
                // 检查密钥是否过期
                if entry.metadata.is_expired() {
                    warn!("密钥已过期: {}", key_id);
                    self.log_access(key_id, KeyOperation::Read, false, Some("密钥已过期".to_string())).await;
                    return Err(ErrorInfo::new(1004, "密钥已过期".to_string())
                        .with_category(ErrorCategory::Authentication)
                        .with_severity(ErrorSeverity::Warning));
                }

                self.log_access(key_id, KeyOperation::Read, true, None).await;
                return Ok(entry.key_data.clone());
            }
        }

        // 从密钥环读取
        let service_name = format!("{}_{}", self.service_prefix, key_id);
        let entry_result = Entry::new(&service_name, key_id);

        match entry_result {
            Ok(keyring_entry) => {
                match keyring_entry.get_password() {
                    Ok(password) => {
                        // 反序列化密钥数据
                        let parsed_data: serde_json::Value = serde_json::from_str(&password)
                            .map_err(|e| ErrorInfo::new(1005, format!("解析密钥数据失败: {}", e))
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Error))?;

                        let key_data = general_purpose::STANDARD.decode(parsed_data["data"].as_str().unwrap_or(""))
                            .map_err(|e| ErrorInfo::new(1006, format!("解码密钥数据失败: {}", e))
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Error))?;

                        let metadata: KeyMetadata = serde_json::from_value(parsed_data["metadata"].clone())
                            .map_err(|e| ErrorInfo::new(1007, format!("解析密钥元数据失败: {}", e))
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Error))?;

                        // 检查密钥是否过期
                        if metadata.is_expired() {
                            self.log_access(key_id, KeyOperation::Read, false, Some("密钥已过期".to_string())).await;
                            return Err(ErrorInfo::new(1008, "密钥已过期".to_string())
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Warning));
                        }

                        // 更新缓存
                        if self.enable_cache {
                            let mut cache = self.key_cache.write().await;
                            cache.insert(key_id.to_string(), KeyEntry {
                                key_id: key_id.to_string(),
                                key_data: key_data.clone(),
                                metadata,
                            });
                        }

                        self.log_access(key_id, KeyOperation::Read, true, None).await;
                        Ok(key_data)
                    }
                    Err(KeyringError::NoEntry) => {
                        self.log_access(key_id, KeyOperation::Read, false, Some("密钥不存在".to_string())).await;
                        Err(ErrorInfo::new(1009, "密钥不存在".to_string())
                            .with_category(ErrorCategory::Authentication)
                            .with_severity(ErrorSeverity::Warning))
                    }
                    Err(e) => {
                        self.log_access(key_id, KeyOperation::Read, false, Some(format!("读取密钥失败: {}", e))).await;
                        Err(ErrorInfo::new(1010, format!("读取密钥失败: {}", e))
                            .with_category(ErrorCategory::Authentication)
                            .with_severity(ErrorSeverity::Error))
                    }
                }
            }
            Err(e) => {
                self.log_access(key_id, KeyOperation::Read, false, Some(format!("创建密钥环条目失败: {}", e))).await;
                Err(ErrorInfo::new(1011, format!("创建密钥环条目失败: {}", e))
                    .with_category(ErrorCategory::Authentication)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 获取密钥元数据
    pub async fn get_key_metadata(&self, key_id: &str) -> Result<KeyMetadata, ErrorInfo> {
        debug!("获取密钥元数据: {}", key_id);

        // 检查缓存
        if self.enable_cache {
            let cache = self.key_cache.read().await;
            if let Some(entry) = cache.get(key_id) {
                return Ok(entry.metadata.clone());
            }
        }

        // 从密钥环读取
        let service_name = format!("{}_{}", self.service_prefix, key_id);
        let entry_result = Entry::new(&service_name, key_id);

        match entry_result {
            Ok(keyring_entry) => {
                match keyring_entry.get_password() {
                    Ok(password) => {
                        let parsed_data: serde_json::Value = serde_json::from_str(&password)
                            .map_err(|e| ErrorInfo::new(1012, format!("解析密钥数据失败: {}", e))
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Error))?;

                        let metadata: KeyMetadata = serde_json::from_value(parsed_data["metadata"].clone())
                            .map_err(|e| ErrorInfo::new(1013, format!("解析密钥元数据失败: {}", e))
                                .with_category(ErrorCategory::Authentication)
                                .with_severity(ErrorSeverity::Error))?;

                        Ok(metadata)
                    }
                    Err(KeyringError::NoEntry) => {
                        Err(ErrorInfo::new(1014, "密钥不存在".to_string())
                            .with_category(ErrorCategory::Authentication)
                            .with_severity(ErrorSeverity::Warning))
                    }
                    Err(e) => {
                        Err(ErrorInfo::new(1015, format!("读取密钥失败: {}", e))
                            .with_category(ErrorCategory::Authentication)
                            .with_severity(ErrorSeverity::Error))
                    }
                }
            }
            Err(e) => {
                Err(ErrorInfo::new(1016, format!("创建密钥环条目失败: {}", e))
                    .with_category(ErrorCategory::Authentication)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 更新密钥
    pub async fn update_key(
        &self,
        key_id: &str,
        new_key_data: Vec<u8>,
        update_description: Option<String>,
    ) -> Result<(), ErrorInfo> {
        info!("更新密钥: {}", key_id);

        // 获取现有元数据
        let mut metadata = self.get_key_metadata(key_id).await?;
        metadata.update_access_time();
        metadata.increment_version();

        if let Some(description) = update_description {
            metadata.description = description;
        }

        let entry = KeyEntry {
            key_id: key_id.to_string(),
            key_data: new_key_data.clone(),
            metadata: metadata.clone(),
        };

        // 存储到系统密钥环
        let service_name = format!("{}_{}", self.service_prefix, key_id);
        let entry_result = Entry::new(&service_name, key_id);

        match entry_result {
            Ok(keyring_entry) => {
                let serialized_data = serde_json::json!({
                    "data": general_purpose::STANDARD.encode(&new_key_data),
                    "metadata": metadata
                });

                let json_str = serde_json::to_string(&serialized_data)
                    .map_err(|e| ErrorInfo::new(1017, format!("序列化密钥数据失败: {}", e))
                        .with_category(ErrorCategory::Authentication)
                        .with_severity(ErrorSeverity::Error))?;

                if let Err(e) = keyring_entry.set_password(&json_str) {
                    self.log_access(key_id, KeyOperation::Update, false, Some(format!("密钥环更新失败: {}", e))).await;
                    return Err(ErrorInfo::new(1018, format!("更新密钥到密钥环失败: {}", e))
                        .with_category(ErrorCategory::Authentication)
                        .with_severity(ErrorSeverity::Error));
                }

                // 更新缓存
                if self.enable_cache {
                    let mut cache = self.key_cache.write().await;
                    cache.insert(key_id.to_string(), entry);
                }

                self.log_access(key_id, KeyOperation::Update, true, None).await;
                info!("密钥更新成功: {}", key_id);
                Ok(())
            }
            Err(e) => {
                self.log_access(key_id, KeyOperation::Update, false, Some(format!("创建密钥环条目失败: {}", e))).await;
                Err(ErrorInfo::new(1019, format!("创建密钥环条目失败: {}", e))
                    .with_category(ErrorCategory::Authentication)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 删除密钥
    pub async fn delete_key(&self, key_id: &str) -> Result<bool, ErrorInfo> {
        info!("删除密钥: {}", key_id);

        // 从密钥环删除
        let service_name = format!("{}_{}", self.service_prefix, key_id);
        let entry_result = Entry::new(&service_name, key_id);

        match entry_result {
            Ok(keyring_entry) => {
                match keyring_entry.delete_credential() {
                    Ok(_) => {
                        // 从缓存删除
                        if self.enable_cache {
                            let mut cache = self.key_cache.write().await;
                            cache.remove(key_id);
                        }

                        self.log_access(key_id, KeyOperation::Delete, true, None).await;
                        info!("密钥删除成功: {}", key_id);
                        Ok(true)
                    }
                    Err(KeyringError::NoEntry) => {
                        self.log_access(key_id, KeyOperation::Delete, false, Some("密钥不存在".to_string())).await;
                        Ok(false)
                    }
                    Err(e) => {
                        self.log_access(key_id, KeyOperation::Delete, false, Some(format!("删除密钥失败: {}", e))).await;
                        Err(ErrorInfo::new(1020, format!("删除密钥失败: {}", e))
                            .with_category(ErrorCategory::Authentication)
                            .with_severity(ErrorSeverity::Error))
                    }
                }
            }
            Err(e) => {
                self.log_access(key_id, KeyOperation::Delete, false, Some(format!("创建密钥环条目失败: {}", e))).await;
                Err(ErrorInfo::new(1021, format!("创建密钥环条目失败: {}", e))
                    .with_category(ErrorCategory::Authentication)
                    .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 列出所有密钥
    pub async fn list_keys(&self) -> Result<Vec<String>, ErrorInfo> {
        debug!("列出所有密钥");

        if self.enable_cache {
            let cache = self.key_cache.read().await;
            let keys = cache.keys().cloned().collect();
            self.log_access("", KeyOperation::List, true, None).await;
            return Ok(keys);
        }

        // 由于keyring库不支持直接列出所有密钥，这里返回空列表
        // 在实际应用中，可能需要维护一个单独的密钥索引
        self.log_access("", KeyOperation::List, true, None).await;
        Ok(Vec::new())
    }

    /// 生成AES密钥
    pub async fn generate_aes_key(&self, key_id: &str, description: String, key_size_bits: usize) -> Result<(), ErrorInfo> {
        use rand::RngCore;

        if key_size_bits != 128 && key_size_bits != 192 && key_size_bits != 256 {
            return Err(ErrorInfo::new(1022, "无效的AES密钥长度，仅支持128、192或256位".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        let mut key_data = vec![0u8; key_size_bits / 8];
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut key_data);

        self.create_key(key_id, key_data, KeyType::AesEncryption, description).await
    }

    /// 生成HMAC密钥
    pub async fn generate_hmac_key(&self, key_id: &str, description: String, key_size_bytes: usize) -> Result<(), ErrorInfo> {
        use rand::RngCore;

        if key_size_bytes < 16 || key_size_bytes > 1024 {
            return Err(ErrorInfo::new(1023, "无效的HMAC密钥长度，必须在16到1024字节之间".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        let mut key_data = vec![0u8; key_size_bytes];
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut key_data);

        self.create_key(key_id, key_data, KeyType::Hmac, description).await
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        if self.enable_cache {
            let mut cache = self.key_cache.write().await;
            cache.clear();
            info!("密钥缓存已清空");
        }
    }

    /// 获取访问日志
    pub async fn get_access_log(&self, limit: Option<usize>) -> Vec<AccessLogEntry> {
        let log = self.access_log.read().await;
        match limit {
            Some(limit) => log.iter().rev().take(limit).cloned().collect(),
            None => log.iter().rev().cloned().collect(),
        }
    }

    /// 清空访问日志
    pub async fn clear_access_log(&self) {
        let mut log = self.access_log.write().await;
        log.clear();
        info!("访问日志已清空");
    }

    /// 记录访问日志
    async fn log_access(&self, key_id: &str, operation: KeyOperation, success: bool, error: Option<String>) {
        let mut log = self.access_log.write().await;
        log.push(AccessLogEntry {
            key_id: key_id.to_string(),
            operation,
            timestamp: std::time::SystemTime::now(),
            success,
            error,
        });

        // 限制日志大小，避免内存泄漏
        if log.len() > 10000 {
            log.drain(0..5000); // 删除前5000条记录
        }
    }
}

/// 便捷函数：创建默认的密钥管理器
pub fn create_default_key_manager() -> Result<SecureKeyManager, ErrorInfo> {
    SecureKeyManager::new("default", true)
}

/// 便捷函数：创建用于云存储的密钥管理器
pub fn create_cloud_storage_key_manager() -> Result<SecureKeyManager, ErrorInfo> {
    SecureKeyManager::new("cloud_storage", true)
}

/// 便捷函数：创建用于分布式存储的密钥管理器
pub fn create_distributed_storage_key_manager() -> Result<SecureKeyManager, ErrorInfo> {
    SecureKeyManager::new("distributed_storage", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_key_manager_creation() {
        let manager = SecureKeyManager::new("test", true).unwrap();
        assert!(manager.enable_cache);
    }

    #[tokio::test]
    async fn test_aes_key_generation() {
        let manager = SecureKeyManager::new("test_aes", true).unwrap();
        let key_id = "test_aes_key";

        let result = manager.generate_aes_key(key_id, "Test AES Key".to_string(), 256).await;
        assert!(result.is_ok());

        // 检查密钥是否存在
        let key_data = manager.get_key(key_id).await.unwrap();
        assert_eq!(key_data.len(), 32); // 256 bits = 32 bytes

        // 清理
        let _ = manager.delete_key(key_id).await;
    }

    #[tokio::test]
    async fn test_hmac_key_generation() {
        let manager = SecureKeyManager::new("test_hmac", true).unwrap();
        let key_id = "test_hmac_key";

        let result = manager.generate_hmac_key(key_id, "Test HMAC Key".to_string(), 32).await;
        assert!(result.is_ok());

        // 检查密钥是否存在
        let key_data = manager.get_key(key_id).await.unwrap();
        assert_eq!(key_data.len(), 32);

        // 清理
        let _ = manager.delete_key(key_id).await;
    }

    #[tokio::test]
    async fn test_key_metadata() {
        let manager = SecureKeyManager::new("test_metadata", true).unwrap();
        let key_id = "test_metadata_key";

        manager.generate_aes_key(key_id, "Test Key".to_string(), 128).await.unwrap();

        let metadata = manager.get_key_metadata(key_id).await.unwrap();
        assert_eq!(metadata.key_type, KeyType::AesEncryption);
        assert_eq!(metadata.description, "Test Key");
        assert_eq!(metadata.version, 1);
        assert!(metadata.enabled);

        // 清理
        let _ = manager.delete_key(key_id).await;
    }

    #[tokio::test]
    async fn test_key_update() {
        let manager = SecureKeyManager::new("test_update", true).unwrap();
        let key_id = "test_update_key";

        // 创建初始密钥
        manager.generate_aes_key(key_id, "Test Key".to_string(), 128).await.unwrap();

        let original_data = manager.get_key(key_id).await.unwrap();

        // 更新密钥
        let new_key_data = vec![1u8; 16]; // 16 bytes for 128-bit key
        manager.update_key(key_id, new_key_data.clone(), Some("Updated Key".to_string())).await.unwrap();

        let updated_data = manager.get_key(key_id).await.unwrap();
        assert_ne!(original_data, updated_data);
        assert_eq!(updated_data, new_key_data);

        // 检查版本号更新
        let metadata = manager.get_key_metadata(key_id).await.unwrap();
        assert_eq!(metadata.version, 2);
        assert_eq!(metadata.description, "Updated Key");

        // 清理
        let _ = manager.delete_key(key_id).await;
    }

    #[tokio::test]
    async fn test_access_log() {
        let manager = SecureKeyManager::new("test_log", true).unwrap();
        let key_id = "test_log_key";

        // 执行一些操作
        let _ = manager.generate_aes_key(key_id, "Test Key".to_string(), 128).await;
        let _ = manager.get_key(key_id).await;
        let _ = manager.delete_key(key_id).await;

        // 检查访问日志
        let log = manager.get_access_log(Some(10)).await;
        assert!(log.len() >= 3); // 至少创建、读取、删除操作

        // 检查操作类型
        let operations: std::collections::HashSet<_> = log.iter()
            .take(3)
            .map(|entry| entry.operation)
            .collect();

        assert!(operations.contains(&KeyOperation::Create));
        assert!(operations.contains(&KeyOperation::Read));
        assert!(operations.contains(&KeyOperation::Delete));
    }
}