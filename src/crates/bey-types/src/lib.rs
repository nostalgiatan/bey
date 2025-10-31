//! # BEY 共享类型定义
//!
//! 定义在各个模块间共享的数据类型，避免循环依赖。
//! 提供完整的数据结构支持，包括设备信息、网络协议、安全认证等。

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// 设备类型枚举
///
/// 定义不同类型的设备，用于权限控制和功能适配
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// 设备信息结构体
///
/// 表示局域网中的一个设备节点，包含设备的基本身份信息
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub last_active: SystemTime,
    /// 设备状态
    pub status: DeviceStatus,
    /// 系统信息
    pub system_info: SystemInfo,
    /// 信任级别
    pub trust_level: TrustLevel,
    /// 版本信息
    pub version: String,
    /// 创建时间
    pub created_at: SystemTime,
}

impl DeviceInfo {
    /// 创建新的设备信息
    pub fn new(
        device_id: String,
        device_name: String,
        device_type: DeviceType,
        address: SocketAddr,
    ) -> Self {
        Self {
            device_id,
            device_name,
            device_type,
            address,
            capabilities: Vec::new(),
            last_active: SystemTime::now(),
            status: DeviceStatus::Online,
            system_info: SystemInfo::default(),
            trust_level: TrustLevel::Unknown,
            version: "1.0.0".to_string(),
            created_at: SystemTime::now(),
        }
    }

    /// 添加能力
    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// 设置状态
    pub fn with_status(mut self, status: DeviceStatus) -> Self {
        self.status = status;
        self
    }

    /// 检查设备是否在线
    pub fn is_online(&self) -> bool {
        matches!(self.status, DeviceStatus::Online)
    }

    /// 检查设备是否支持特定能力
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }

    /// 更新最后活跃时间
    pub fn update_last_active(&mut self) {
        self.last_active = SystemTime::now();
    }

    /// 计算设备年龄（创建到现在的时间）
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }
}

/// 设备状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceStatus {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 忙碌
    Busy,
    /// 维护中
    Maintenance,
    /// 错误状态
    Error(String),
}

/// 系统信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// 操作系统
    pub os: String,
    /// 架构
    pub arch: String,
    /// CPU核心数
    pub cpu_cores: u32,
    /// 内存大小（字节）
    pub memory_bytes: u64,
    /// 可用存储空间（字节）
    pub available_storage: u64,
    /// 网络延迟（毫秒）
    pub network_latency_ms: Option<u32>,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            os: "Unknown".to_string(),
            arch: "Unknown".to_string(),
            cpu_cores: 1,
            memory_bytes: 0,
            available_storage: 0,
            network_latency_ms: None,
        }
    }
}

/// 信任级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustLevel {
    /// 未知
    Unknown = 0,
    /// 不信任
    Untrusted = 1,
    /// 基本信任
    Basic = 2,
    /// 受信任
    Trusted = 3,
    /// 完全信任
    FullyTrusted = 4,
}

/// 网络协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolType {
    /// QUIC协议
    Quic,
    /// TCP协议
    Tcp,
    /// UDP协议
    Udp,
    /// WebSocket协议
    WebSocket,
}

/// 消息类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// 设备发现
    DeviceDiscovery,
    /// 设备公告
    DeviceAnnouncement,
    /// 心跳
    Heartbeat,
    /// 文件传输请求
    FileTransferRequest,
    /// 文件传输响应
    FileTransferResponse,
    /// 文件数据
    FileData,
    /// 存储请求
    StorageRequest,
    /// 存储响应
    StorageResponse,
    /// 权限请求
    PermissionRequest,
    /// 权限响应
    PermissionResponse,
    /// 认证请求
    AuthRequest,
    /// 认证响应
    AuthResponse,
    /// 错误消息
    Error(String),
}

/// 网络消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    /// 消息ID
    pub message_id: String,
    /// 消息类型
    pub message_type: MessageType,
    /// 发送者设备ID
    pub sender_id: String,
    /// 接收者设备ID（可选，广播消息时为空）
    pub receiver_id: Option<String>,
    /// 消息内容
    pub payload: Vec<u8>,
    /// 时间戳
    pub timestamp: SystemTime,
    /// 协议类型
    pub protocol: ProtocolType,
    /// 消息优先级
    pub priority: MessagePriority,
    /// 是否需要确认
    pub requires_ack: bool,
    /// 跳数限制（TTL）
    pub ttl: Option<u32>,
}

impl NetworkMessage {
    /// 创建新消息
    pub fn new(
        message_type: MessageType,
        sender_id: String,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4().to_string(),
            message_type,
            sender_id,
            receiver_id: None,
            payload,
            timestamp: SystemTime::now(),
            protocol: ProtocolType::Quic,
            priority: MessagePriority::Normal,
            requires_ack: false,
            ttl: Some(64),
        }
    }

    /// 设置接收者
    pub fn with_receiver(mut self, receiver_id: String) -> Self {
        self.receiver_id = Some(receiver_id);
        self
    }

    /// 设置协议
    pub fn with_protocol(mut self, protocol: ProtocolType) -> Self {
        self.protocol = protocol;
        self
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// 需要确认
    pub fn requires_acknowledgment(mut self) -> Self {
        self.requires_ack = true;
        self
    }

    /// 检查消息是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = self.timestamp.elapsed().unwrap_or_default();
            elapsed.as_secs() > ttl as u64
        } else {
            false
        }
    }

    /// 获取消息大小
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}

/// 消息优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// 低优先级
    Low = 0,
    /// 普通优先级
    Normal = 1,
    /// 高优先级
    High = 2,
    /// 紧急优先级
    Critical = 3,
}

/// 安全级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// 无加密
    None = 0,
    /// 基本加密
    Basic = 1,
    /// 标准加密
    Standard = 2,
    /// 高级加密
    High = 3,
    /// 军事级加密
    Military = 4,
}

/// 认证方法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    /// 无认证
    None,
    /// 预共享密钥
    PreSharedKey,
    /// 证书认证
    Certificate,
    /// 密码认证
    Password,
    /// 双因素认证
    TwoFactor,
}

/// 连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// 连接ID
    pub connection_id: String,
    /// 本地地址
    pub local_address: SocketAddr,
    /// 远程地址
    pub remote_address: SocketAddr,
    /// 协议类型
    pub protocol: ProtocolType,
    /// 安全级别
    pub security_level: SecurityLevel,
    /// 认证方法
    pub auth_method: AuthMethod,
    /// 建立时间
    pub established_at: SystemTime,
    /// 最后活动时间
    pub last_activity: SystemTime,
    /// 发送字节数
    pub bytes_sent: u64,
    /// 接收字节数
    pub bytes_received: u64,
    /// 连接状态
    pub status: ConnectionStatus,
}

impl ConnectionInfo {
    /// 创建新连接信息
    pub fn new(
        local_address: SocketAddr,
        remote_address: SocketAddr,
        protocol: ProtocolType,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            connection_id: Uuid::new_v4().to_string(),
            local_address,
            remote_address,
            protocol,
            security_level: SecurityLevel::None,
            auth_method: AuthMethod::None,
            established_at: now,
            last_activity: now,
            bytes_sent: 0,
            bytes_received: 0,
            status: ConnectionStatus::Connecting,
        }
    }

    /// 设置安全级别
    pub fn with_security_level(mut self, level: SecurityLevel) -> Self {
        self.security_level = level;
        self
    }

    /// 设置认证方法
    pub fn with_auth_method(mut self, method: AuthMethod) -> Self {
        self.auth_method = method;
        self
    }

    /// 更新活动时间
    pub fn update_activity(&mut self) {
        self.last_activity = SystemTime::now();
    }

    /// 增加发送字节数
    pub fn add_sent_bytes(&mut self, bytes: u64) {
        self.bytes_sent += bytes;
        self.update_activity();
    }

    /// 增加接收字节数
    pub fn add_received_bytes(&mut self, bytes: u64) {
        self.bytes_received += bytes;
        self.update_activity();
    }

    /// 获取连接持续时间
    pub fn duration(&self) -> Duration {
        self.established_at.elapsed().unwrap_or_default()
    }

    /// 检查连接是否活跃
    pub fn is_active(&self) -> bool {
        matches!(self.status, ConnectionStatus::Connected | ConnectionStatus::Active)
    }
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// 连接中
    Connecting,
    /// 已连接
    Connected,
    /// 活跃状态
    Active,
    /// 断开连接中
    Disconnecting,
    /// 已断开
    Disconnected,
    /// 错误状态
    Error,
}