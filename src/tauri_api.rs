//! # Tauri API Module
//!
//! 定义所有 GUI 与后端之间的 API 接口，包括：
//! - 前端调用后端的命令 (Commands)
//! - 后端推送给前端的事件 (Events)
//! - 双向通信的事件通道
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Tauri Frontend                        │
//! │  ┌──────────────┐         ┌──────────────┐             │
//! │  │   Commands   │         │    Events    │             │
//! │  │  (F -> B)    │         │   (B -> F)   │             │
//! │  └──────┬───────┘         └──────▲───────┘             │
//! └─────────┼─────────────────────────┼─────────────────────┘
//!           │                         │
//!           ▼                         │
//! ┌─────────────────────────────────────────────────────────┐
//! │                   Tauri API Layer                        │
//! │  ┌──────────────────────────────────────────────────┐  │
//! │  │            TauriEventChannel                      │  │
//! │  │  - emit_network_sync()                           │  │
//! │  │  - emit_list_update()                            │  │
//! │  │  - emit_new_message()                            │  │
//! │  │  - emit_new_file()                               │  │
//! │  └──────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────┘
//!           │                         ▲
//!           │                         │
//!           ▼                         │
//! ┌─────────────────────────────────────────────────────────┐
//! │                  BEY Backend Core                        │
//! │  - BeyFuncManager                                        │
//! │  - Network Layer                                         │
//! │  - Storage Layer                                         │
//! └─────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use error::ErrorInfo;

/// API 结果类型
pub type ApiResult<T> = Result<T, ErrorInfo>;

// ============================================================================
// 前端 -> 后端 Commands (前端调用后端)
// ============================================================================

/// 配置相关命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    /// 配置项键
    pub key: String,
    /// 配置项值
    pub value: serde_json::Value,
}

/// 热重载请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadRequest {
    /// 重载类型 (config, plugin, theme等)
    pub reload_type: String,
    /// 可选的目标路径
    pub target: Option<String>,
}

/// 程序启动配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupConfig {
    /// 设备名称
    pub device_name: String,
    /// 存储路径
    pub storage_path: String,
    /// 网络端口
    pub port: Option<u16>,
    /// 是否自动发现
    pub auto_discovery: bool,
}

/// 发送消息请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// 目标设备ID (None 表示广播)
    pub target_device: Option<String>,
    /// 消息内容
    pub content: String,
    /// 消息类型
    pub message_type: MessageType,
}

/// 接收消息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveMessageResponse {
    /// 消息ID
    pub message_id: String,
    /// 发送方设备ID
    pub from_device: String,
    /// 消息内容
    pub content: String,
    /// 消息类型
    pub message_type: MessageType,
    /// 时间戳
    pub timestamp: i64,
}

/// 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    /// 私信
    Private,
    /// 群消息
    Group,
    /// 广播
    Broadcast,
}

/// 文件传输请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferRequest {
    /// 目标设备ID
    pub target_device: String,
    /// 文件路径
    pub file_path: String,
    /// 文件大小
    pub file_size: u64,
}

/// 剪切板操作请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardRequest {
    /// 操作类型
    pub operation: ClipboardOperation,
    /// 剪切板数据
    pub data: Option<String>,
}

/// 剪切板操作类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardOperation {
    /// 添加
    Add,
    /// 删除
    Remove,
    /// 同步
    Sync,
    /// 获取
    Get,
}

// ============================================================================
// 后端 -> 前端 Events (后端推送给前端)
// ============================================================================

/// 网络同步事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSyncEvent {
    /// 同步类型
    pub sync_type: NetworkSyncType,
    /// 设备ID
    pub device_id: String,
    /// 同步数据
    pub data: serde_json::Value,
    /// 时间戳
    pub timestamp: i64,
}

/// 网络同步类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkSyncType {
    /// 设备上线
    DeviceOnline,
    /// 设备下线
    DeviceOffline,
    /// 网络状态变化
    NetworkStatusChanged,
    /// 连接建立
    ConnectionEstablished,
    /// 连接断开
    ConnectionLost,
}

/// 列表更新事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUpdateEvent {
    /// 列表类型
    pub list_type: ListType,
    /// 更新操作
    pub operation: ListOperation,
    /// 更新的项目
    pub item: serde_json::Value,
}

/// 列表类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListType {
    /// 设备列表
    Devices,
    /// 消息列表
    Messages,
    /// 文件列表
    Files,
    /// 剪切板列表
    Clipboard,
}

/// 列表操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListOperation {
    /// 添加
    Add,
    /// 更新
    Update,
    /// 删除
    Remove,
    /// 清空
    Clear,
}

/// 新消息事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMessageEvent {
    /// 消息ID
    pub message_id: String,
    /// 发送方设备ID
    pub from_device: String,
    /// 发送方设备名称
    pub from_device_name: String,
    /// 消息内容
    pub content: String,
    /// 消息类型
    pub message_type: MessageType,
    /// 时间戳
    pub timestamp: i64,
    /// 是否已读
    pub is_read: bool,
}

/// 新文件事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFileEvent {
    /// 文件ID
    pub file_id: String,
    /// 文件名
    pub file_name: String,
    /// 文件大小
    pub file_size: u64,
    /// 发送方设备ID
    pub from_device: String,
    /// 发送方设备名称
    pub from_device_name: String,
    /// 文件类型
    pub file_type: String,
    /// 时间戳
    pub timestamp: i64,
    /// 传输进度 (0-100)
    pub progress: u8,
}

/// 文件传输进度事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferProgressEvent {
    /// 传输ID
    pub transfer_id: String,
    /// 文件名
    pub file_name: String,
    /// 已传输字节数
    pub transferred_bytes: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 进度百分比 (0-100)
    pub progress: u8,
    /// 传输速度 (bytes/sec)
    pub speed: u64,
}

/// 系统通知事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemNotificationEvent {
    /// 通知类型
    pub notification_type: NotificationType,
    /// 通知标题
    pub title: String,
    /// 通知内容
    pub message: String,
    /// 时间戳
    pub timestamp: i64,
}

/// 通知类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationType {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 错误
    Error,
    /// 成功
    Success,
}

// ============================================================================
// 事件通道系统
// ============================================================================

/// Tauri 事件通道
///
/// 管理所有后端到前端的事件推送
pub struct TauriEventChannel {
    /// 网络同步事件发送器
    network_sync_tx: broadcast::Sender<NetworkSyncEvent>,
    /// 列表更新事件发送器
    list_update_tx: broadcast::Sender<ListUpdateEvent>,
    /// 新消息事件发送器
    new_message_tx: broadcast::Sender<NewMessageEvent>,
    /// 新文件事件发送器
    new_file_tx: broadcast::Sender<NewFileEvent>,
    /// 文件传输进度事件发送器
    file_progress_tx: broadcast::Sender<FileTransferProgressEvent>,
    /// 系统通知事件发送器
    system_notification_tx: broadcast::Sender<SystemNotificationEvent>,
}

impl TauriEventChannel {
    /// 创建新的事件通道
    pub fn new() -> Self {
        let (network_sync_tx, _) = broadcast::channel(100);
        let (list_update_tx, _) = broadcast::channel(100);
        let (new_message_tx, _) = broadcast::channel(100);
        let (new_file_tx, _) = broadcast::channel(100);
        let (file_progress_tx, _) = broadcast::channel(100);
        let (system_notification_tx, _) = broadcast::channel(100);

        Self {
            network_sync_tx,
            list_update_tx,
            new_message_tx,
            new_file_tx,
            file_progress_tx,
            system_notification_tx,
        }
    }

    /// 发送网络同步事件
    pub fn emit_network_sync(&self, event: NetworkSyncEvent) -> Result<(), String> {
        self.network_sync_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit network sync event: {}", e))
    }

    /// 订阅网络同步事件
    pub fn subscribe_network_sync(&self) -> broadcast::Receiver<NetworkSyncEvent> {
        self.network_sync_tx.subscribe()
    }

    /// 发送列表更新事件
    pub fn emit_list_update(&self, event: ListUpdateEvent) -> Result<(), String> {
        self.list_update_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit list update event: {}", e))
    }

    /// 订阅列表更新事件
    pub fn subscribe_list_update(&self) -> broadcast::Receiver<ListUpdateEvent> {
        self.list_update_tx.subscribe()
    }

    /// 发送新消息事件
    pub fn emit_new_message(&self, event: NewMessageEvent) -> Result<(), String> {
        self.new_message_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit new message event: {}", e))
    }

    /// 订阅新消息事件
    pub fn subscribe_new_message(&self) -> broadcast::Receiver<NewMessageEvent> {
        self.new_message_tx.subscribe()
    }

    /// 发送新文件事件
    pub fn emit_new_file(&self, event: NewFileEvent) -> Result<(), String> {
        self.new_file_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit new file event: {}", e))
    }

    /// 订阅新文件事件
    pub fn subscribe_new_file(&self) -> broadcast::Receiver<NewFileEvent> {
        self.new_file_tx.subscribe()
    }

    /// 发送文件传输进度事件
    pub fn emit_file_progress(&self, event: FileTransferProgressEvent) -> Result<(), String> {
        self.file_progress_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit file progress event: {}", e))
    }

    /// 订阅文件传输进度事件
    pub fn subscribe_file_progress(&self) -> broadcast::Receiver<FileTransferProgressEvent> {
        self.file_progress_tx.subscribe()
    }

    /// 发送系统通知事件
    pub fn emit_system_notification(&self, event: SystemNotificationEvent) -> Result<(), String> {
        self.system_notification_tx
            .send(event)
            .map(|_| ())
            .map_err(|e| format!("Failed to emit system notification event: {}", e))
    }

    /// 订阅系统通知事件
    pub fn subscribe_system_notification(&self) -> broadcast::Receiver<SystemNotificationEvent> {
        self.system_notification_tx.subscribe()
    }
}

impl Default for TauriEventChannel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Command Handlers (前端调用的命令处理器)
// ============================================================================

/// 命令处理器特质
pub trait CommandHandler: Send + Sync {
    /// 处理配置更新
    fn handle_config_update(&self, request: ConfigUpdateRequest) -> ApiResult<()>;

    /// 处理热重载
    fn handle_hot_reload(&self, request: HotReloadRequest) -> ApiResult<()>;

    /// 处理程序启动
    fn handle_startup(&self, config: StartupConfig) -> ApiResult<()>;

    /// 处理发送消息
    fn handle_send_message(&self, request: SendMessageRequest) -> ApiResult<String>;

    /// 处理接收消息
    fn handle_receive_messages(&self) -> ApiResult<Vec<ReceiveMessageResponse>>;

    /// 处理文件传输
    fn handle_file_transfer(&self, request: FileTransferRequest) -> ApiResult<String>;

    /// 处理剪切板操作
    fn handle_clipboard_operation(&self, request: ClipboardRequest) -> ApiResult<Option<String>>;

    /// 获取设备列表
    fn handle_get_devices(&self) -> ApiResult<Vec<serde_json::Value>>;

    /// 获取系统状态
    fn handle_get_system_status(&self) -> ApiResult<serde_json::Value>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_channel_creation() {
        let channel = TauriEventChannel::new();
        
        // 测试订阅
        let _rx1 = channel.subscribe_network_sync();
        let _rx2 = channel.subscribe_list_update();
        let _rx3 = channel.subscribe_new_message();
        let _rx4 = channel.subscribe_new_file();
    }

    #[test]
    fn test_network_sync_event() {
        let channel = TauriEventChannel::new();
        let mut rx = channel.subscribe_network_sync();

        let event = NetworkSyncEvent {
            sync_type: NetworkSyncType::DeviceOnline,
            device_id: "test_device".to_string(),
            data: serde_json::json!({"status": "online"}),
            timestamp: 12345,
        };

        channel.emit_network_sync(event.clone()).unwrap();
        
        let received = rx.try_recv().unwrap();
        assert_eq!(received.device_id, "test_device");
    }

    #[test]
    fn test_new_message_event() {
        let channel = TauriEventChannel::new();
        let mut rx = channel.subscribe_new_message();

        let event = NewMessageEvent {
            message_id: "msg123".to_string(),
            from_device: "device1".to_string(),
            from_device_name: "Device One".to_string(),
            content: "Hello".to_string(),
            message_type: MessageType::Private,
            timestamp: 67890,
            is_read: false,
        };

        channel.emit_new_message(event.clone()).unwrap();
        
        let received = rx.try_recv().unwrap();
        assert_eq!(received.message_id, "msg123");
        assert_eq!(received.content, "Hello");
    }

    #[test]
    fn test_list_update_event() {
        let channel = TauriEventChannel::new();
        let mut rx = channel.subscribe_list_update();

        let event = ListUpdateEvent {
            list_type: ListType::Devices,
            operation: ListOperation::Add,
            item: serde_json::json!({"device_id": "new_device"}),
        };

        channel.emit_list_update(event.clone()).unwrap();
        
        let received = rx.try_recv().unwrap();
        assert!(matches!(received.list_type, ListType::Devices));
        assert!(matches!(received.operation, ListOperation::Add));
    }

    #[test]
    fn test_serialization() {
        let msg_req = SendMessageRequest {
            target_device: Some("device1".to_string()),
            content: "Test message".to_string(),
            message_type: MessageType::Private,
        };

        let json = serde_json::to_string(&msg_req).unwrap();
        let deserialized: SendMessageRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.content, "Test message");
    }
}
