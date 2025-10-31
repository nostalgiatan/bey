//! # 证书管理错误定义
//!
//! 定义证书管理模块专用的错误类型，提供详细的错误分类和处理。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::fmt;

/// 证书管理专用错误类型
///
/// 提供更具体的证书管理相关错误分类。
#[derive(Debug, Clone)]
pub enum IdentityError {
    /// 配置相关错误
    Config(ConfigError),

    /// 加密相关错误
    CryptoError(String),

    /// 存储相关错误
    StorageError(String),

    /// 验证相关错误
    ValidationError(String),

    /// 证书状态错误
    CertificateStatusError(String),

    /// IO相关错误
    IoError(String),

    /// 网络相关错误
    NetworkError(String),

    /// 时间相关错误
    TimeError(String),

    /// 权限相关错误
    PermissionError(String),

    /// 未知错误
    Unknown(String),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityError::Config(err) => write!(f, "配置错误: {}", err),
            IdentityError::CryptoError(msg) => write!(f, "加密错误: {}", msg),
            IdentityError::StorageError(msg) => write!(f, "存储错误: {}", msg),
            IdentityError::ValidationError(msg) => write!(f, "验证错误: {}", msg),
            IdentityError::CertificateStatusError(msg) => write!(f, "证书状态错误: {}", msg),
            IdentityError::IoError(msg) => write!(f, "IO错误: {}", msg),
            IdentityError::NetworkError(msg) => write!(f, "网络错误: {}", msg),
            IdentityError::TimeError(msg) => write!(f, "时间错误: {}", msg),
            IdentityError::PermissionError(msg) => write!(f, "权限错误: {}", msg),
            IdentityError::Unknown(msg) => write!(f, "未知错误: {}", msg),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<ConfigError> for IdentityError {
    fn from(err: ConfigError) -> Self {
        IdentityError::Config(err)
    }
}

impl From<std::io::Error> for IdentityError {
    fn from(err: std::io::Error) -> Self {
        IdentityError::IoError(err.to_string())
    }
}

/// 配置错误详细类型
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

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidKeySize(size) => write!(f, "无效的密钥长度: {}", size),
            ConfigError::InvalidValidityPeriod(days) => write!(f, "无效的有效期: {} 天", days),
            ConfigError::InvalidStoragePath(path) => write!(f, "无效的存储路径: {}", path),
            ConfigError::ValidationFailed(msg) => write!(f, "配置验证失败: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<IdentityError> for ErrorInfo {
    fn from(err: IdentityError) -> Self {
        let (code, category, severity) = match &err {
            IdentityError::Config(_) => (6001, ErrorCategory::Configuration, ErrorSeverity::Error),
            IdentityError::CryptoError(_) => (6002, ErrorCategory::System, ErrorSeverity::Critical),
            IdentityError::StorageError(_) => (6003, ErrorCategory::System, ErrorSeverity::Error),
            IdentityError::ValidationError(_) => (6004, ErrorCategory::Validation, ErrorSeverity::Warning),
            IdentityError::CertificateStatusError(_) => (6005, ErrorCategory::Permission, ErrorSeverity::Error),
            IdentityError::IoError(_) => (6006, ErrorCategory::Io, ErrorSeverity::Error),
            IdentityError::NetworkError(_) => (6007, ErrorCategory::Network, ErrorSeverity::Error),
            IdentityError::TimeError(_) => (6008, ErrorCategory::System, ErrorSeverity::Error),
            IdentityError::PermissionError(_) => (6009, ErrorCategory::Permission, ErrorSeverity::Error),
            IdentityError::Unknown(_) => (6100, ErrorCategory::Other, ErrorSeverity::Error),
        };

        ErrorInfo::new(code, err.to_string())
            .with_category(category)
            .with_severity(severity)
    }
}

impl From<ErrorInfo> for IdentityError {
    fn from(err: ErrorInfo) -> Self {
        IdentityError::Unknown(format!("{}", err))
    }
}