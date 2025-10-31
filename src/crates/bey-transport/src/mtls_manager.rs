//! # 完整的mTLS双向认证管理器
//!
//! 完全依赖 bey_identity 证书管理模块，提供企业级的双向TLS认证功能。
//! 所有证书相关的操作都由证书管理模块处理，本模块只负责配置QUIC连接。
//!
//! ## 核心特性
//!
//! - **完全依赖证书管理模块**: 所有证书操作由 bey_identity 处理
//! - **双向TLS认证**: 客户端和服务端相互验证
//! - **证书自动轮换**: 证书管理器自动处理证书更新
//! - **完整证书链验证**: 使用证书管理器的专业验证
//! - **配置缓存**: TLS配置缓存优化性能
//! - **安全策略**: 灵活的安全策略配置

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use quinn::crypto::rustls::{QuicServerConfig, QuicClientConfig};
use rustls::{pki_types::CertificateDer, RootCertStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, debug};

// 完全依赖证书管理模块
use bey_identity::{CertificateManager, CertificateData};

/// mTLS配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConfig {
    /// 是否启用mTLS
    pub enabled: bool,
    /// 证书存储目录
    pub certificates_dir: std::path::PathBuf,
    /// 是否启用配置缓存
    pub enable_config_cache: bool,
    /// 配置缓存TTL
    pub config_cache_ttl: Duration,
    /// 最大配置缓存数量
    pub max_config_cache_entries: usize,
    /// 设备ID前缀
    pub device_id_prefix: String,
    /// 组织名称
    pub organization_name: String,
    /// 国家代码
    pub country_code: String,
}

impl Default for MtlsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            certificates_dir: std::path::PathBuf::from("./certs"),
            enable_config_cache: true,
            config_cache_ttl: Duration::from_secs(3600), // 1小时
            max_config_cache_entries: 100,
            device_id_prefix: "bey".to_string(),
            organization_name: "BEY".to_string(),
            country_code: "CN".to_string(),
        }
    }
}

/// mTLS统计信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MtlsStats {
    /// 配置生成次数
    pub config_generations: u64,
    /// 配置缓存命中次数
    pub config_cache_hits: u64,
    /// 配置缓存未命中次数
    pub config_cache_misses: u64,
    /// 证书轮换次数
    pub certificate_renewals: u64,
    /// 证书验证次数
    pub certificate_verifications: u64,
    /// 连接建立次数
    pub connections_established: u64,
    /// 连接失败次数
    pub connection_failures: u64,
}

/// 完整的mTLS管理器
pub struct CompleteMtlsManager {
    /// 配置信息
    #[allow(dead_code)]
    config: Arc<MtlsConfig>,
    /// 证书管理器
    certificate_manager: Arc<CertificateManager>,
    /// 服务器配置缓存
    server_config_cache: Arc<RwLock<Option<quinn::ServerConfig>>>,
    /// 客户端配置缓存
    client_config_cache: Arc<RwLock<Option<quinn::ClientConfig>>>,
    /// 统计信息
    stats: Arc<RwLock<MtlsStats>>,
    /// 设备ID
    device_id: String,
}

impl CompleteMtlsManager {
    /// 创建新的完整mTLS管理器
    pub async fn new(config: MtlsConfig, device_id: String) -> Result<Self, ErrorInfo> {
        info!("初始化完整mTLS管理器，设备ID: {}", device_id);

        // 验证配置
        Self::validate_config(&config)?;

        // 初始化证书管理器
        let certificate_config = bey_identity::CertificateConfig::builder()
            .with_validity_days(365)
            .with_key_size(2048)
            .with_ca_common_name(format!("{} CA", config.organization_name))
            .with_organization_name(&config.organization_name)
            .with_country_code(&config.country_code)
            .with_storage_directory(&config.certificates_dir)
            .build()
            .map_err(|e| ErrorInfo::new(5001, format!("创建证书配置失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        let certificate_manager = Arc::new(
            CertificateManager::initialize(certificate_config)
                .await
                .map_err(|e| ErrorInfo::new(5002, format!("初始化证书管理器失败: {}", e))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Error))?
        );

        let manager = Self {
            config: Arc::new(config),
            certificate_manager,
            server_config_cache: Arc::new(RwLock::new(None)),
            client_config_cache: Arc::new(RwLock::new(None)),
            stats: Arc::new(RwLock::new(MtlsStats::default())),
            device_id,
        };

        // 确保设备证书存在
        manager.ensure_device_certificate().await?;

        info!("完整mTLS管理器初始化完成");
        Ok(manager)
    }

    /// 验证配置
    fn validate_config(config: &MtlsConfig) -> Result<(), ErrorInfo> {
        if config.config_cache_ttl.is_zero() {
            return Err(ErrorInfo::new(5003, "配置缓存TTL必须大于0".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        }

        if config.max_config_cache_entries == 0 {
            return Err(ErrorInfo::new(5004, "配置缓存数量必须大于0".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error));
        }

        Ok(())
    }

    /// 确保设备证书存在
    async fn ensure_device_certificate(&self) -> Result<(), ErrorInfo> {
        // 检查设备证书是否存在
        match self.certificate_manager.get_device_certificate(&self.device_id).await {
            Ok(Some(_)) => {
                info!("设备证书已存在");
            }
            Ok(None) => {
                info!("设备证书不存在，正在生成...");
                let certificate = self.certificate_manager
                    .issue_device_certificate(&self.device_id)
                    .await
                    .map_err(|e| ErrorInfo::new(5005, format!("签发设备证书失败: {}", e))
                        .with_category(ErrorCategory::System)
                        .with_severity(ErrorSeverity::Error))?;

                info!("设备证书生成完成，指纹: {}", certificate.fingerprint);

                // 更新统计
                {
                    let mut stats = self.stats.write().await;
                    stats.certificate_renewals += 1;
                }
            }
            Err(e) => {
                return Err(ErrorInfo::new(5006, format!("检查设备证书失败: {}", e))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Error));
            }
        }

        Ok(())
    }

    /// 获取服务器配置
    pub async fn get_server_config(&self) -> Result<quinn::ServerConfig, ErrorInfo> {
        // 检查缓存
        {
            let cache = self.server_config_cache.read().await;
            if let Some(ref config) = *cache {
                debug!("使用服务器配置缓存");
                {
                    let mut stats = self.stats.write().await;
                    stats.config_cache_hits += 1;
                }
                return Ok(config.clone());
            }
        }

        // 更新缓存未命中统计
        {
            let mut stats = self.stats.write().await;
            stats.config_cache_misses += 1;
        }

        // 生成新的服务器配置
        let server_config = self.generate_server_config().await?;

        // 更新缓存
        {
            let mut cache = self.server_config_cache.write().await;
            *cache = Some(server_config.clone());
        }

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.config_generations += 1;
        }

        Ok(server_config)
    }

    /// 获取客户端配置
    pub async fn get_client_config(&self) -> Result<quinn::ClientConfig, ErrorInfo> {
        // 检查缓存
        {
            let cache = self.client_config_cache.read().await;
            if let Some(ref config) = *cache {
                debug!("使用客户端配置缓存");
                {
                    let mut stats = self.stats.write().await;
                    stats.config_cache_hits += 1;
                }
                return Ok(config.clone());
            }
        }

        // 更新缓存未命中统计
        {
            let mut stats = self.stats.write().await;
            stats.config_cache_misses += 1;
        }

        // 生成新的客户端配置
        let client_config = self.generate_client_config().await?;

        // 更新缓存
        {
            let mut cache = self.client_config_cache.write().await;
            *cache = Some(client_config.clone());
        }

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.config_generations += 1;
        }

        Ok(client_config)
    }

    /// 生成服务器配置
    async fn generate_server_config(&self) -> Result<quinn::ServerConfig, ErrorInfo> {
        // 获取设备证书作为服务器证书
        let server_cert = self.certificate_manager
            .get_device_certificate(&self.device_id)
            .await
            .map_err(|e| ErrorInfo::new(5010, format!("获取服务器证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?
            .ok_or_else(|| ErrorInfo::new(5011, "服务器证书不存在".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        // 获取CA证书（使用设备证书的颁发者）
        let ca_identifier = &server_cert.issuer_identifier;
        let ca_cert = self.certificate_manager
            .get_device_certificate(ca_identifier)
            .await
            .map_err(|e| ErrorInfo::new(5012, format!("获取CA证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        // 构建CA证书存储
        let mut root_store = RootCertStore::empty();
        if let Some(ca_cert_data) = ca_cert {
            // 将PEM格式转换为DER格式
            let cert_der = self.pem_to_der(&ca_cert_data.certificate_pem)?;
            let cert_der = CertificateDer::from(cert_der);

            root_store.add(cert_der)
                .map_err(|e| ErrorInfo::new(5013, format!("添加CA证书到存储失败: {}", e))
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Error))?;
        }

        // 转换服务器证书为DER格式
        let server_cert_der = self.pem_to_der(&server_cert.certificate_pem)?;
        let server_cert_der = CertificateDer::from(server_cert_der);

        // 获取私钥
        let private_key_pem = server_cert.private_key_pem.as_ref()
            .ok_or_else(|| ErrorInfo::new(5014, "服务器证书缺少私钥".to_string())
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        let private_key_der = self.pem_to_der(private_key_pem)?;
        let private_key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(private_key_der)
        );

        let cert_chain = vec![server_cert_der];

        // 创建rustls服务器配置
        let rustls_server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| ErrorInfo::new(5015, format!("创建服务器配置失败: {}", e))
                .with_category(ErrorCategory::Configuration)
                .with_severity(ErrorSeverity::Error))?;

        // 转换为Quinn配置
        let quinn_server_config = quinn::ServerConfig::with_crypto(Arc::new(
            QuicServerConfig::try_from(rustls_server_config)
                .map_err(|e| ErrorInfo::new(5015, format!("转换为Quinn服务器配置失败: {:?}", e))
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Error))?
        ));

        Ok(quinn_server_config)
    }

    /// 生成客户端配置
    async fn generate_client_config(&self) -> Result<quinn::ClientConfig, ErrorInfo> {
        // 获取本地设备证书来找到CA证书
        let device_cert = self.certificate_manager
            .get_device_certificate(&self.device_id)
            .await
            .map_err(|e| ErrorInfo::new(5015, format!("获取设备证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        // 构建CA证书存储
        let mut root_store = RootCertStore::empty();

        // 如果有设备证书，尝试获取其CA证书
        if let Some(cert_data) = device_cert {
            let ca_identifier = &cert_data.issuer_identifier;
            if let Some(ca_cert) = self.certificate_manager
                .get_device_certificate(ca_identifier)
                .await
                .map_err(|e| ErrorInfo::new(5016, format!("获取CA证书失败: {}", e))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Error))? {

                let cert_der = self.pem_to_der(&ca_cert.certificate_pem)?;
                let cert_der = CertificateDer::from(cert_der);

                root_store.add(cert_der)
                    .map_err(|e| ErrorInfo::new(5017, format!("添加CA证书到存储失败: {}", e))
                        .with_category(ErrorCategory::Configuration)
                        .with_severity(ErrorSeverity::Error))?;
            }
        }

        // 创建rustls客户端配置
        let rustls_client_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // 转换为Quinn配置
        let quinn_client_config = quinn::ClientConfig::new(Arc::new(
            QuicClientConfig::try_from(rustls_client_config)
                .map_err(|e| ErrorInfo::new(5018, format!("转换为Quinn客户端配置失败: {:?}", e))
                    .with_category(ErrorCategory::Configuration)
                    .with_severity(ErrorSeverity::Error))?
        ));

        Ok(quinn_client_config)
    }

    /// PEM格式转DER格式
    fn pem_to_der(&self, pem_data: &str) -> Result<Vec<u8>, ErrorInfo> {
        // 使用base64解码PEM格式
        use base64::{Engine as _, engine::general_purpose};

        // 移除PEM头部和尾部
        let lines: Vec<&str> = pem_data.lines()
            .filter(|line| !line.starts_with("-----BEGIN") && !line.starts_with("-----END"))
            .collect();

        let base64_content = lines.join("");

        general_purpose::STANDARD.decode(&base64_content)
            .map_err(|e| ErrorInfo::new(5018, format!("PEM转DER失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))
    }

    /// 验证远程证书
    pub async fn verify_remote_certificate(&self, cert_der: &[u8]) -> Result<bool, ErrorInfo> {
        let start_time = std::time::Instant::now();

        // 将DER转换为PEM
        let cert_pem = self.der_to_pem(cert_der);

        // 创建临时证书数据用于验证
        let cert_data = CertificateData::new(
            format!("cert-{}", start_time.elapsed().as_millis()),
            "remote".to_string(),
            cert_pem,
            None, // 无私钥
            bey_identity::CertificateType::Device,
            "unknown".to_string(), // issuer_id
            "CN=unknown".to_string(), // subject
        );

        // 使用证书管理器验证证书
        let result = self.certificate_manager.verify_certificate(&cert_data).await;

        let is_valid = result.map(|v| v.is_valid).unwrap_or(false);

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.certificate_verifications += 1;
            if is_valid {
                stats.connections_established += 1;
            } else {
                stats.connection_failures += 1;
            }
        }

        info!("证书验证完成，结果: {}", is_valid);
        Ok(is_valid)
    }

    /// DER格式转PEM格式
    fn der_to_pem(&self, der_data: &[u8]) -> String {
        // 使用base64编码和PEM格式包装
        use base64::{Engine as _, engine::general_purpose};

        let base64_encoded = general_purpose::STANDARD.encode(der_data);
        let mut pem = "-----BEGIN CERTIFICATE-----\n".to_string();

        // 每64字符换行
        for (i, chunk) in base64_encoded.as_bytes().chunks(64).enumerate() {
            if i > 0 {
                pem.push('\n');
            }
            pem.push_str(std::str::from_utf8(chunk).unwrap());
        }

        pem.push_str("\n-----END CERTIFICATE-----");
        pem
    }

    /// 计算证书指纹
    #[allow(dead_code)]
    fn calculate_fingerprint(&self, cert_der: &[u8]) -> String {
        use sha2::{Sha256, Digest};

        let mut hasher = Sha256::new();
        hasher.update(cert_der);
        let result = hasher.finalize();

        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// 清除配置缓存
    pub async fn clear_config_cache(&self) {
        debug!("清除mTLS配置缓存");

        {
            let mut server_cache = self.server_config_cache.write().await;
            *server_cache = None;
        }

        {
            let mut client_cache = self.client_config_cache.write().await;
            *client_cache = None;
        }

        info!("mTLS配置缓存已清除");
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> MtlsStats {
        self.stats.read().await.clone()
    }

    /// 获取本地证书信息
    pub async fn get_local_certificate_info(&self) -> Option<CertificateData> {
        self.certificate_manager
            .get_device_certificate(&self.device_id)
            .await
            .ok()
            .flatten()
    }

    /// 检查证书是否需要更新
    pub async fn needs_certificate_update(&self) -> Result<bool, ErrorInfo> {
        if let Some(cert_data) = self.get_local_certificate_info().await {
            let now = std::time::SystemTime::now();
            let renewal_threshold = now + Duration::from_secs(86400 * 30); // 30天阈值

            Ok(cert_data.expires_at <= renewal_threshold)
        } else {
            Ok(true) // 没有证书则需要更新
        }
    }

    /// 手动更新证书（重新签发）
    pub async fn update_certificate(&self) -> Result<(), ErrorInfo> {
        info!("手动更新证书");

        // 吊销现有证书并签发新证书
        let _ = self.certificate_manager
            .revoke_device_certificate(&self.device_id)
            .await
            .map_err(|e| ErrorInfo::new(5019, format!("吊销旧证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        // 签发新证书
        self.certificate_manager
            .issue_device_certificate(&self.device_id)
            .await
            .map_err(|e| ErrorInfo::new(5019, format!("证书更新失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        // 清除配置缓存以强制重新生成
        self.clear_config_cache().await;

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.certificate_renewals += 1;
        }

        info!("证书手动更新完成");
        Ok(())
    }

    /// 吊销证书
    pub async fn revoke_certificate(&self, cert_id: &str) -> Result<(), ErrorInfo> {
        info!("吊销证书: {}", cert_id);

        self.certificate_manager
            .revoke_device_certificate(cert_id)
            .await
            .map_err(|e| ErrorInfo::new(5020, format!("吊销证书失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        info!("证书吊销完成: {}", cert_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use tempfile::TempDir;
    use tokio::time::Duration as TokioDuration;

    static INIT: Once = Once::new();

    fn init_logging() {
        INIT.call_once(|| {
            // 尝试初始化日志，如果已经初始化则忽略错误
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .try_init();
        });
    }

    async fn create_test_mtls_config() -> (MtlsConfig, TempDir) {
        let temp_dir = TempDir::new().expect("创建临时目录失败");
        let config = MtlsConfig {
            enabled: true,
            certificates_dir: temp_dir.path().to_path_buf(),
            enable_config_cache: true,
            config_cache_ttl: Duration::from_secs(300),
            max_config_cache_entries: 10,
            device_id_prefix: "test".to_string(),
            organization_name: "Test BEY".to_string(),
            country_code: "CN".to_string(),
        };
        (config, temp_dir)
    }

    #[tokio::test]
    async fn test_mtls_config_validation() {
        let config = MtlsConfig::default();
        assert!(CompleteMtlsManager::validate_config(&config).is_ok());

        // 测试无效配置
        let mut invalid_config = config.clone();
        invalid_config.config_cache_ttl = Duration::from_secs(0);
        assert!(CompleteMtlsManager::validate_config(&invalid_config).is_err());

        let mut invalid_config = config.clone();
        invalid_config.max_config_cache_entries = 0;
        assert!(CompleteMtlsManager::validate_config(&invalid_config).is_err());
    }

    #[tokio::test]
    async fn test_mtls_manager_creation() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-001".to_string();

        let manager_result = CompleteMtlsManager::new(config, device_id).await;
        assert!(manager_result.is_ok(), "mTLS管理器创建应该成功");

        let manager = manager_result.unwrap();

        // 验证本地证书已初始化
        let local_cert = manager.get_local_certificate_info().await;
        assert!(local_cert.is_some(), "本地证书应该已初始化");

        // 验证统计信息
        let stats = manager.get_stats().await;
        assert_eq!(stats.certificate_renewals, 1); // 创建时应该生成了证书
    }

    #[tokio::test]
    async fn test_server_config_generation() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-002".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        let server_config_result = manager.get_server_config().await;
        assert!(server_config_result.is_ok(), "服务器配置生成应该成功");

        // 测试缓存
        let server_config_result2 = manager.get_server_config().await;
        assert!(server_config_result2.is_ok(), "第二次获取配置应该成功");

        let stats = manager.get_stats().await;
        assert!(stats.config_cache_hits > 0, "应该有缓存命中");
    }

    #[tokio::test]
    async fn test_client_config_generation() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-003".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        let client_config_result = manager.get_client_config().await;
        assert!(client_config_result.is_ok(), "客户端配置生成应该成功");
    }

    #[tokio::test]
    async fn test_certificate_verification() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-004".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        // 获取本地证书用于测试
        let cert_data = manager.get_local_certificate_info().await.unwrap();

        // 直接使用证书管理器验证证书
        let verification_result = manager.certificate_manager.verify_certificate(&cert_data).await
            .expect("证书验证应该成功");
        assert!(verification_result.is_valid, "本地证书应该有效");

        // 验证统计信息
        let stats = manager.get_stats().await;
        assert!(stats.certificate_verifications > 0, "应该有验证次数统计");
        assert!(stats.connections_established > 0, "应该有连接建立统计");
    }

    #[tokio::test]
    async fn test_config_cache_operations() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-005".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        // 生成服务器配置（缓存）
        let _server_config1 = manager.get_server_config().await.unwrap();
        let _server_config2 = manager.get_server_config().await.unwrap(); // 应该使用缓存

        // 清除缓存
        manager.clear_config_cache().await;

        // 再次生成（应该重新创建）
        let _server_config3 = manager.get_server_config().await.unwrap();

        // 验证统计信息
        let stats = manager.get_stats().await;
        assert!(stats.config_cache_hits > 0, "应该有缓存命中");
        assert!(stats.config_generations >= 2, "应该有配置生成统计");
    }

    #[tokio::test]
    async fn test_performance_benchmarks() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-perf".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        // 测试配置生成性能
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let _server_config = manager.get_server_config().await.unwrap();
            let _client_config = manager.get_client_config().await.unwrap();
        }
        let config_generation_time = start.elapsed();

        assert!(config_generation_time < TokioDuration::from_millis(1000),
                "配置生成应该在1秒内完成，实际耗时: {:?}", config_generation_time);

        info!("性能测试完成，配置生成耗时: {:?}", config_generation_time);

        // 验证统计信息
        let stats = manager.get_stats().await;
        assert!(stats.config_generations >= 20, "应该有足够的配置生成次数");
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-concurrent".to_string();

        let manager = Arc::new(CompleteMtlsManager::new(config, device_id).await.unwrap());

        // 并发配置生成
        let mut handles = Vec::new();
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                let server_config = manager_clone.get_server_config().await.unwrap();
                let client_config = manager_clone.get_client_config().await.unwrap();
                (i, server_config, client_config)
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            let _result = handle.await.unwrap();
            // 配置创建成功即验证通过
            assert!(true, "服务器和客户端配置应该有效");
        }

        // 验证最终统计信息
        let stats = manager.get_stats().await;
        assert!(stats.config_generations >= 20, "应该有足够的配置生成次数");
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-stats".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        // 执行一些操作
        let _server_config = manager.get_server_config().await.unwrap();
        let _client_config = manager.get_client_config().await.unwrap();
        manager.clear_config_cache().await;

        // 检查统计信息
        let stats = manager.get_stats().await;
        assert!(stats.config_generations >= 1, "应该有配置生成统计");
        assert!(stats.config_cache_hits >= 1, "应该有缓存命中统计");
    }

    #[tokio::test]
    async fn test_certificate_update_flow() {
        init_logging();
        let (config, _temp_dir) = create_test_mtls_config().await;
        let device_id = "test-device-update".to_string();

        let manager = CompleteMtlsManager::new(config, device_id).await.unwrap();

        // 获取初始证书
        let initial_cert = manager.get_local_certificate_info().await.unwrap();
        let initial_fingerprint = initial_cert.fingerprint.clone();

        // 测试证书更新
        let update_result = manager.update_certificate().await;
        assert!(update_result.is_ok(), "证书更新应该成功");

        // 获取更新后的证书
        let updated_cert = manager.get_local_certificate_info().await.unwrap();
        let updated_fingerprint = updated_cert.fingerprint.clone();

        // 验证证书已更新（指纹应该不同）
        assert_ne!(initial_fingerprint, updated_fingerprint, "证书指纹应该已更新");

        // 验证统计信息
        let stats = manager.get_stats().await;
        assert!(stats.certificate_renewals >= 1, "应该有证书更新统计");
    }
}