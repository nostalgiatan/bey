//! # 策略引擎配置模块
//!
//! 定义策略引擎的配置选项

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 策略引擎配置
///
/// 配置策略引擎的各项参数，包括缓存、性能监控等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEngineConfig {
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 缓存TTL（生存时间）
    pub cache_ttl: Duration,
    /// 最大缓存条目数
    pub max_cache_entries: usize,
    /// 是否启用性能监控
    pub enable_performance_monitoring: bool,
    /// 是否启用详细日志
    pub enable_detailed_logging: bool,
    /// 最大评估时间
    pub max_evaluation_time: Duration,
}

impl Default for PolicyEngineConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            cache_ttl: Duration::from_secs(300), // 5分钟
            max_cache_entries: 10000,
            enable_performance_monitoring: true,
            enable_detailed_logging: false,
            max_evaluation_time: Duration::from_secs(10),
        }
    }
}
