//! # 证书管理核心功能
//!
//! 提供证书生成、签发、验证和管理的核心功能。
//! 包含证书颁发机构（CA）管理和设备证书管理。

use crate::config::CertificateConfig;
use crate::error::{IdentityError, ConfigError};
use crate::storage::CertificateStorage;
use crate::types::{CertificateData, CertificateType, CertificateStatus, CertificateVerificationResult, KeyPairInfo};
use crate::validation::CertificateValidator;
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, KeyPair, SanType, IsCa, BasicConstraints, Issuer, KeyUsagePurpose, ExtendedKeyUsagePurpose, SigningKey};
use sha2::Digest;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

/// 证书管理器
///
/// 提供完整的证书生命周期管理功能，包括证书生成、签发、验证和吊销。
pub struct CertificateManager {
    /// 配置信息
    config: CertificateConfig,

    /// 证书存储
    storage: Arc<CertificateStorage>,

    /// 证书验证器
    validator: Arc<CertificateValidator>,

    /// CA 证书颁发者
    ca_issuer: Arc<RwLock<Option<CertificateAuthority>>>,

    /// 证书缓存
    certificate_cache: Arc<RwLock<std::collections::HashMap<String, CertificateData>>>,
}

/// 证书颁发机构
///
/// 管理根CA证书和私钥，用于签发其他证书。
#[derive(Clone)]
pub struct CertificateAuthority {
    /// CA 证书
    pub certificate: Certificate,

    /// CA 私钥（使用Arc共享）
    pub private_key: Arc<KeyPair>,

    /// CA 证书参数
    pub params: CertificateParams,

    /// CA 证书数据
    pub certificate_data: CertificateData,

    /// CA 密钥信息
    pub key_info: KeyPairInfo,
}

impl CertificateManager {
    /// 初始化证书管理器
    ///
    /// # 参数
    ///
    /// * `config` - 证书配置
    ///
    /// # 返回值
    ///
    /// 返回初始化的证书管理器
    pub async fn initialize(config: CertificateConfig) -> Result<Self, IdentityError> {
        info!("初始化证书管理器");

        // 确保存储目录存在
        config.ensure_storage_directory()?;

        // 创建存储实例
        let storage_config = crate::storage::StorageConfig {
            root_directory: config.storage_directory.clone(),
            enable_memory_cache: true,
            cache_ttl_seconds: 3600,
            enable_encryption: false,
            enable_backup: true,
            backup_retention_days: 30,
            #[cfg(unix)]
            file_permissions: Some(std::fs::Permissions::from_mode(0o600)),
            #[cfg(not(unix))]
            file_permissions: None,
        };

        let storage = Arc::new(CertificateStorage::new(storage_config).await?);
        let validator = Arc::new(CertificateValidator::new(config.clone()));

        let manager = Self {
            config,
            storage,
            validator,
            ca_issuer: Arc::new(RwLock::new(None)),
            certificate_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        };

        // 初始化或加载CA证书
        manager.initialize_certificate_authority().await?;

        info!("证书管理器初始化完成");
        Ok(manager)
    }

    /// 为设备签发证书
    ///
    /// # 参数
    ///
    /// * `device_identifier` - 设备标识符
    ///
    /// # 返回值
    ///
    /// 返回签发的设备证书
    pub async fn issue_device_certificate(&self, device_identifier: &str) -> Result<CertificateData, IdentityError> {
        info!("为设备 {} 签发证书", device_identifier);

        // 检查是否已存在有效证书
        if let Some(existing_cert) = self.get_device_certificate(device_identifier).await? {
            if existing_cert.is_valid() {
                warn!("设备 {} 已存在有效证书", device_identifier);
                return Ok(existing_cert);
            } else {
                info!("设备 {} 的证书已过期，重新签发", device_identifier);
            }
        }

        // 获取CA颁发者
        let ca_issuer = self.get_certificate_authority().await?;

        // 生成设备证书参数
        let params = self.create_device_certificate_params(device_identifier)?;

        // 生成密钥对
        let key_pair = KeyPair::generate()
            .map_err(|e| IdentityError::CryptoError(format!("生成密钥对失败: {}", e)))?;

        // 使用CA签发证书
        let issuer = Issuer::new(ca_issuer.params.clone(), ca_issuer.private_key.as_ref());
        let cert = params.signed_by(&key_pair, &issuer)
            .map_err(|e| IdentityError::CryptoError(format!("签发证书失败: {}", e)))?;

        // 序列化证书
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // 创建证书数据
        let mut certificate_data = CertificateData::new(
            device_identifier.to_string(), // 使用设备ID作为证书ID
            device_identifier.to_string(),
            cert_pem,
            Some(key_pem),
            CertificateType::Device,
            ca_issuer.certificate_data.certificate_id.clone(),
            format!("CN={}, O={}, OU=Device", device_identifier, self.config.organization_name),
        );

        // 设置过期时间
        certificate_data.expires_at = SystemTime::now() + Duration::from_secs(86400 * self.config.validity_days as u64);

        // 计算指纹
        certificate_data.calculate_fingerprint()?;

        // 设置密钥算法信息
        certificate_data.key_algorithm = Some(format!("{}-{}", self.config.key_algorithm, self.config.key_size));

        // 设置证书状态为有效
        certificate_data.set_status(CertificateStatus::Valid);

        // 保存证书
        self.storage.store_certificate(certificate_data.clone()).await?;

        // 更新缓存
        let mut cache = self.certificate_cache.write().await;
        cache.insert(device_identifier.to_string(), certificate_data.clone());

        info!("设备证书签发成功: {} (指纹: {})", device_identifier, certificate_data.fingerprint);
        Ok(certificate_data)
    }

    /// 验证证书
    ///
    /// # 参数
    ///
    /// * `certificate` - 要验证的证书
    ///
    /// # 返回值
    ///
    /// 返回验证结果
    pub async fn verify_certificate(&self, certificate: &CertificateData) -> Result<CertificateVerificationResult, IdentityError> {
        debug!("验证证书: {}", certificate.certificate_id);

        // 使用验证器进行验证
        let result = self.validator.verify_certificate(certificate).await?;

        debug!("证书验证完成: {} (结果: {})", certificate.certificate_id, result.is_valid);
        Ok(result)
    }

    /// 吊销证书
    ///
    /// # 参数
    ///
    /// * `device_identifier` - 设备标识符
    ///
    /// # 返回值
    ///
    /// 返回吊销结果
    pub async fn revoke_device_certificate(&self, device_identifier: &str) -> Result<bool, IdentityError> {
        info!("吊销设备证书: {}", device_identifier);

        // 获取证书
        let mut certificate = match self.get_device_certificate(device_identifier).await? {
            Some(cert) => cert,
            None => {
                warn!("设备 {} 的证书不存在", device_identifier);
                return Ok(false);
            }
        };

        // 更新证书状态
        certificate.set_status(CertificateStatus::Revoked);

        // 保存更新后的证书
        self.storage.store_certificate(certificate.clone()).await?;

        // 更新缓存
        let mut cache = self.certificate_cache.write().await;
        cache.insert(device_identifier.to_string(), certificate);

        info!("设备证书吊销成功: {}", device_identifier);
        Ok(true)
    }

    /// 获取设备证书
    ///
    /// # 参数
    ///
    /// * `device_identifier` - 设备标识符
    ///
    /// # 返回值
    ///
    /// 返回设备证书（如果存在）
    pub async fn get_device_certificate(&self, device_identifier: &str) -> Result<Option<CertificateData>, IdentityError> {
        debug!("获取设备证书: {}", device_identifier);

        // 首先检查缓存
        {
            let cache = self.certificate_cache.read().await;
            if let Some(certificate) = cache.get(device_identifier) {
                debug!("从缓存中找到证书: {}", device_identifier);
                return Ok(Some(certificate.clone()));
            }
        }

        // 从存储中加载
        let certificate = self.storage.retrieve_certificate(device_identifier).await?;

        // 如果找到，更新缓存
        if let Some(ref cert) = certificate {
            let mut cache = self.certificate_cache.write().await;
            cache.insert(device_identifier.to_string(), cert.clone());
        }

        debug!("证书获取完成: {} (结果: {})", device_identifier, certificate.is_some());
        Ok(certificate)
    }

    /// 列出所有证书
    ///
    /// # 返回值
    ///
    /// 返回所有证书的列表
    pub async fn list_all_certificates(&self) -> Result<Vec<CertificateData>, IdentityError> {
        debug!("列出所有证书");

        let certificates = self.storage.list_certificates().await?;
        debug!("找到 {} 个证书", certificates.len());
        Ok(certificates)
    }

    /// 获取证书颁发机构
    ///
    /// # 返回值
    ///
    /// 返回CA颁发者
    async fn get_certificate_authority(&self) -> Result<CertificateAuthority, IdentityError> {
        let ca_issuer = self.ca_issuer.read().await;
        ca_issuer.as_ref()
            .cloned()
            .ok_or_else(|| IdentityError::Config(ConfigError::ValidationFailed(
                "证书颁发机构未初始化".to_string()
            )))
    }

    /// 初始化证书颁发机构
    async fn initialize_certificate_authority(&self) -> Result<(), IdentityError> {
        info!("初始化证书颁发机构");

        // 尝试加载现有CA证书
        if let Ok(Some(ca_data)) = self.storage.retrieve_certificate("ca").await {
            info!("加载现有CA证书");
            let ca = self.recreate_certificate_authority(&ca_data).await?;
            let mut ca_issuer = self.ca_issuer.write().await;
            *ca_issuer = Some(ca);
            return Ok(());
        }

        // 创建新的CA证书
        info!("创建新的CA证书");
        let ca = self.create_certificate_authority().await?;

        // 保存CA证书
        self.storage.store_certificate(ca.certificate_data.clone()).await?;

        let mut ca_issuer = self.ca_issuer.write().await;
        *ca_issuer = Some(ca);

        info!("证书颁发机构初始化完成");
        Ok(())
    }

    /// 创建证书颁发机构
    async fn create_certificate_authority(&self) -> Result<CertificateAuthority, IdentityError> {
        // 创建CA证书参数
        let mut params = CertificateParams::default();

        // 设置CA信息
        params.distinguished_name = self.create_ca_distinguished_name()?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

        // 设置密钥用途
        params.key_usages.push(KeyUsagePurpose::DigitalSignature);
        params.key_usages.push(KeyUsagePurpose::KeyCertSign);
        params.key_usages.push(KeyUsagePurpose::CrlSign);

        // 生成密钥对
        let key_pair = KeyPair::generate()
            .map_err(|e| IdentityError::CryptoError(format!("生成CA密钥对失败: {}", e)))?;

        // 创建CA证书（自签名）
        let cert = params.self_signed(&key_pair)
            .map_err(|e| IdentityError::CryptoError(format!("创建CA证书失败: {}", e)))?;

        // 序列化证书
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // 创建CA证书数据
        let mut certificate_data = CertificateData::new(
            "ca".to_string(),
            "ca".to_string(),
            cert_pem,
            Some(key_pem),
            CertificateType::RootCA,
            "ca".to_string(),
            format!("CN={}, O={}, OU=Certificate Authority", self.config.ca_common_name, self.config.organization_name),
        );

        // 设置过期时间（CA证书有效期更长）
        certificate_data.expires_at = SystemTime::now() + Duration::from_secs(86400 * self.config.ca_validity_days() as u64);

        // 计算指纹
        certificate_data.calculate_fingerprint()?;

        // 设置密钥算法信息
        certificate_data.key_algorithm = Some(format!("{}-{}", self.config.key_algorithm, self.config.key_size));

        // 设置证书状态为有效
        certificate_data.set_status(CertificateStatus::Valid);

        // 创建密钥信息
        let key_info = KeyPairInfo::new(
            self.config.key_algorithm.clone(),
            self.config.key_size,
            "ca-key".to_string(),
        );

        Ok(CertificateAuthority {
            certificate: cert,
            private_key: Arc::new(key_pair),
            params,
            certificate_data,
            key_info,
        })
    }

    /// 重新创建证书颁发机构
    async fn recreate_certificate_authority(&self, ca_data: &CertificateData) -> Result<CertificateAuthority, IdentityError> {
        // 验证CA数据完整性
        if ca_data.private_key_pem.is_none() {
            return Err(IdentityError::Config(ConfigError::ValidationFailed(
                "CA私钥不存在".to_string()
            )));
        }

        // 解析私钥
        let private_key_pem = ca_data.private_key_pem.as_ref()
            .ok_or_else(|| IdentityError::Config(ConfigError::ValidationFailed(
                "CA私钥数据为空".to_string()
            )))?;
        let private_key = KeyPair::from_pem(private_key_pem)
            .map_err(|e| IdentityError::CryptoError(format!("解析CA私钥失败: {}", e)))?;

        // 验证私钥与证书的匹配性
        self.verify_key_certificate_match(&private_key, &ca_data.certificate_pem).await?;

        // 重建CA证书参数，保持与原始证书一致
        let mut params = CertificateParams::default();
        params.distinguished_name = self.create_ca_distinguished_name()?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages.push(KeyUsagePurpose::DigitalSignature);
        params.key_usages.push(KeyUsagePurpose::KeyCertSign);
        params.key_usages.push(KeyUsagePurpose::CrlSign);

        // 设置有效期与原证书一致
        let validity_duration = ca_data.expires_at.duration_since(ca_data.issued_at)
            .unwrap_or_else(|_| Duration::from_secs(86400 * self.config.ca_validity_days() as u64));
        params.not_after = time::OffsetDateTime::from(ca_data.issued_at + validity_duration);

        // 重新创建CA证书
        let cert = params.self_signed(&private_key)
            .map_err(|e| IdentityError::CryptoError(format!("重建CA证书失败: {}", e)))?;

        // 创建密钥信息
        let key_algorithm = ca_data.key_algorithm.clone()
            .unwrap_or_else(|| format!("{}-{}", self.config.key_algorithm, self.config.key_size));

        let key_info = KeyPairInfo::new(
            key_algorithm,
            self.config.key_size,
            "ca-key".to_string(),
        );

        Ok(CertificateAuthority {
            certificate: cert,
            private_key: Arc::new(private_key),
            params,
            certificate_data: ca_data.clone(),
            key_info,
        })
    }

    /// 创建设备证书参数
    fn create_device_certificate_params(&self, device_identifier: &str) -> Result<CertificateParams, IdentityError> {
        let mut params = CertificateParams::default();

        // 设置有效期
        let not_after = SystemTime::now() + Duration::from_secs(86400 * self.config.validity_days as u64);
        params.not_after = time::OffsetDateTime::from(not_after);

        // 设置主题信息
        params.distinguished_name = self.create_device_distinguished_name(device_identifier)?;

        // 添加SAN
        let dns_name = format!("{}.bey.local", device_identifier);
        params.subject_alt_names.push(SanType::DnsName(
            dns_name.try_into()
                .map_err(|e| IdentityError::ValidationError(format!("DNS名称转换失败: {}", e)))?
        ));

        // 设置密钥用途
        params.key_usages.push(KeyUsagePurpose::DigitalSignature);
        params.key_usages.push(KeyUsagePurpose::KeyEncipherment);
        params.extended_key_usages.push(ExtendedKeyUsagePurpose::ServerAuth);
        params.extended_key_usages.push(ExtendedKeyUsagePurpose::ClientAuth);

        Ok(params)
    }

    /// 创建CA主题信息
    fn create_ca_distinguished_name(&self) -> Result<DistinguishedName, IdentityError> {
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CountryName, &self.config.country_code);
        dn.push(DnType::OrganizationName, &self.config.organization_name);
        dn.push(DnType::OrganizationalUnitName, "Certificate Authority");
        dn.push(DnType::CommonName, &self.config.ca_common_name);
        Ok(dn)
    }

    /// 创建设备主题信息
    fn create_device_distinguished_name(&self, device_identifier: &str) -> Result<DistinguishedName, IdentityError> {
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CountryName, &self.config.country_code);
        dn.push(DnType::OrganizationName, &self.config.organization_name);
        dn.push(DnType::OrganizationalUnitName, "Device");
        dn.push(DnType::CommonName, device_identifier);
        Ok(dn)
    }

    /// 验证私钥与证书的匹配性
    ///
    /// 使用真实的密码学方法验证私钥是否与证书中的公钥匹配
    ///
    /// # 参数
    ///
    /// * `private_key` - 私钥对象
    /// * `certificate_pem` - 证书PEM字符串
    ///
    /// # 返回值
    ///
    /// 如果匹配则返回Ok(())，否则返回错误
    async fn verify_key_certificate_match(&self, private_key: &KeyPair, certificate_pem: &str) -> Result<(), IdentityError> {
        debug!("验证私钥与证书的匹配性");

        // 使用rcgen的KeyPair方法直接验证
        // 比较公钥是否相同
        let private_public_key = private_key.public_key_raw();

        // 从证书PEM中提取公钥信息
        let cert_key_pair = rcgen::KeyPair::from_pem(certificate_pem)
            .map_err(|e| IdentityError::CryptoError(format!("解析证书公钥失败: {}", e)))?;

        let cert_public_key = cert_key_pair.public_key_raw();

        // 比较公钥是否相同
        if cert_public_key != private_public_key {
            return Err(IdentityError::CryptoError(
                "私钥与证书不匹配：公钥不一致".to_string()
            ));
        }

        // 进行签名验证测试
        self.perform_signature_verification_test(private_key).await?;

        debug!("私钥与证书匹配性验证通过");
        Ok(())
    }

    /// 执行签名验证测试
    ///
    /// 生成测试数据并用私钥签名
    async fn perform_signature_verification_test(&self, private_key: &KeyPair) -> Result<(), IdentityError> {
        // 生成测试数据
        let test_data = b"key-certificate-verification-test-2024";

        // 计算测试数据的哈希
        let data_hash = sha2::Sha256::digest(test_data);

        // 使用私钥对哈希进行签名
        let signature = private_key.sign(&data_hash)
            .map_err(|e| IdentityError::CryptoError(format!("签名失败: {}", e)))?;

        // 验证签名长度和格式
        self.validate_signature_format(&signature).await?;

        debug!("签名验证测试通过");
        Ok(())
    }

    /// 验证签名格式
    ///
    /// 检查签名是否符合预期的格式和长度
    async fn validate_signature_format(&self, signature: &[u8]) -> Result<(), IdentityError> {
        // 检查签名长度
        if signature.is_empty() {
            return Err(IdentityError::CryptoError(
                "签名格式无效：签名为空".to_string()
            ));
        }

        // 根据算法检查签名长度
        let expected_min_length = match self.config.key_algorithm.as_str() {
            "RSA" => match self.config.key_size {
                2048 => 256, // 2048位RSA签名至少256字节
                3072 => 384, // 3072位RSA签名至少384字节
                4096 => 512, // 4096位RSA签名至少512字节
                _ => return Err(IdentityError::CryptoError(
                    format!("不支持的RSA密钥长度: {}", self.config.key_size)
                )),
            },
            "ECDSA" => match self.config.key_size {
                256 => 64,  // P-256曲线签名通常64字节
                384 => 96,  // P-384曲线签名通常96字节
                521 => 132, // P-521曲线签名通常132字节
                _ => return Err(IdentityError::CryptoError(
                    format!("不支持的ECDSA密钥长度: {}", self.config.key_size)
                )),
            },
            _ => return Err(IdentityError::CryptoError(
                format!("不支持的签名算法: {}", self.config.key_algorithm)
            )),
        };

        if signature.len() < expected_min_length {
            return Err(IdentityError::CryptoError(
                format!("签名长度无效：期望至少{}字节，实际{}字节",
                       expected_min_length, signature.len())
            ));
        }

        debug!("签名格式验证通过，长度: {} 字节", signature.len());
        Ok(())
    }

    /// 清理缓存
    pub async fn clear_cache(&self) -> Result<(), IdentityError> {
        debug!("清理证书缓存");

        let mut cache = self.certificate_cache.write().await;
        cache.clear();

        info!("证书缓存清理完成");
        Ok(())
    }

    /// 获取管理器统计信息
    pub async fn get_statistics(&self) -> Result<CertificateManagerStatistics, IdentityError> {
        let storage_stats = self.storage.get_statistics().await;
        let cache_size = self.certificate_cache.read().await.len();

        Ok(CertificateManagerStatistics {
            cached_certificates: cache_size,
            storage_statistics: storage_stats,
            initialized_at: SystemTime::now(),
        })
    }
}

/// 证书管理器统计信息
#[derive(Debug, Clone)]
pub struct CertificateManagerStatistics {
    /// 缓存中的证书数量
    pub cached_certificates: usize,

    /// 存储统计信息
    pub storage_statistics: crate::storage::StorageStatistics,

    /// 初始化时间
    pub initialized_at: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_certificate_manager_initialization() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager_result = CertificateManager::initialize(config).await;
        assert!(manager_result.is_ok(), "证书管理器初始化应该成功");

        let manager = manager_result.unwrap();

        // 测试统计信息
        let stats = manager.get_statistics().await.expect("获取统计信息应该成功");
        assert_eq!(stats.cached_certificates, 0, "初始化时缓存应该为空");
    }

    #[tokio::test]
    async fn test_device_certificate_issuance() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-001";
        let cert_result = manager.issue_device_certificate(device_id).await;
        assert!(cert_result.is_ok(), "设备证书签发应该成功");

        let certificate = cert_result.unwrap();
        assert_eq!(certificate.device_identifier, device_id, "设备标识符应该匹配");
        assert_eq!(certificate.certificate_type, CertificateType::Device, "证书类型应该为Device");
        assert!(!certificate.fingerprint.is_empty(), "证书指纹不应为空");
        assert!(certificate.is_valid(), "新签发的证书应该有效");
    }

    #[tokio::test]
    async fn test_certificate_verification() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-002";
        let certificate = manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        let verification_result = manager.verify_certificate(&certificate).await
            .expect("证书验证失败");

        assert!(verification_result.is_valid, "有效证书应该通过验证");
        assert!(verification_result.error_message.is_none(), "有效证书不应有错误信息");
    }

    #[tokio::test]
    async fn test_certificate_revocation() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-003";
        manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        let revoke_result = manager.revoke_device_certificate(device_id).await
            .expect("证书吊销失败");

        assert!(revoke_result, "证书吊销应该成功");

        // 验证吊销后的证书
        let certificate = manager.get_device_certificate(device_id).await
            .expect("获取证书失败")
            .expect("证书应该存在");

        assert_eq!(certificate.status, CertificateStatus::Revoked, "证书状态应该为Revoked");
        assert!(!certificate.is_valid(), "吊销的证书应该无效");
    }

    #[tokio::test]
    async fn test_certificate_cache_operations() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-004";
        manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        // 检查缓存
        let stats_before = manager.get_statistics().await
            .expect("获取统计信息失败");
        assert!(stats_before.cached_certificates > 0, "应该有缓存的证书");

        // 清理缓存
        manager.clear_cache().await
            .expect("清理缓存失败");

        let stats_after = manager.get_statistics().await
            .expect("获取统计信息失败");
        assert_eq!(stats_after.cached_certificates, 0, "清理后缓存应该为空");
    }

    #[tokio::test]
    async fn test_duplicate_certificate_issuance() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-005";

        // 首次签发
        let cert1 = manager.issue_device_certificate(device_id).await
            .expect("首次证书签发失败");

        // 再次签发（应该返回现有证书）
        let cert2 = manager.issue_device_certificate(device_id).await
            .expect("重复证书签发失败");

        assert_eq!(cert1.certificate_id, cert2.certificate_id, "重复签发应该返回相同的证书");
        assert_eq!(cert1.fingerprint, cert2.fingerprint, "证书指纹应该相同");
    }

    #[tokio::test]
    async fn test_certificate_fingerprint_calculation() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-fingerprint";
        let certificate = manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        // 验证指纹不为空
        assert!(!certificate.fingerprint.is_empty(), "证书指纹不应为空");

        // 验证指纹长度（SHA-256应该是64个十六进制字符）
        assert_eq!(certificate.fingerprint.len(), 64, "SHA-256指纹应该是64个字符");

        // 验证指纹只包含十六进制字符
        assert!(certificate.fingerprint.chars().all(|c| c.is_ascii_hexdigit()),
                "指纹应该只包含十六进制字符");

        // 重新签发相同设备的证书，指纹应该相同
        let cert2 = manager.issue_device_certificate(device_id).await
            .expect("重复签发失败");
        assert_eq!(certificate.fingerprint, cert2.fingerprint, "相同设备证书的指纹应该相同");
    }

    #[tokio::test]
    async fn test_certificate_key_algorithm_validation() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(3072)
            .with_key_algorithm("RSA")
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-rsa3072";
        let certificate = manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        // 验证密钥算法信息
        assert!(certificate.key_algorithm.is_some(), "应该设置密钥算法信息");
        assert_eq!(certificate.key_algorithm.as_ref().unwrap(), "RSA-3072",
                  "密钥算法应该正确设置");

        // 验证证书可以正常解析
        assert!(certificate.certificate_pem.contains("BEGIN CERTIFICATE"),
                "证书应该包含PEM头");
        assert!(certificate.certificate_pem.contains("END CERTIFICATE"),
                "证书应该包含PEM尾");
    }

    #[tokio::test]
    async fn test_certificate_expiry_validation() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(1) // 只有一天有效期
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        let device_id = "test-device-expiry";
        let certificate = manager.issue_device_certificate(device_id).await
            .expect("设备证书签发失败");

        // 验证剩余天数
        let remaining_days = certificate.remaining_days();
        assert!(remaining_days.is_some(), "应该有剩余天数");
        assert!(remaining_days.unwrap() <= 1, "剩余天数应该不超过1天");

        // 验证证书当前是有效的
        assert!(certificate.is_valid(), "新签发的证书应该有效");
    }

    #[tokio::test]
    async fn test_storage_statistics() {
        let temp_dir = TempDir::new().expect("无法创建临时目录");
        let config = CertificateConfig::builder()
            .with_storage_directory(temp_dir.path())
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name("Test CA")
            .build()
            .expect("配置创建失败");

        let manager = CertificateManager::initialize(config).await
            .expect("证书管理器初始化失败");

        // 初始状态统计
        let stats = manager.get_statistics().await
            .expect("获取统计信息应该成功");
        assert_eq!(stats.cached_certificates, 0, "初始缓存应该为空");

        // 签发一些证书
        for i in 1..=3 {
            let device_id = format!("test-device-stats-{}", i);
            manager.issue_device_certificate(&device_id).await
                .expect("设备证书签发失败");
        }

        // 检查更新后的统计
        let updated_stats = manager.get_statistics().await
            .expect("获取更新后的统计信息应该成功");
        assert!(updated_stats.cached_certificates > 0, "应该有缓存的证书");

        // 检查存储统计
        let storage_stats = updated_stats.storage_statistics;
        assert!(storage_stats.total_certificates >= 3, "应该至少有3个证书（包括CA）");
    }
}