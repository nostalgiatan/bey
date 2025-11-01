//! # 局域网中心项目 - BEY
//!
//! 一个去中心化的局域网协作平台，提供对象剪切板、文件传输、消息传递、
//! 权限控制、分布式云空间、证书管理和传输优先级控制功能。
//!
//! ## 核心特性
//!
//! - **去中心化架构**: 无需中央服务器，提高可靠性和隐私性
//! - **局域网优化**: 低延迟、高速度的本地传输
//! - **多功能集成**: 剪切板、文件传输、消息推送一体化
//! - **资源贡献**: 利用闲置磁盘空间创建分布式存储
//! - **权限管理**: 基于证书的细粒度权限控制
//! - **极致性能**: 零隐式转换，内存安全优化
//! - **插件系统**: 灵活的插件架构，支持功能扩展
//!
//! ## 模块架构
//!
//! ```text
//! bey/
//! ├── src/
//! │   ├── main.rs         # 主程序入口
//! │   ├── lib.rs          # 库入口
//! │   ├── app.rs          # 应用程序管理器
//! │   └── crates/
//! │       ├── error/          # 错误处理框架
//! │       ├── sys/            # 系统监控模块
//! │       ├── bey-types/      # 类型定义
//! │       ├── bey-identity/   # 身份和证书管理
//! │       ├── bey-transport/  # QUIC传输层
//! │       ├── bey-net/        # 网络传输和Token路由
//! │       ├── bey-storage/    # 分布式存储（对象、云、剪切板、消息）
//! │       ├── bey-func/       # 分布式功能高级API
//! │       ├── bey-plugin/     # 插件系统
//! │       ├── bey-gui/        # GUI界面（Tauri）
//! │       └── bey-tui/        # TUI界面（ratatui）
//! ```
//!
//! ## 使用示例
//!
//! ```no_run
//! use bey::app::{AppConfig, BeyAppManager};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建应用程序管理器
//!     let config = AppConfig::default();
//!     let mut manager = BeyAppManager::new(config).await?;
//!
//!     // 初始化并启动
//!     manager.initialize().await?;
//!     manager.start().await?;
//!
//!     // 应用程序运行...
//!
//!     // 停止应用程序
//!     manager.stop().await?;
//!     Ok(())
//! }
//! ```

// 导出应用程序模块
pub mod app;

use error::{ErrorInfo, ErrorCategory};
use sys::SystemInfo;
use std::net::SocketAddr;

// 重新导出证书管理模块
pub use bey_identity::certificate::{CertificateManager, CertificateAuthority, CertificateManagerStatistics};
pub use bey_identity::config::{CertificateConfig, CertificatePolicy};
pub use bey_identity::types::{CertificateData, CertificateType, CertificateStatus, CertificateVerificationResult, KeyPairInfo};
pub use bey_identity::validation::{CertificateValidator, ValidatorStatistics};
pub use bey_identity::storage::{CertificateStorage, StorageStatistics};
pub use bey_identity::error::{IdentityError, ConfigError};

/// 应用程序结果类型
pub type AppResult<T> = std::result::Result<T, ErrorInfo>;

/// 设备信息结构体
///
/// 表示局域网中的一个设备节点，包含设备的基本身份信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    /// 设备唯一标识符
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 设备类型
    pub device_type: DeviceType,
    /// 网络地址
    pub address: SocketAddr,
    /// 设备能力
    pub capabilities: Vec<Capability>,
    /// 最后活跃时间
    pub last_active: std::time::SystemTime,
}

/// 设备类型枚举
///
/// 定义不同类型的设备，用于权限控制和功能适配
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeviceType {
    /// 桌面计算机
    Desktop,
    /// 笔记本电脑
    Laptop,
    /// 移动设备
    Mobile,
    /// 服务器
    Server,
    /// 嵌入式设备
    Embedded,
}

/// 设备能力枚举
///
/// 定义设备支持的功能特性
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Capability {
    /// 文件传输能力
    FileTransfer,
    /// 剪切板同步能力
    ClipboardSync,
    /// 消息传递能力
    Messaging,
    /// 存储贡献能力
    StorageContribution,
    /// 证书管理能力
    CertificateManagement,
}

/// BEY 应用程序主结构体
///
/// 管理整个应用程序的生命周期和核心功能
pub struct BeyApp {
    /// 本地设备信息
    local_device: DeviceInfo,
    /// 系统信息监控
    system_info: SystemInfo,
}

impl BeyApp {
    /// 创建新的 BEY 应用程序实例
    ///
    /// # 返回值
    ///
    /// 返回初始化的应用程序实例或错误信息
    pub async fn new() -> AppResult<Self> {
        // 获取系统信息
        let system_info = SystemInfo::new().await;

        // 生成本地设备信息
        let local_device = Self::create_local_device_info(&system_info)?;

        Ok(Self {
            local_device,
            system_info,
        })
    }

    /// 创建本地设备信息
    ///
    /// 根据系统信息生成本地设备的身份和能力信息
    ///
    /// # 参数
    ///
    /// * `system_info` - 系统信息引用
    ///
    /// # 返回值
    ///
    /// 返回设备信息或错误信息
    fn create_local_device_info(system_info: &SystemInfo) -> AppResult<DeviceInfo> {
        // 生成设备唯一标识符
        let device_id = Self::generate_device_id(system_info)?;

        // 获取设备名称
        let device_name = system_info.host_name();

        // 根据系统信息推断设备类型
        let device_type = Self::infer_device_type(system_info);

        // 获取本机网络地址
        let address = Self::get_local_address()?;

        // 确定设备能力
        let capabilities = Self::determine_capabilities(&device_type, system_info);

        Ok(DeviceInfo {
            device_id,
            device_name,
            device_type,
            address,
            capabilities,
            last_active: std::time::SystemTime::now(),
        })
    }

    /// 生成设备唯一标识符
    ///
    /// 基于系统硬件信息生成唯一的设备ID
    ///
    /// # 参数
    ///
    /// * `system_info` - 系统信息引用
    ///
    /// # 返回值
    ///
    /// 返回设备ID字符串或错误信息
    fn generate_device_id(system_info: &SystemInfo) -> AppResult<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // 组合系统唯一信息
        let mut hasher = DefaultHasher::new();
        system_info.host_name().hash(&mut hasher);
        system_info.os_name().hash(&mut hasher);
        system_info.kernel_version().hash(&mut hasher);
        system_info.cpu_count().hash(&mut hasher);

        // 获取内存信息作为额外熵
        let (used_mem, total_mem) = system_info.memory_info();
        used_mem.hash(&mut hasher);
        total_mem.hash(&mut hasher);

        // 生成十六进制设备ID
        let hash_value = hasher.finish();
        Ok(format!("bey-{:016x}", hash_value))
    }

    /// 推断设备类型
    ///
    /// 根据系统信息推断设备的类型
    ///
    /// # 参数
    ///
    /// * `system_info` - 系统信息引用
    ///
    /// # 返回值
    ///
    /// 返回推断的设备类型
    fn infer_device_type(system_info: &SystemInfo) -> DeviceType {
        let os_name = system_info.os_name().to_lowercase();
        let cpu_count = system_info.cpu_count();
        let memory_gb = system_info.memory_info().1 / (1024 * 1024 * 1024);

        // 简单的启发式规则推断设备类型
        if os_name.contains("android") || os_name.contains("ios") {
            DeviceType::Mobile
        } else if cpu_count >= 16 && memory_gb >= 32 {
            DeviceType::Server
        } else if os_name.contains("windows") || os_name.contains("macos") || os_name.contains("linux") {
            if memory_gb <= 16 {
                DeviceType::Laptop
            } else {
                DeviceType::Desktop
            }
        } else {
            DeviceType::Embedded
        }
    }

    /// 获取本机网络地址
    ///
    /// 获取用于局域网通信的本机IP地址
    ///
    /// # 返回值
    ///
    /// 返回本机Socket地址或错误信息
    fn get_local_address() -> AppResult<SocketAddr> {
        // 绑定到任意地址获取本地接口信息
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| ErrorInfo::new(1001, format!("绑定本地地址失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        // 连接到外部地址以确定本地出口IP
        socket.connect("8.8.8.8:80")
            .map_err(|e| ErrorInfo::new(1002, format!("连接外部地址失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        // 获取本地地址
        let local_addr = socket.local_addr()
            .map_err(|e| ErrorInfo::new(1003, format!("获取本地地址失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        // 使用默认端口，后续可以通过配置文件修改
        let socket_addr: SocketAddr = format!("{}:8080", local_addr.ip())
            .parse()
            .map_err(|e| ErrorInfo::new(1004, format!("解析地址失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        Ok(socket_addr)
    }

    /// 确定设备能力
    ///
    /// 根据设备类型和系统信息确定设备支持的功能
    ///
    /// # 参数
    ///
    /// * `device_type` - 设备类型
    /// * `system_info` - 系统信息引用
    ///
    /// # 返回值
    ///
    /// 返回设备能力列表
    fn determine_capabilities(device_type: &DeviceType, system_info: &SystemInfo) -> Vec<Capability> {
        let mut capabilities = Vec::with_capacity(5);

        // 所有设备都支持基本的消息传递
        capabilities.push(Capability::Messaging);

        // 根据设备类型添加不同能力
        match device_type {
            DeviceType::Desktop | DeviceType::Laptop => {
                capabilities.push(Capability::FileTransfer);
                capabilities.push(Capability::ClipboardSync);
                capabilities.push(Capability::CertificateManagement);

                // 如果有足够的磁盘空间，支持存储贡献
                let available_gb = system_info.disk_available() / (1024 * 1024 * 1024);
                if available_gb >= 10 {
                    capabilities.push(Capability::StorageContribution);
                }
            }
            DeviceType::Server => {
                capabilities.push(Capability::FileTransfer);
                capabilities.push(Capability::StorageContribution);
                capabilities.push(Capability::CertificateManagement);
            }
            DeviceType::Mobile => {
                capabilities.push(Capability::FileTransfer);
                capabilities.push(Capability::ClipboardSync);
            }
            DeviceType::Embedded => {
                capabilities.push(Capability::Messaging);
            }
        }

        capabilities
    }

    /// 获取本地设备信息
    ///
    /// # 返回值
    ///
    /// 返回本地设备信息的不可变引用
    pub fn local_device(&self) -> &DeviceInfo {
        &self.local_device
    }

    /// 获取系统信息
    ///
    /// # 返回值
    ///
    /// 返回系统信息的不可变引用
    pub fn system_info(&self) -> &SystemInfo {
        &self.system_info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bey_app_creation() {
        // 测试应用程序创建
        let app_result = BeyApp::new().await;
        assert!(app_result.is_ok(), "应用程序创建应该成功");

        let app = app_result.unwrap();
        assert!(!app.local_device.device_id.is_empty(), "设备ID不应为空");
        assert!(!app.local_device.device_name.is_empty(), "设备名称不应为空");
        assert!(!app.local_device.capabilities.is_empty(), "设备能力不应为空");
    }

    #[tokio::test]
    async fn test_device_id_generation() {
        let system_info = SystemInfo::new().await;

        // 测试设备ID生成
        let device_id1 = BeyApp::generate_device_id(&system_info).unwrap();
        let device_id2 = BeyApp::generate_device_id(&system_info).unwrap();

        // 同一系统应该生成相同的设备ID
        assert_eq!(device_id1, device_id2, "同一系统应该生成相同的设备ID");
        assert!(device_id1.starts_with("bey-"), "设备ID应该以'bey-'开头");
        assert_eq!(device_id1.len(), 20, "设备ID长度应该为20个字符");
    }

    #[tokio::test]
    async fn test_device_type_inference() {
        let system_info = SystemInfo::new().await;

        let device_type = BeyApp::infer_device_type(&system_info);

        // 设备类型应该是已知的类型之一
        match device_type {
            DeviceType::Desktop | DeviceType::Laptop | DeviceType::Mobile |
            DeviceType::Server | DeviceType::Embedded => {
                // 所有类型都是有效的
            }
        }
    }

    #[tokio::test]
    async fn test_local_address_retrieval() {
        let addr_result = BeyApp::get_local_address();
        assert!(addr_result.is_ok(), "获取本地地址应该成功");

        let addr = addr_result.unwrap();
        match addr.ip() {
            std::net::IpAddr::V4(ipv4) => {
                // 检查是否为有效的IPv4地址（私有或本地地址）
                assert!(
                    ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local(),
                    "地址应该是私有、本地或链路本地地址，实际为: {}",
                    ipv4
                );
            },
            std::net::IpAddr::V6(ipv6) => {
                // IPv6地址也应该是有效的本地地址
                assert!(
                    ipv6.is_loopback() || ipv6.is_unicast_link_local() || ipv6.is_unique_local(),
                    "IPv6地址应该是本地地址，实际为: {}",
                    ipv6
                );
            }
        }
        assert_eq!(addr.port(), 8080, "端口应该是8080");
    }

    #[tokio::test]
    async fn test_capability_determination() {
        let system_info = SystemInfo::new().await;
        let device_type = DeviceType::Desktop;

        let capabilities = BeyApp::determine_capabilities(&device_type, &system_info);

        assert!(!capabilities.is_empty(), "能力列表不应为空");
        assert!(capabilities.contains(&Capability::Messaging), "应该支持消息传递");

        // 桌面设备应该支持更多能力
        if system_info.disk_available() / (1024 * 1024 * 1024) >= 10 {
            assert!(capabilities.contains(&Capability::StorageContribution),
                   "有足够空间的设备应该支持存储贡献");
        }
    }

    #[tokio::test]
    async fn test_device_info_serialization() {
        let system_info = SystemInfo::new().await;
        let device_info = BeyApp::create_local_device_info(&system_info).unwrap();

        // 测试序列化和反序列化
        let serialized = serde_json::to_string(&device_info).unwrap();
        let deserialized: DeviceInfo = serde_json::from_str(&serialized).unwrap();

        assert_eq!(device_info.device_id, deserialized.device_id);
        assert_eq!(device_info.device_name, deserialized.device_name);
        assert_eq!(device_info.device_type, deserialized.device_type);
    }
}