//! # BEY 网络模块
//!
//! 提供BEY系统的网络通信基础功能，包括mDNS设备发现、
//! 网络连接管理、数据传输等核心网络功能。
//!
//! ## 核心特性
//!
//! - **mDNS设备发现**: 零配置网络设备自动发现
//! - **网络通信**: 高性能网络数据传输
//! - **连接管理**: 智能连接池和连接复用
//! - **安全传输**: 支持加密和认证的网络通信
//! - **性能优化**: 零拷贝、内存池等性能优化技术
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use bey_net::{MdnsDiscovery, MdnsDiscoveryConfig, MdnsServiceInfo};
//! use std::net::IpAddr;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建mDNS配置
//! let config = MdnsDiscoveryConfig::default();
//!
//! // 创建设备信息
//! let device_info = MdnsServiceInfo {
//!     service_name: "my-device".to_string(),
//!     service_type: "_bey._tcp".to_string(),
//!     domain: "local".to_string(),
//!     hostname: "my-device.local".to_string(),
//!     port: 8080,
//!     priority: 0,
//!     weight: 0,
//!     addresses: vec![],
//!     txt_records: vec![],
//!     ttl: 120,
//! };
//!
//! // 创建mDNS发现服务
//! let discovery = MdnsDiscovery::new(config, device_info).await?;
//!
//! // 启动服务
//! discovery.start().await?;
//!
//! // 查询服务
//! let services = discovery.query_service("_bey._tcp", None).await?;
//! println!("发现 {} 个服务", services.len());
//!
//! // 停止服务
//! discovery.stop().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 模块结构
//!
//! - `mdns_discovery` - mDNS设备发现功能
//! - 未来将添加更多网络功能模块

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};

/// 网络模块结果类型
pub type NetResult<T> = std::result::Result<T, ErrorInfo>;

// 导出mDNS发现模块
mod mdns_discovery;
pub use mdns_discovery::{
    MdnsDiscovery, MdnsDiscoveryConfig, MdnsServiceInfo, MdnsDiscoveryEvent,
    MdnsInfo, MdnsQuery, MdnsRecord, MdnsResponse, MdnsRecordType, MdnsQueryType,
    MdnsDiscoveryStats, mdns_constants, create_default_mdns_config, 
    create_default_mdns_device_info,
};

// 导出UDP广播发现模块
mod udp_discovery;
pub use udp_discovery::{
    DeviceInfo, DiscoveryConfig, DiscoveryMessage, DeviceEvent,
    DiscoveryService, DiscoveryResult,
};

/// 网络模块版本信息
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 网络模块描述
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// 创建默认错误信息
///
/// # 参数
///
/// * `code` - 错误代码
/// * `message` - 错误消息
///
/// # 返回值
///
/// 返回网络模块错误信息
pub fn create_network_error(code: u32, message: String) -> ErrorInfo {
    ErrorInfo::new(code, message)
        .with_category(ErrorCategory::Network)
        .with_severity(ErrorSeverity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty(), "版本号不应为空");
    }

    #[test]
    fn test_description() {
        assert!(!DESCRIPTION.is_empty(), "描述不应为空");
    }

    #[test]
    fn test_create_network_error() {
        let error = create_network_error(1001, "测试错误".to_string());
        assert_eq!(error.code(), 1001);
        assert_eq!(error.message(), "测试错误");
    }
}
