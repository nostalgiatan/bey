//! # 证书配置管理
//!
//! 提供证书管理系统的配置功能，包括证书策略、安全参数、存储配置等。
//! 支持构建器模式的配置创建和验证。

use error::ErrorInfo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 证书管理配置错误类型
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// 无效的密钥长度
    InvalidKeySize(u32),

    /// 无效的有效期
    InvalidValidityPeriod(u32),

    /// 无效的存储路径
    InvalidStoragePath(String),

    /// 配置验证失败
    ValidationFailed(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidKeySize(size) => write!(f, "无效的密钥长度: {}", size),
            ConfigError::InvalidValidityPeriod(days) => write!(f, "无效的有效期: {} 天", days),
            ConfigError::InvalidStoragePath(path) => write!(f, "无效的存储路径: {}", path),
            ConfigError::ValidationFailed(msg) => write!(f, "配置验证失败: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

/// 证书配置构建器
///
/// 使用构建器模式创建和验证证书配置，确保配置的一致性和正确性。
#[derive(Debug, Clone)]
pub struct CertificateConfigBuilder {
    validity_days: u32,
    key_size: u32,
    key_algorithm: String,
    storage_directory: PathBuf,
    ca_common_name: String,
    organization_name: String,
    country_code: String,
    enable_crl: bool,
    crl_update_interval: Duration,
    max_certificate_chain_length: u8,
    enforce_strict_validation: bool,
}

impl Default for CertificateConfigBuilder {
    fn default() -> Self {
        Self {
            validity_days: 365,
            key_size: 256,
            key_algorithm: "ECDSA".to_string(),
            storage_directory: PathBuf::from("./certificates"),
            ca_common_name: "BEY Internal CA".to_string(),
            organization_name: "BEY".to_string(),
            country_code: "CN".to_string(),
            enable_crl: true,
            crl_update_interval: Duration::from_secs(86400), // 1天
            max_certificate_chain_length: 5,
            enforce_strict_validation: true,
        }
    }
}

impl CertificateConfigBuilder {
    /// 创建新的配置构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置证书有效期（天）
    pub fn with_validity_days(mut self, days: u32) -> Self {
        self.validity_days = days;
        self
    }

    /// 设置密钥长度（位）
    pub fn with_key_size(mut self, size: u32) -> Self {
        self.key_size = size;
        self
    }

    /// 设置密钥算法
    pub fn with_key_algorithm(mut self, algorithm: impl Into<String>) -> Self {
        self.key_algorithm = algorithm.into();
        self
    }

    /// 设置证书存储目录
    pub fn with_storage_directory<P: AsRef<Path>>(mut self, directory: P) -> Self {
        self.storage_directory = directory.as_ref().to_path_buf();
        self
    }

    /// 设置CA通用名称
    pub fn with_ca_common_name(mut self, name: impl Into<String>) -> Self {
        self.ca_common_name = name.into();
        self
    }

    /// 设置组织名称
    pub fn with_organization_name(mut self, name: impl Into<String>) -> Self {
        self.organization_name = name.into();
        self
    }

    /// 设置国家代码
    pub fn with_country_code(mut self, code: impl Into<String>) -> Self {
        self.country_code = code.into();
        self
    }

    /// 启用或禁用CRL支持
    pub fn with_crl_support(mut self, enabled: bool) -> Self {
        self.enable_crl = enabled;
        self
    }

    /// 设置CRL更新间隔
    pub fn with_crl_update_interval(mut self, interval: Duration) -> Self {
        self.crl_update_interval = interval;
        self
    }

    /// 设置最大证书链长度
    pub fn with_max_chain_length(mut self, length: u8) -> Self {
        self.max_certificate_chain_length = length;
        self
    }

    /// 启用或禁用严格验证
    pub fn with_strict_validation(mut self, enabled: bool) -> Self {
        self.enforce_strict_validation = enabled;
        self
    }

    /// 构建并验证配置
    pub fn build(self) -> Result<CertificateConfig, ConfigError> {
        self.validate()?;
        Ok(CertificateConfig {
            validity_days: self.validity_days,
            key_size: self.key_size,
            key_algorithm: self.key_algorithm,
            storage_directory: self.storage_directory,
            ca_common_name: self.ca_common_name,
            organization_name: self.organization_name,
            country_code: self.country_code,
            enable_crl: self.enable_crl,
            crl_update_interval: self.crl_update_interval,
            max_certificate_chain_length: self.max_certificate_chain_length,
            enforce_strict_validation: self.enforce_strict_validation,
        })
    }

    /// 验证配置参数
    fn validate(&self) -> Result<(), ConfigError> {
        // 验证密钥长度
        if !self.is_valid_key_size(self.key_size) {
            return Err(ConfigError::InvalidKeySize(self.key_size));
        }

        // 验证有效期
        if self.validity_days < 1 || self.validity_days > 3650 {
            return Err(ConfigError::InvalidValidityPeriod(self.validity_days));
        }

        // 验证国家代码（2个字母）
        if self.country_code.len() != 2 || !self.country_code.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ConfigError::ValidationFailed("国家代码必须是2个字母".to_string()));
        }

        // 验证存储路径
        if self.storage_directory.as_os_str().is_empty() {
            return Err(ConfigError::InvalidStoragePath("存储路径不能为空".to_string()));
        }

        // 验证证书链长度
        if self.max_certificate_chain_length == 0 || self.max_certificate_chain_length > 10 {
            return Err(ConfigError::ValidationFailed("证书链长度必须在1-10之间".to_string()));
        }

        Ok(())
    }

    /// 检查密钥长度是否有效
    fn is_valid_key_size(&self, size: u32) -> bool {
        match self.key_algorithm.as_str() {
            "RSA" => matches!(size, 1024 | 2048 | 3072 | 4096),
            "ECDSA" => matches!(size, 256 | 384 | 521),
            "EdDSA" => matches!(size, 255 | 448),
            _ => false,
        }
    }
}

/// 证书管理配置
///
/// 包含证书管理系统的所有配置参数，经过验证的安全配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateConfig {
    /// 证书有效期（天）
    pub validity_days: u32,

    /// 密钥长度（位）
    pub key_size: u32,

    /// 密钥算法
    pub key_algorithm: String,

    /// 证书存储目录
    pub storage_directory: PathBuf,

    /// CA证书通用名称
    pub ca_common_name: String,

    /// 组织名称
    pub organization_name: String,

    /// 国家代码
    pub country_code: String,

    /// 是否启用CRL支持
    pub enable_crl: bool,

    /// CRL更新间隔
    pub crl_update_interval: Duration,

    /// 最大证书链长度
    pub max_certificate_chain_length: u8,

    /// 是否启用严格验证
    pub enforce_strict_validation: bool,
}

impl CertificateConfig {
    /// 创建配置构建器
    pub fn builder() -> CertificateConfigBuilder {
        CertificateConfigBuilder::new()
    }

    /// 创建默认配置
    pub fn default() -> Self {
        CertificateConfigBuilder::new()
            .build()
            .expect("默认配置应该有效")
    }

    /// 获取CA证书有效期（通常比设备证书长）
    pub fn ca_validity_days(&self) -> u32 {
        self.validity_days * 10 // CA证书有效期是设备证书的10倍
    }

    /// 检查是否为生产环境配置
    pub fn is_production_config(&self) -> bool {
        self.key_size >= 2048 && self.enforce_strict_validation
    }

    /// 获取证书文件路径
    pub fn certificate_file_path(&self, certificate_id: &str) -> PathBuf {
        self.storage_directory.join(format!("{}.crt", certificate_id))
    }

    /// 获取私钥文件路径
    pub fn private_key_file_path(&self, certificate_id: &str) -> PathBuf {
        self.storage_directory.join(format!("{}.key", certificate_id))
    }

    /// 获取CRL文件路径
    pub fn crl_file_path(&self) -> PathBuf {
        self.storage_directory.join("ca.crl")
    }

    /// 获取CA证书文件路径
    pub fn ca_certificate_path(&self) -> PathBuf {
        self.storage_directory.join("ca.crt")
    }

    /// 获取CA私钥文件路径
    pub fn ca_private_key_path(&self) -> PathBuf {
        self.storage_directory.join("ca.key")
    }

    /// 创建存储目录（如果不存在）
    pub fn ensure_storage_directory(&self) -> Result<(), ErrorInfo> {
        std::fs::create_dir_all(&self.storage_directory)
            .map_err(|e| ErrorInfo::new(5001, format!("创建证书存储目录失败: {}", e)))
    }

    /// 验证存储目录权限
    pub fn verify_storage_permissions(&self) -> Result<bool, ErrorInfo> {
        if !self.storage_directory.exists() {
            return Ok(false);
        }

        // 检查目录权限（跨平台兼容的检查）
        let _metadata = std::fs::metadata(&self.storage_directory)
            .map_err(|e| ErrorInfo::new(5002, format!("读取存储目录元数据失败: {}", e)))?;

        // 检查是否可读可写（跨平台方式）
        let test_file = self.storage_directory.join(".permission_test");

        // 尝试创建测试文件来验证写权限
        match std::fs::write(&test_file, "test") {
            Ok(_) => {
                // 删除测试文件
                let _ = std::fs::remove_file(&test_file);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// 生成配置摘要
    pub fn generate_config_summary(&self) -> String {
        format!(
            "证书配置摘要:\n\
            - 算法: {}-{}\n\
            - 有效期: {} 天\n\
            - 存储目录: {}\n\
            - CA名称: {}\n\
            - 组织: {}\n\
            - CRL支持: {}\n\
            - 严格验证: {}",
            self.key_algorithm,
            self.key_size,
            self.validity_days,
            self.storage_directory.display(),
            self.ca_common_name,
            self.organization_name,
            if self.enable_crl { "启用" } else { "禁用" },
            if self.enforce_strict_validation { "启用" } else { "禁用" }
        )
    }
}

/// 证书策略配置
///
/// 定义证书签发和管理的策略规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificatePolicy {
    /// 最小密钥长度
    pub min_key_size: u32,

    /// 最大证书有效期
    pub max_validity_days: u32,

    /// 允许的密钥算法
    pub allowed_algorithms: Vec<String>,

    /// 是否允许密钥重用
    pub allow_key_reuse: bool,

    /// 证书续订策略
    pub renewal_policy: RenewalPolicy,
}

/// 证书续订策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenewalPolicy {
    /// 自动续订
    Automatic,

    /// 手动续订
    Manual,

    /// 在过期前指定天数自动续订
    BeforeExpiry(u32),
}

impl Default for CertificatePolicy {
    fn default() -> Self {
        Self {
            min_key_size: 2048,
            max_validity_days: 825, // 约2.25年，符合行业最佳实践
            allowed_algorithms: vec!["RSA-2048".to_string(), "RSA-3072".to_string(), "RSA-4096".to_string()],
            allow_key_reuse: false,
            renewal_policy: RenewalPolicy::BeforeExpiry(30),
        }
    }
}