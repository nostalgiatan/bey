//! # 连接池模块
//!
//! 提供高效的QUIC连接复用和管理功能

pub mod config;
pub mod types;

// 重新导出常用类型
pub use config::{CompleteConnectionPoolConfig, LoadBalanceStrategy};
pub use types::{
    CompleteConnectionInfo, ConnectionHealthStatus, CompleteConnectionStats,
    CompletePoolEvent, ConnectionRequest, AddressGroup,
};
