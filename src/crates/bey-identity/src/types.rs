//! # 证书数据类型定义
//!
//! 定义证书管理系统中使用的所有数据结构和枚举类型。
//! 包含证书信息、配置参数、状态枚举等核心类型。

use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use base64::Engine;

/// 证书类型枚举
///
/// 定义系统中支持的各类证书类型，用于区分不同的证书用途和管理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateType {
    /// 根证书颁发机构证书
    /// 具有最高信任级别，用于签发其他CA证书
    RootCA,

    /// 中间证书颁发机构证书
    /// 由根CA签发，用于签发终端实体证书
    IntermediateCA,

    /// 设备身份证书
    /// 用于设备认证和标识
    Device,

    /// 客户端认证证书
    /// 用于客户端身份验证
    Client,

    /// 服务器认证证书
    /// 用于服务器身份验证和TLS连接
    Server,

    /// 代码签名证书
    /// 用于软件代码签名验证
    CodeSigning,

    /// 时间戳证书
    /// 用于时间戳服务
    Timestamp,
}

impl CertificateType {
    /// 获取证书类型的字符串描述
    pub fn description(&self) -> &'static str {
        match self {
            CertificateType::RootCA => "根证书颁发机构",
            CertificateType::IntermediateCA => "中间证书颁发机构",
            CertificateType::Device => "设备身份证书",
            CertificateType::Client => "客户端认证证书",
            CertificateType::Server => "服务器认证证书",
            CertificateType::CodeSigning => "代码签名证书",
            CertificateType::Timestamp => "时间戳证书",
        }
    }

    /// 判断是否为CA类型证书
    pub fn is_ca(&self) -> bool {
        matches!(self, CertificateType::RootCA | CertificateType::IntermediateCA)
    }

    /// 判断是否为终端实体证书
    pub fn is_end_entity(&self) -> bool {
        !self.is_ca()
    }
}

/// 证书状态枚举
///
/// 表示证书在生命周期中的当前状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateStatus {
    /// 有效状态
    /// 证书在有效期内且未被吊销
    Valid,

    /// 已过期
    /// 证书已超过其有效期
    Expired,

    /// 已吊销
    /// 证书已被主动吊销
    Revoked,

    /// 暂停使用
    /// 证书临时暂停使用
    Suspended,

    /// 待激活
    /// 证书已生成但尚未激活
    Pending,

    /// 未知状态
    /// 无法确定证书状态
    Unknown,
}

impl CertificateStatus {
    /// 获取状态的字符串描述
    pub fn description(&self) -> &'static str {
        match self {
            CertificateStatus::Valid => "有效",
            CertificateStatus::Expired => "已过期",
            CertificateStatus::Revoked => "已吊销",
            CertificateStatus::Suspended => "暂停使用",
            CertificateStatus::Pending => "待激活",
            CertificateStatus::Unknown => "未知状态",
        }
    }

    /// 判断证书是否可用
    pub fn is_usable(&self) -> bool {
        matches!(self, CertificateStatus::Valid)
    }
}

/// 证书数据结构
///
/// 包含证书的所有相关信息，包括证书内容、元数据和状态信息。
/// 这是证书管理的核心数据结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateData {
    /// 证书唯一标识符
    /// 系统内部使用，用于证书索引和管理
    pub certificate_id: String,

    /// 关联的设备标识符
    /// 用于设备证书管理和查找
    pub device_identifier: String,

    /// 证书SHA-256指纹
    /// 用于证书完整性验证和快速识别
    pub fingerprint: String,

    /// 证书内容（PEM格式）
    /// X.509证书的PEM编码内容
    pub certificate_pem: String,

    /// 私钥内容（PEM格式）
    /// 对应的私钥PEM编码内容（如果适用）
    pub private_key_pem: Option<String>,

    /// 证书颁发时间
    pub issued_at: SystemTime,

    /// 证书过期时间
    pub expires_at: SystemTime,

    /// 证书类型
    pub certificate_type: CertificateType,

    /// 证书颁发者标识
    /// 颁发此证书的CA标识
    pub issuer_identifier: String,

    /// 证书主题标识
    /// 证书主体的标识信息
    pub subject_identifier: String,

    /// 证书序列号
    /// 证书的序列号，用于吊销列表
    pub serial_number: Option<String>,

    /// 证书当前状态
    pub status: CertificateStatus,

    /// 密钥算法
    /// 如 "RSA-2048", "ECDSA-P256" 等
    pub key_algorithm: Option<String>,

    /// 证书版本
    /// X.509证书版本号
    pub version: Option<u32>,
}

impl CertificateData {
    /// 创建新的证书数据实例
    pub fn new(
        certificate_id: String,
        device_identifier: String,
        certificate_pem: String,
        private_key_pem: Option<String>,
        certificate_type: CertificateType,
        issuer_identifier: String,
        subject_identifier: String,
    ) -> Self {
        Self {
            certificate_id,
            device_identifier,
            fingerprint: String::new(), // 将在后续计算
            certificate_pem,
            private_key_pem,
            issued_at: SystemTime::now(),
            expires_at: SystemTime::now(), // 将在后续设置
            certificate_type,
            issuer_identifier,
            subject_identifier,
            serial_number: None,
            status: CertificateStatus::Pending,
            key_algorithm: None,
            version: Some(3), // X.509 v3
        }
    }

    /// 计算并设置证书指纹
    pub fn calculate_fingerprint(&mut self) -> Result<(), crate::IdentityError> {
        use sha2::{Sha256, Digest};

        // 移除PEM头尾和换行符
        let der_data = self.certificate_pem
            .lines()
            .skip(1)
            .take_while(|line| !line.starts_with("-"))
            .collect::<String>();

        let decoded = base64::engine::general_purpose::STANDARD.decode(&der_data)
            .map_err(|e| crate::IdentityError::CryptoError(format!("解码证书失败: {}", e)))?;

        let mut hasher = Sha256::new();
        hasher.update(&decoded);
        let result = hasher.finalize();

        self.fingerprint = format!("{:x}", result);
        Ok(())
    }

    /// 检查证书是否过期
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// 检查证书是否有效
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && self.status == CertificateStatus::Valid
    }

    /// 获取证书剩余有效天数
    pub fn remaining_days(&self) -> Option<u64> {
        if self.is_expired() {
            None
        } else {
            let remaining = self.expires_at
                .duration_since(SystemTime::now())
                .ok()?;
            Some(remaining.as_secs() / 86400)
        }
    }

    /// 设置证书状态
    pub fn set_status(&mut self, status: CertificateStatus) {
        self.status = status;
    }

    /// 获取证书简称显示名称
    pub fn display_name(&self) -> String {
        match self.certificate_type {
            CertificateType::Device => format!("设备证书({})", self.device_identifier),
            CertificateType::RootCA => "根CA证书".to_string(),
            CertificateType::IntermediateCA => format!("中间CA证书({})", self.subject_identifier),
            _ => format!("{}证书({})", self.certificate_type.description(), self.subject_identifier),
        }
    }
}

/// 证书验证结果
///
/// 包含证书验证的详细结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateVerificationResult {
    /// 验证是否通过
    pub is_valid: bool,

    /// 验证错误信息（如果有）
    pub error_message: Option<String>,

    /// 验证路径
    /// 从终端证书到根CA的证书链
    pub verification_path: Vec<String>,

    /// 验证时间戳
    pub verified_at: SystemTime,

    /// 证书状态检查结果
    pub status_check: CertificateStatus,
}

impl CertificateVerificationResult {
    /// 创建成功的验证结果
    pub fn success(verification_path: Vec<String>) -> Self {
        Self {
            is_valid: true,
            error_message: None,
            verification_path,
            verified_at: SystemTime::now(),
            status_check: CertificateStatus::Valid,
        }
    }

    /// 创建失败的验证结果
    pub fn failure(error_message: String) -> Self {
        Self {
            is_valid: false,
            error_message: Some(error_message),
            verification_path: Vec::new(),
            verified_at: SystemTime::now(),
            status_check: CertificateStatus::Unknown,
        }
    }
}

/// 密钥对信息
///
/// 存储密钥对的相关信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPairInfo {
    /// 密钥算法
    pub algorithm: String,

    /// 密钥长度（位）
    pub key_size: u32,

    /// 密钥生成时间
    pub generated_at: SystemTime,

    /// 密钥标识符
    pub key_id: String,
}

impl KeyPairInfo {
    /// 创建新的密钥对信息
    pub fn new(algorithm: String, key_size: u32, key_id: String) -> Self {
        Self {
            algorithm,
            key_size,
            generated_at: SystemTime::now(),
            key_id,
        }
    }

    /// 获取密钥类型描述
    pub fn key_type_description(&self) -> String {
        format!("{}-{}", self.algorithm, self.key_size)
    }
}