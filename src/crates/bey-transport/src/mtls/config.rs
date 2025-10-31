//! # mTLS配置模块
//!
//! 定义mTLS双向认证的配置选项

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// mTLS配置
///
/// 配置mTLS双向认证的各项参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConfig {
    /// 是否启用mTLS
    pub enabled: bool,
    /// 证书存储目录
    pub certificates_dir: PathBuf,
    /// 是否启用配置缓存
    pub enable_config_cache: bool,
    /// 配置缓存TTL（生存时间）
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
            certificates_dir: PathBuf::from("./certs"),
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
///
/// 记录mTLS管理器的运行统计数据
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
