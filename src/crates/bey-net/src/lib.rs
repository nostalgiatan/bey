//! # BEY 网络模块
//!
//! 提供BEY系统的网络通信基础功能，基于有限状态机和令牌系统的高性能网络架构。
//!
//! ## 核心特性
//!
//! - **令牌系统**: 灵活的基于令牌的消息传输
//! - **状态机管理**: 清晰的连接状态管理
//! - **元接收器**: 灵活的消息接收和过滤机制
//! - **集成认证**: 无缝集成BEY身份认证
//! - **加密传输**: 自动化的加密和解密
//! - **高性能**: 零拷贝、内存池等优化技术
//!
//! ## 架构设计
//!
//! 本模块采用分层设计：
//!
//! 1. **令牌层**: 定义网络传输的基本单位（Token）
//! 2. **状态机层**: 管理连接的生命周期和状态转换
//! 3. **接收器层**: 提供灵活的消息接收和处理机制
//! 4. **传输引擎层**: 集成所有组件的完整传输引擎
//! 5. **设备发现层**: mDNS和UDP广播设备发现
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use bey_net::{TransportEngine, EngineConfig, Token, TokenMeta};
//! use std::net::SocketAddr;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建传输引擎
//! let config = EngineConfig::default();
//! let engine = TransportEngine::new(config).await?;
//!
//! // 连接到服务器
//! let server_addr: SocketAddr = "127.0.0.1:8080".parse()?;
//! engine.connect(server_addr).await?;
//!
//! // 发送令牌
//! let meta = TokenMeta::new("test".to_string(), "client".to_string());
//! let token = Token::new(meta, vec![1, 2, 3]);
//! engine.send_token(token).await?;
//!
//! // 接收令牌
//! use bey_net::ReceiverMode;
//! if let Some(token) = engine.receive_token(ReceiverMode::NonBlocking).await? {
//!     println!("收到令牌: {}", token.meta.id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## 模块结构
//!
//! - `token` - 令牌系统：定义Token、TokenMeta、TokenHandler、TokenRouter
//! - `state_machine` - 状态机：管理连接状态和转换
//! - `receiver` - 元接收器：灵活的消息接收机制
//! - `engine` - 传输引擎：集成所有功能的核心引擎
//! - `stream` - 流式传输：大文件分块传输和流水线
//! - `priority_queue` - 优先级队列：令牌优先级排序和确认机制
//! - `flow_control` - 流量控制：滑动窗口和拥塞控制
//! - `metrics` - 性能监控：指标收集和统计
//! - `mdns_discovery` - mDNS设备发现
//! - `udp_discovery` - UDP广播设备发现

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};

/// 网络模块结果类型
pub type NetResult<T> = std::result::Result<T, ErrorInfo>;

// 导出令牌系统
pub mod token;
pub use token::{
    Token, TokenMeta, TokenId, TokenType, TokenPriority,
    TokenHandler, TokenRouter,
};

// 导出状态机
pub mod state_machine;
pub use state_machine::{
    ConnectionState, ConnectionStateMachine, StateEvent, StateTransition,
};

// 导出元接收器
pub mod receiver;
pub use receiver::{
    MetaReceiver, BufferedReceiver, ReceiverMode,
    ReceiverFilter, TypeFilter, PriorityFilter,
    create_receiver,
};

// 导出传输引擎
pub mod engine;
pub use engine::{
    TransportEngine, EngineConfig,
};

// 导出流式传输
pub mod stream;
pub use stream::{
    StreamFlag, StreamMeta, StreamChunk, StreamSession, StreamManager,
};

// 导出优先级队列
pub mod priority_queue;
pub use priority_queue::{
    PriorityQueue, AckStatus,
};

// 导出流量控制
pub mod flow_control;
pub use flow_control::{
    FlowController, FlowControlStats, CongestionState,
    RateLimiter,
};

// 导出性能监控
pub mod metrics;
pub use metrics::{
    Metrics, MetricsCollector, ErrorStats,
};

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
