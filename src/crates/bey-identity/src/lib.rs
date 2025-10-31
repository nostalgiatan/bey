//! # BEY 身份认证与证书管理模块
//!
//! 提供企业级的 X.509 证书生命周期管理、公钥基础设施（PKI）和设备身份认证功能。
//! 支持完整的证书链管理、安全存储和符合行业标准的加密实现。
//!
//! ## 架构设计
//!
//! 本模块采用分层架构设计：
//! - **配置层**: 证书策略和参数配置
//! - **数据层**: 证书数据的序列化和持久化
//! - **服务层**: 证书生成、验证、吊销等核心服务
//! - **存储层**: 文件系统和内存缓存管理
//!
//! ## 核心功能
//!
//! - **证书颁发机构（CA）管理**: 自建根CA，支持证书签发和吊销
//! - **设备证书管理**: 自动生成设备身份证书，支持密钥轮换
//! - **证书验证链**: 完整的X.509证书路径验证
//! - **安全存储**: 证书和私钥的加密存储和访问控制
//! - **证书吊销列表（CRL）**: 支持证书状态查询和批量吊销
//! - **密钥管理**: 支持RSA和ECDSA密钥算法，安全的密钥生成
//!
//! ## 安全特性
//!
//! - 使用经过安全验证的加密库（rustls, rcgen）
//! - 所有私钥操作都在内存中进行，不写入日志
//! - 证书文件权限严格控制（跨平台兼容）
//! - 支持密钥长度和算法策略配置
//!
//! ## 使用示例
//!
//! ```rust
//! use bey_identity::certificate::{CertificateManager, CertificateConfig};
//! use bey_identity::types::CertificateType;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 配置证书策略
//! let config = CertificateConfig::builder()
//!     .with_validity_days(365)
//!     .with_key_size(2048)
//!     .with_ca_common_name("BEY Production CA")
//!     .build()?;
//!
//! // 初始化证书管理器
//! let manager = CertificateManager::initialize(config).await?;
//!
//! // 为设备生成身份证书
//! let device_cert = manager.issue_device_certificate("device-001").await?;
//! println!("设备证书指纹: {}", device_cert.fingerprint);
//!
//! // 验证证书有效性
//! let verification_result = manager.verify_certificate(&device_cert).await?;
//! println!("证书验证结果: {}", verification_result.is_valid);
//! # Ok(())
//! # }
//! ```

// 导入错误模块
use ::error::ErrorInfo;

pub mod certificate;
pub mod types;
pub mod storage;
pub mod validation;
pub mod config;
pub mod error;

pub use certificate::{CertificateManager, CertificateAuthority, CertificateManagerStatistics};
pub use types::{CertificateData, CertificateType, CertificateStatus, CertificateVerificationResult, KeyPairInfo};
pub use storage::{CertificateStorage, StorageConfig, StorageStatistics};
pub use validation::{CertificateValidator, ValidatorStatistics};
pub use config::{CertificateConfig, CertificatePolicy};
pub use error::{IdentityError, ConfigError};

/// 证书管理统一结果类型
pub type IdentityResult<T> = std::result::Result<T, ErrorInfo>;