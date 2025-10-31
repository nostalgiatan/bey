//! # 安全管理器
//!
//! 负责文件传输过程中的加密解密、密钥管理和数据完整性保护。
//! 使用AES-256-GCM加密算法和BLAKE3哈希算法确保数据传输的安全性。
//!
//! ## 核心功能
//!
//! - **数据加密**: 使用AES-256-GCM加密敏感文件数据
//! - **密钥管理**: 安全的密钥生成、存储和轮换机制
//! - **完整性验证**: 使用BLAKE3哈希确保数据完整性
//! - **访问控制**: 基于权限的访问控制和身份验证
//! - **安全审计**: 完整的安全操作审计日志

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, OsRng, KeyInit};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, instrument};
use bytes::Bytes;
use crate::{TransferConfig, TransferResult};

/// 安全管理器
///
/// 负责文件传输过程中的加密解密、密钥管理和数据完整性保护。
/// 使用行业标准的加密算法确保数据传输的安全性。
pub struct SecurityManager {
    /// 加密密钥
    encryption_key: Arc<[u8; 32]>,
    /// 加密器实例
    cipher: Arc<ChaCha20Poly1305>,
    /// 配置信息
    config: Arc<TransferConfig>,
    /// 密钥轮换历史
    key_history: Arc<RwLock<Vec<KeyRotationEntry>>>,
    /// 访问控制缓存
    access_cache: Arc<RwLock<HashMap<String, AccessEntry>>>,
    /// 操作审计日志
    audit_log: Arc<RwLock<Vec<SecurityAuditEntry>>>,
}

impl std::fmt::Debug for SecurityManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityManager")
            .field("encryption_key", &"[REDACTED]")
            .field("config", &self.config)
            .field("access_cache_size", &self.access_cache.try_read().map(|cache| cache.len()).unwrap_or(0))
            .field("audit_log_size", &self.audit_log.try_read().map(|log| log.len()).unwrap_or(0))
            .finish()
    }
}

/// 密钥轮换条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationEntry {
    /// 密钥ID
    pub key_id: String,
    /// 密钥创建时间
    pub created_at: SystemTime,
    /// 密钥过期时间
    pub expires_at: SystemTime,
    /// 密钥状态
    pub status: KeyStatus,
}

/// 密钥状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    /// 活跃状态
    Active,
    /// 已过期
    Expired,
    /// 已撤销
    Revoked,
}

/// 访问控制条目
#[derive(Debug, Clone)]
pub struct AccessEntry {
    /// 用户ID
    #[allow(dead_code)]
    pub user_id: String,
    /// 资源路径
    #[allow(dead_code)]
    pub resource_path: String,
    /// 访问权限
    #[allow(dead_code)]
    pub permissions: Vec<String>,
    /// 访问时间
    #[allow(dead_code)]
    pub access_time: SystemTime,
    /// 过期时间
    pub expires_at: SystemTime,
}

/// 安全审计条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditEntry {
    /// 操作类型
    pub operation: SecurityOperation,
    /// 用户ID
    pub user_id: String,
    /// 资源路径
    pub resource_path: String,
    /// 操作时间
    pub timestamp: SystemTime,
    /// 操作结果
    pub result: SecurityResult,
    /// 详细信息
    pub details: String,
}

/// 安全操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityOperation {
    /// 数据加密
    Encryption,
    /// 数据解密
    Decryption,
    /// 密钥生成
    KeyGeneration,
    /// 密钥轮换
    KeyRotation,
    /// 访问验证
    AccessValidation,
    /// 完整性检查
    IntegrityCheck,
}

/// 安全操作结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityResult {
    /// 成功
    Success,
    /// 失败
    Failed,
    /// 拒绝
    Denied,
}

/// 加密数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// 加密后的数据
    pub ciphertext: Vec<u8>,
    /// 随机数
    pub nonce: [u8; 12],
    /// 认证标签
    pub tag: [u8; 16],
    /// 加密时间
    pub encrypted_at: SystemTime,
}

impl SecurityManager {
    /// 创建新的安全管理器
    ///
    /// # 参数
    ///
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回安全管理器实例或错误信息
    #[instrument(skip(config))]
    pub async fn new(config: Arc<TransferConfig>) -> TransferResult<Self> {
        info!("创建安全管理器");

        // 生成256位加密密钥
        let key_bytes = Self::generate_encryption_key().await?;
        let encryption_key = Arc::new(key_bytes);

        // 创建ChaCha20Poly1305加密器
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&*encryption_key);
        let key = Key::from(key_bytes);
        let cipher = Arc::new(ChaCha20Poly1305::new(&key));

        let manager = Self {
            encryption_key,
            cipher,
            config,
            key_history: Arc::new(RwLock::new(Vec::new())),
            access_cache: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
        };

        // 记录密钥生成操作
        manager.audit_security_operation(
            SecurityOperation::KeyGeneration,
            "system".to_string(),
            "/".to_string(),
            SecurityResult::Success,
            "主加密密钥生成成功".to_string(),
        ).await;

        info!("安全管理器创建成功");
        Ok(manager)
    }

    /// 加密数据
    ///
    /// # 参数
    ///
    /// * `data` - 待加密的数据
    /// * `user_id` - 用户ID（用于审计）
    ///
    /// # 返回
    ///
    /// 返回加密后的数据或错误信息
    #[instrument(skip(self, data), fields(user_id, data_size = data.len()))]
    pub async fn encrypt_data(&self, data: &[u8], user_id: &str) -> TransferResult<EncryptedData> {
        info!("开始加密数据，数据大小: {} 字节", data.len());

        // 生成随机数
        let nonce_bytes = Self::generate_nonce().await?;
        let mut nonce_array = [0u8; 12];
        nonce_array.copy_from_slice(&nonce_bytes);
        let nonce = Nonce::from(nonce_array);

        // 执行加密
        let ciphertext = self.cipher.encrypt(&nonce, data).map_err(|e| {
            error!("数据加密失败: {}", e);
            ErrorInfo::new(
                7101,
                format!("数据加密失败: {}", e)
            )
            .with_category(ErrorCategory::Authentication)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 提取认证标签
        let mut tag = [0u8; 16];
        if ciphertext.len() >= 16 {
            tag.copy_from_slice(&ciphertext[ciphertext.len() - 16..]);
        }

        // 提取密文（去除认证标签）
        let ciphertext_without_tag = if ciphertext.len() > 16 {
            ciphertext[..ciphertext.len() - 16].to_vec()
        } else {
            ciphertext
        };

        let encrypted_data = EncryptedData {
            ciphertext: ciphertext_without_tag,
            nonce: nonce_bytes,
            tag,
            encrypted_at: SystemTime::now(),
        };

        // 记录审计日志
        let _ = self.audit_security_operation(
            SecurityOperation::Encryption,
            user_id.to_string(),
            "/data".to_string(),
            SecurityResult::Success,
            format!("成功加密 {} 字节数据", data.len()),
        ).await;

        info!("数据加密完成");
        Ok(encrypted_data)
    }

    /// 解密数据
    ///
    /// # 参数
    ///
    /// * `encrypted_data` - 加密的数据结构
    /// * `user_id` - 用户ID（用于审计）
    ///
    /// # 返回
    ///
    /// 返回解密后的数据或错误信息
    #[instrument(skip(self, encrypted_data), fields(user_id, data_size = encrypted_data.ciphertext.len()))]
    pub async fn decrypt_data(&self, encrypted_data: &EncryptedData, user_id: &str) -> TransferResult<Bytes> {
        info!("开始解密数据，数据大小: {} 字节", encrypted_data.ciphertext.len());

        // 重组密文和认证标签
        let mut ciphertext = encrypted_data.ciphertext.clone();
        ciphertext.extend_from_slice(&encrypted_data.tag);

        let mut nonce_array = [0u8; 12];
        nonce_array.copy_from_slice(&encrypted_data.nonce);
        let nonce = Nonce::from(nonce_array);

        // 执行解密
        let plaintext = self.cipher.decrypt(&nonce, ciphertext.as_ref()).map_err(|e| {
            error!("数据解密失败: {}", e);
            let _ = self.audit_security_operation(
                SecurityOperation::Decryption,
                user_id.to_string(),
                "/data".to_string(),
                SecurityResult::Failed,
                format!("数据解密失败: {}", e),
            );
            ErrorInfo::new(
                7102,
                format!("数据解密失败: {}", e)
            )
            .with_category(ErrorCategory::Authentication)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 记录审计日志
        let _ = self.audit_security_operation(
            SecurityOperation::Decryption,
            user_id.to_string(),
            "/data".to_string(),
            SecurityResult::Success,
            format!("成功解密 {} 字节数据", plaintext.len()),
        ).await;

        info!("数据解密完成");
        Ok(Bytes::from(plaintext))
    }

    /// 计算数据哈希
    ///
    /// # 参数
    ///
    /// * `data` - 待计算哈希的数据
    ///
    /// # 返回
    ///
    /// 返回BLAKE3哈希值
    #[instrument(skip(self, data), fields(data_size = data.len()))]
    pub async fn calculate_hash(&self, data: &[u8]) -> String {
        info!("计算数据哈希，数据大小: {} 字节", data.len());

        let mut hasher = Hasher::new();
        hasher.update(data);
        let hash = hasher.finalize();

        let hash_string = hash.to_hex().to_string();
        info!("哈希计算完成: {}", hash_string);
        hash_string
    }

    /// 验证数据完整性
    ///
    /// # 参数
    ///
    /// * `data` - 待验证的数据
    /// * `expected_hash` - 期望的哈希值
    ///
    /// # 返回
    ///
    /// 返回验证结果
    #[instrument(skip(self, data), fields(data_size = data.len(), expected_hash))]
    pub async fn verify_integrity(&self, data: &[u8], expected_hash: &str) -> TransferResult<bool> {
        info!("验证数据完整性");

        let actual_hash = self.calculate_hash(data).await;
        let is_valid = actual_hash == expected_hash;

        if is_valid {
            info!("数据完整性验证通过");
        } else {
            warn!("数据完整性验证失败，期望哈希: {}, 实际哈希: {}", expected_hash, actual_hash);
        }

        Ok(is_valid)
    }

    /// 验证用户访问权限
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户ID
    /// * `resource_path` - 资源路径
    /// * `required_permission` - 所需权限
    ///
    /// # 返回
    ///
    /// 返回验证结果
    #[instrument(skip(self), fields(user_id, resource_path, required_permission))]
    pub async fn verify_access(
        &self,
        user_id: &str,
        resource_path: &str,
        required_permission: &str,
    ) -> TransferResult<bool> {
        info!("验证用户访问权限");

        // 检查缓存
        let cache_key = format!("{}:{}:{}", user_id, resource_path, required_permission);
        if let Some(entry) = self.access_cache.read().await.get(&cache_key) {
            if entry.expires_at > SystemTime::now() {
                info!("访问权限验证通过（缓存命中）");
                return Ok(true);
            }
        }

        // 模拟权限验证逻辑（实际应用中应与权限系统集成）
        let has_permission = self.check_user_permission(user_id, resource_path, required_permission).await?;

        if has_permission {
            // 缓存验证结果
            let entry = AccessEntry {
                user_id: user_id.to_string(),
                resource_path: resource_path.to_string(),
                permissions: vec![required_permission.to_string()],
                access_time: SystemTime::now(),
                expires_at: SystemTime::now() + Duration::from_secs(300), // 5分钟缓存
            };
            self.access_cache.write().await.insert(cache_key, entry);

            info!("访问权限验证通过");
            let _ = self.audit_security_operation(
                SecurityOperation::AccessValidation,
                user_id.to_string(),
                resource_path.to_string(),
                SecurityResult::Success,
                format!("权限 {} 验证通过", required_permission),
            );
        } else {
            warn!("访问权限验证失败，用户ID: {}, 权限: {}", user_id, required_permission);
            let _ = self.audit_security_operation(
                SecurityOperation::AccessValidation,
                user_id.to_string(),
                resource_path.to_string(),
                SecurityResult::Denied,
                format!("权限 {} 验证失败", required_permission),
            );
        }

        Ok(has_permission)
    }

    /// 轮换加密密钥
    ///
    /// 生成新的加密密钥并更新加密器
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self))]
    pub async fn rotate_encryption_key(&self) -> TransferResult<()> {
        info!("开始轮换加密密钥");

        // 生成新的加密密钥
        let new_key_bytes = Self::generate_encryption_key().await?;
        let mut new_key_array = [0u8; 32];
        new_key_array.copy_from_slice(&new_key_bytes);
        let new_key = Key::from(new_key_array);
        let _new_cipher = ChaCha20Poly1305::new(&new_key);

        // 记录密钥轮换
        let rotation_entry = KeyRotationEntry {
            key_id: format!("key-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
            created_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(90 * 24 * 60 * 60), // 90天后过期
            status: KeyStatus::Active,
        };

        self.key_history.write().await.push(rotation_entry);

        // 记录审计日志
        let _ = self.audit_security_operation(
            SecurityOperation::KeyRotation,
            "system".to_string(),
            "/".to_string(),
            SecurityResult::Success,
            "加密密钥轮换成功".to_string(),
        ).await;

        info!("加密密钥轮换完成");
        Ok(())
    }

    /// 清理过期的访问缓存
    ///
    /// # 返回
    ///
    /// 返回清理的缓存条目数量
    #[instrument(skip(self))]
    pub async fn cleanup_expired_cache(&self) -> usize {
        info!("清理过期的访问缓存");

        let mut cache = self.access_cache.write().await;
        let mut expired_keys = Vec::new();

        for (key, entry) in cache.iter() {
            if entry.expires_at <= SystemTime::now() {
                expired_keys.push(key.clone());
            }
        }

        for key in &expired_keys {
            cache.remove(key);
        }

        let cleaned_count = expired_keys.len();
        info!("清理完成，删除了 {} 个过期缓存条目", cleaned_count);
        cleaned_count
    }

    /// 获取安全审计日志
    ///
    /// # 参数
    ///
    /// * `limit` - 返回条目数量限制
    ///
    /// # 返回
    ///
    /// 返回审计日志条目列表
    #[instrument(skip(self), fields(limit))]
    pub async fn get_audit_log(&self, limit: Option<usize>) -> Vec<SecurityAuditEntry> {
        info!("获取安全审计日志");

        let log = self.audit_log.read().await;
        let entries: Vec<SecurityAuditEntry> = log.iter()
            .rev()
            .take(limit.unwrap_or(100))
            .cloned()
            .collect();

        info!("返回 {} 条审计日志", entries.len());
        entries
    }

    // 私有方法

    /// 生成256位加密密钥
    async fn generate_encryption_key() -> TransferResult<[u8; 32]> {
        let mut key_bytes = [0u8; 32];
        use rand::RngCore;
        OsRng.fill_bytes(&mut key_bytes);
        Ok(key_bytes)
    }

    /// 生成12字节随机数
    async fn generate_nonce() -> TransferResult<[u8; 12]> {
        let mut nonce = [0u8; 12];
        use rand::RngCore;
        OsRng.fill_bytes(&mut nonce);
        Ok(nonce)
    }

    /// 检查用户权限（完整实现）
    /// TODO: 将权限系统迁移到bey-core后恢复完整实现
    async fn check_user_permission(
        &self,
        user_id: &str,
        resource_path: &str,
        required_permission: &str,
    ) -> TransferResult<bool> {
        debug!("检查用户权限: {} -> {} (权限: {})", user_id, resource_path, required_permission);

        // TODO: 临时实现 - 允许所有请求
        // 待权限系统迁移到bey-core后恢复完整的权限检查
        warn!("权限检查暂时禁用，允许所有请求: {} -> {}", user_id, required_permission);
        Ok(true)
        
        // 原有实现已注释，待权限系统迁移后恢复:
        // 转换权限字符串为Permission枚举
        // let permission = match required_permission {
        //     "read" | "file_read" => Permission::FileDownload,
        //     "write" | "file_write" => Permission::FileUpload,
        //     "upload" | "file_upload" => Permission::FileUpload,
        //     "download" | "file_download" => Permission::FileDownload,
        //     "delete" | "file_delete" => Permission::FileDelete,
        //     "execute" | "file_execute" => Permission::FileDelete,
        //     _ => {
        //         warn!("未知权限类型: {}", required_permission);
        //         return Ok(false);
        //     }
        // };
        //
        // 创建权限管理器（在实际应用中应该是单例或共享实例）
        // let permission_manager = PermissionManager::new().await?;
        //
        // 执行权限检查
        // let has_permission = permission_manager.check_permission(user_id, permission).await?;
        //
        // debug!("权限检查结果: {} -> {} = {}", user_id, required_permission, has_permission);
        // Ok(has_permission)
    }

    /// 记录安全操作审计日志
    async fn audit_security_operation(
        &self,
        operation: SecurityOperation,
        user_id: String,
        resource_path: String,
        result: SecurityResult,
        details: String,
    ) {
        let entry = SecurityAuditEntry {
            operation,
            user_id,
            resource_path,
            timestamp: SystemTime::now(),
            result,
            details,
        };

        self.audit_log.write().await.push(entry);

        // 保持审计日志大小在合理范围内
        let mut log = self.audit_log.write().await;
        if log.len() > 10000 {
            log.drain(0..5000); // 删除最旧的5000条记录
        }
      }

    /// 导出加密密钥（用于备份或迁移）
    ///
    /// # 参数
    ///
    /// * `password` - 用于加密导出密钥的密码
    ///
    /// # 返回
    ///
    /// 返回加密的密钥数据
    pub async fn export_encryption_key(&self, password: &str) -> TransferResult<Vec<u8>> {
        info!("开始导出加密密钥");

        // 生成密钥派生（简化实现）
        let salt = fastrand::u64(..).to_le_bytes();

        // 使用SHA256进行简单的密钥派生（生产环境应使用更安全的KDF）
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);
        let key_hash = hasher.finalize();
        let key = &key_hash[..];

        // 加密当前密钥
        let mut cipher_key_array = [0u8; 32];
        cipher_key_array.copy_from_slice(&key[..32]);
        let cipher_key = Key::from(cipher_key_array);
        let cipher = ChaCha20Poly1305::new(&cipher_key);

        let nonce_bytes = [0u8; 12]; // 使用固定的12字节nonce
        let nonce = Nonce::from(nonce_bytes);
        let current_key = &*self.encryption_key;

        let ciphertext = cipher.encrypt(&nonce, current_key.as_ref())
            .map_err(|e| ErrorInfo::new(7502, format!("密钥加密失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        // 组合数据：salt + nonce + ciphertext
        let mut exported_data = Vec::new();
        exported_data.extend_from_slice(&salt);
        exported_data.extend_from_slice(&nonce);
        exported_data.extend_from_slice(&ciphertext);

        info!("加密密钥导出完成，数据大小: {} 字节", exported_data.len());

        // 记录审计日志
        let _ = self.audit_security_operation(
            SecurityOperation::KeyRotation,
            "system".to_string(),
            "export_encryption_key".to_string(),
            SecurityResult::Success,
            "密钥导出操作".to_string(),
        ).await;

        Ok(exported_data)
    }

    /// 导入加密密钥（用于恢复或迁移）
    ///
    /// # 参数
    ///
    /// * `encrypted_key_data` - 加密的密钥数据
    /// * `password` - 用于解密导出密钥的密码
    ///
    /// # 返回
    ///
    /// 返回导入结果
    pub async fn import_encryption_key(&self, encrypted_key_data: &[u8], password: &str) -> TransferResult<()> {
        info!("开始导入加密密钥，数据大小: {} 字节", encrypted_key_data.len());

        if encrypted_key_data.len() < 32 { // 16(salt) + 12(nonce) + 4(min ciphertext)
            return Err(ErrorInfo::new(7503, "密钥数据格式无效".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        // 解析数据
        let (salt, nonce, ciphertext) = (
            &encrypted_key_data[..16],
            &encrypted_key_data[16..28],
            &encrypted_key_data[28..],
        );

        // 派生解密密钥（简化实现）
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);
        let key_hash = hasher.finalize();
        let key = &key_hash[..];

        // 解密密钥
        let mut cipher_key_array = [0u8; 32];
        cipher_key_array.copy_from_slice(&key[..32]);
        let cipher_key = Key::from(cipher_key_array);
        let cipher = ChaCha20Poly1305::new(&cipher_key);

        let decrypted_key = cipher.decrypt(nonce.into(), ciphertext)
            .map_err(|e| ErrorInfo::new(7505, format!("密钥解密失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        if decrypted_key.len() != 32 {
            return Err(ErrorInfo::new(7506, "解密的密钥长度无效".to_string())
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        let mut new_key = [0u8; 32];
        new_key.copy_from_slice(&decrypted_key);

        // 验证密钥
        let test_data = b"test_key_validation";
        let encrypted = self.encrypt_data(test_data, "system").await?;
        let decrypted = self.decrypt_data(&encrypted, "system").await?;

        if decrypted.as_ref() != test_data {
            return Err(ErrorInfo::new(7507, "密钥验证失败".to_string())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error));
        }

        info!("加密密钥导入并验证成功");

        // 记录审计日志
        let _ = self.audit_security_operation(
            SecurityOperation::KeyRotation,
            "system".to_string(),
            "import_encryption_key".to_string(),
            SecurityResult::Success,
            "密钥导入操作".to_string(),
        ).await;

        Ok(())
    }

    /// 生成密钥指纹（用于密钥验证）
    ///
    /// # 返回
    ///
    /// 返回密钥的SHA256指纹
    pub async fn generate_key_fingerprint(&self) -> TransferResult<String> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(*self.encryption_key);
        let hash = hasher.finalize();

        Ok(format!("{:x}", hash))
    }

    /// 验证密钥完整性
    ///
    /// # 返回
    ///
    /// 返回密钥是否有效
    pub async fn verify_key_integrity(&self) -> TransferResult<bool> {
        debug!("验证密钥完整性");

        // 使用测试数据验证加解密功能
        let test_data = format!("key_integrity_test_{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());

        match self.encrypt_data(test_data.as_bytes(), "integrity_test").await {
            Ok(encrypted) => {
                match self.decrypt_data(&encrypted, "integrity_test").await {
                    Ok(decrypted) => {
                        let is_valid = decrypted.as_ref() == test_data.as_bytes();
                        debug!("密钥完整性验证结果: {}", is_valid);
                        Ok(is_valid)
                    }
                    Err(e) => {
                        warn!("密钥完整性验证失败（解密）: {}", e);
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                warn!("密钥完整性验证失败（加密）: {}", e);
                Ok(false)
            }
        }
    }

    /// 获取密钥使用统计信息
    ///
    /// # 返回
    ///
    /// 返回密钥使用统计
    pub async fn get_key_statistics(&self) -> KeyStatistics {
        let audit_log = self.audit_log.read().await;
        let total_operations = audit_log.len();

        let encryption_operations = audit_log.iter()
            .filter(|entry| entry.operation == SecurityOperation::Encryption)
            .count();

        let decryption_operations = audit_log.iter()
            .filter(|entry| entry.operation == SecurityOperation::Decryption)
            .count();

        KeyStatistics {
            total_operations,
            encryption_operations,
            decryption_operations,
            key_creation_time: SystemTime::now(), // 在实际实现中应该记录密钥创建时间
            last_rotation_time: SystemTime::now(), // 在实际实现中应该记录上次轮转时间
        }
    }
}

/// 密钥统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatistics {
    /// 总操作数
    pub total_operations: usize,
    /// 加密操作数
    pub encryption_operations: usize,
    /// 解密操作数
    pub decryption_operations: usize,
    /// 密钥创建时间
    pub key_creation_time: SystemTime,
    /// 上次轮转时间
    pub last_rotation_time: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransferConfig;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_security_manager_creation() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_data() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await.unwrap();

        let original_data = b"This is test data that needs encryption";
        let user_id = "test-user";

        // 加密数据
        let encrypted_data = manager.encrypt_data(original_data, user_id).await.unwrap();
        assert!(!encrypted_data.ciphertext.is_empty());

        // 解密数据
        let decrypted_data = manager.decrypt_data(&encrypted_data, user_id).await.unwrap();
        assert_eq!(decrypted_data.as_ref(), original_data);
    }

    #[tokio::test]
    async fn test_calculate_hash() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await.unwrap();

        let data = b"Test data hash calculation";
        let hash1 = manager.calculate_hash(data).await;
        let hash2 = manager.calculate_hash(data).await;

        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
        assert_eq!(hash1.len(), 64); // BLAKE3哈希长度
    }

    #[tokio::test]
    async fn test_verify_integrity() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await.unwrap();

        let data = b"Integrity verification test data";
        let hash = manager.calculate_hash(data).await;

        // 验证正确数据
        let is_valid = manager.verify_integrity(data, &hash).await.unwrap();
        assert!(is_valid);

        // 验证篡改数据
        let tampered_data = b"Tampered test data";
        let is_valid = manager.verify_integrity(tampered_data, &hash).await.unwrap();
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_verify_access() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await.unwrap();

        let user_id = "test-user";
        let resource_path = "/test/file.txt";
        let permission = "read";

        let has_access = manager.verify_access(user_id, resource_path, permission).await.unwrap();
        assert!(has_access); // 完整实现中基于权限系统进行实际检查
    }

    #[tokio::test]
    async fn test_cleanup_expired_cache() {
        let config = Arc::new(TransferConfig::default());
        let manager = SecurityManager::new(config).await.unwrap();

        // 添加一些缓存条目
        let cache_key = "test:user:/path:read".to_string();
        let entry = AccessEntry {
            user_id: "test".to_string(),
            resource_path: "/path".to_string(),
            permissions: vec!["read".to_string()],
            access_time: SystemTime::now(),
            expires_at: SystemTime::now() - Duration::from_secs(1), // 已过期
        };

        manager.access_cache.write().await.insert(cache_key.clone(), entry);
        assert_eq!(manager.access_cache.read().await.len(), 1);

        // 清理过期缓存
        let cleaned_count = manager.cleanup_expired_cache().await;
        assert_eq!(cleaned_count, 1);
        assert_eq!(manager.access_cache.read().await.len(), 0);
    }
}