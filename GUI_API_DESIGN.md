# BEY GUI API 设计文档

本文档描述了 BEY 项目中 Tauri GUI 的 API 架构和双向通信机制。

## 概览

BEY GUI 使用 Tauri 框架实现，提供了前端与后端之间的双向通信能力：

- **前端 → 后端**: 通过 Tauri Commands 调用后端功能
- **后端 → 前端**: 通过 Tauri Events 推送实时更新

## 架构图

```text
┌─────────────────────────────────────────────────────────┐
│                    Tauri Frontend                        │
│  ┌──────────────┐         ┌──────────────┐             │
│  │   Commands   │         │    Events    │             │
│  │  (F -> B)    │         │   (B -> F)   │             │
│  └──────┬───────┘         └──────▲───────┘             │
└─────────┼─────────────────────────┼─────────────────────┘
          │                         │
          ▼                         │
┌─────────────────────────────────────────────────────────┐
│                   Tauri API Layer                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │  src/tauri_api.rs                                │  │
│  │  - TauriEventChannel (事件通道)                  │  │
│  │  - Commands (前端调用后端)                       │  │
│  │  - Events (后端推送前端)                         │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │  src/crates/bey-gui/src/gui/                     │  │
│  │  - commands.rs (命令处理器)                      │  │
│  │  - events.rs (事件发射器)                        │  │
│  │  - state.rs (状态管理)                           │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
          │                         ▲
          │                         │
          ▼                         │
┌─────────────────────────────────────────────────────────┐
│                  BEY Backend Core                        │
│  - BeyFuncManager (功能管理器)                          │
│  - Network Layer (网络层)                               │
│  - Storage Layer (存储层)                               │
└─────────────────────────────────────────────────────────┘
```

## API 定义

### 前端 → 后端 Commands

#### 1. 配置管理

**update_config** - 更新配置
```rust
ConfigUpdateRequest {
    key: String,        // 配置键
    value: Value,       // 配置值
}
```

**hot_reload** - 热重载
```rust
HotReloadRequest {
    reload_type: String,    // "config" | "plugin" | "theme"
    target: Option<String>, // 可选目标路径
}
```

#### 2. 应用启动

**startup** - 程序启动
```rust
StartupConfig {
    device_name: String,        // 设备名称
    storage_path: String,       // 存储路径
    port: Option<u16>,          // 网络端口
    auto_discovery: bool,       // 是否自动发现
}
```

#### 3. 消息传递

**send_message** - 发送消息
```rust
SendMessageRequest {
    target_device: Option<String>,  // 目标设备 (None=广播)
    content: String,                // 消息内容
    message_type: MessageType,      // Private | Group | Broadcast
}
```

**receive_messages** - 接收消息
```rust
Response: Vec<ReceiveMessageResponse> {
    message_id: String,
    from_device: String,
    content: String,
    message_type: MessageType,
    timestamp: i64,
}
```

#### 4. 文件传输

**transfer_file** - 传输文件
```rust
FileTransferRequest {
    target_device: String,      // 目标设备
    file_path: String,          // 文件路径
    file_size: u64,             // 文件大小
}
```

#### 5. 剪切板操作

**clipboard_operation** - 剪切板操作
```rust
ClipboardRequest {
    operation: ClipboardOperation,  // Add | Remove | Sync | Get
    data: Option<String>,           // 剪切板数据
}
```

#### 6. 设备管理

**get_devices** - 获取设备列表
```rust
Response: Vec<DeviceInfo>
```

**get_system_status** - 获取系统状态
```rust
Response: SystemStatus {
    status: String,
    uptime: u64,
    connections: u32,
    ...
}
```

### 后端 → 前端 Events

#### 1. 网络同步事件 (network-sync)

```rust
NetworkSyncEvent {
    sync_type: NetworkSyncType,     // DeviceOnline | DeviceOffline | ...
    device_id: String,
    data: Value,
    timestamp: i64,
}
```

**事件类型:**
- `DeviceOnline` - 设备上线
- `DeviceOffline` - 设备下线
- `NetworkStatusChanged` - 网络状态变化
- `ConnectionEstablished` - 连接建立
- `ConnectionLost` - 连接断开

#### 2. 列表更新事件 (list-update)

```rust
ListUpdateEvent {
    list_type: ListType,        // Devices | Messages | Files | Clipboard
    operation: ListOperation,   // Add | Update | Remove | Clear
    item: Value,
}
```

#### 3. 新消息事件 (new-message)

```rust
NewMessageEvent {
    message_id: String,
    from_device: String,
    from_device_name: String,
    content: String,
    message_type: MessageType,
    timestamp: i64,
    is_read: bool,
}
```

#### 4. 新文件事件 (new-file)

```rust
NewFileEvent {
    file_id: String,
    file_name: String,
    file_size: u64,
    from_device: String,
    from_device_name: String,
    file_type: String,
    timestamp: i64,
    progress: u8,
}
```

#### 5. 文件传输进度事件 (file-progress)

```rust
FileTransferProgressEvent {
    transfer_id: String,
    file_name: String,
    transferred_bytes: u64,
    total_bytes: u64,
    progress: u8,           // 0-100
    speed: u64,             // bytes/sec
}
```

#### 6. 系统通知事件 (system-notification)

```rust
SystemNotificationEvent {
    notification_type: NotificationType,  // Info | Warning | Error | Success
    title: String,
    message: String,
    timestamp: i64,
}
```

## 事件通道系统

`TauriEventChannel` 提供了类型安全的事件发送和订阅机制：

```rust
pub struct TauriEventChannel {
    // 各种事件的 broadcast channel
    network_sync_tx: broadcast::Sender<NetworkSyncEvent>,
    list_update_tx: broadcast::Sender<ListUpdateEvent>,
    new_message_tx: broadcast::Sender<NewMessageEvent>,
    new_file_tx: broadcast::Sender<NewFileEvent>,
    file_progress_tx: broadcast::Sender<FileTransferProgressEvent>,
    system_notification_tx: broadcast::Sender<SystemNotificationEvent>,
}
```

### 发送事件

```rust
let channel = TauriEventChannel::new();

// 发送网络同步事件
channel.emit_network_sync(NetworkSyncEvent { ... })?;

// 发送新消息事件
channel.emit_new_message(NewMessageEvent { ... })?;

// 发送文件进度事件
channel.emit_file_progress(FileTransferProgressEvent { ... })?;
```

### 订阅事件

```rust
// 订阅网络同步事件
let mut rx = channel.subscribe_network_sync();

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        // 处理事件
        println!("收到网络同步事件: {:?}", event);
    }
});
```

## GUI 模块结构

### gui/commands.rs

实现 `TauriCommandHandler`，处理所有前端命令：

```rust
pub struct TauriCommandHandler {
    func_manager: Arc<BeyFuncManager>,
}

impl TauriCommandHandler {
    pub async fn update_config(&self, ...) -> Result<(), String>;
    pub async fn hot_reload(&self, ...) -> Result<(), String>;
    pub async fn send_message(&self, ...) -> Result<String, String>;
    pub async fn transfer_file(&self, ...) -> Result<String, String>;
    // ... 更多命令处理方法
}
```

### gui/events.rs

实现 `EventEmitter`，发送事件到前端：

```rust
pub struct EventEmitter {
    app_handle: Arc<AppHandle>,
}

impl EventEmitter {
    pub fn emit_network_sync(&self, ...) -> Result<(), String>;
    pub fn emit_device_online(&self, ...) -> Result<(), String>;
    pub fn emit_new_message(&self, ...) -> Result<(), String>;
    pub fn emit_file_progress(&self, ...) -> Result<(), String>;
    // ... 更多事件发送方法
}
```

### gui/state.rs

管理 GUI 应用状态：

```rust
pub struct GuiState {
    func_manager: Arc<BeyFuncManager>,
    command_handler: Arc<TauriCommandHandler>,
    event_emitter: Option<Arc<EventEmitter>>,
    config: Arc<RwLock<GuiConfig>>,
}

impl GuiState {
    pub async fn start_background_tasks(&self);
    // 启动后台监听任务，将后端事件转发到前端
}
```

## 实现示例

### 前端调用后端 (JavaScript/TypeScript)

```javascript
import { invoke } from '@tauri-apps/api/tauri'

// 发送消息
const messageId = await invoke('send_message', {
  targetDevice: 'device-123',
  content: 'Hello!',
  messageType: 'private'
})

// 获取设备列表
const devices = await invoke('get_devices')

// 更新配置
await invoke('update_config', {
  key: 'theme',
  value: 'dark'
})
```

### 前端监听后端事件 (JavaScript/TypeScript)

```javascript
import { listen } from '@tauri-apps/api/event'

// 监听新消息
await listen('new-message', (event) => {
  console.log('收到新消息:', event.payload)
  // {
  //   message_id: "msg-123",
  //   from_device: "device-456",
  //   content: "Hello!",
  //   ...
  // }
})

// 监听网络同步
await listen('network-sync', (event) => {
  console.log('网络同步:', event.payload)
})

// 监听文件传输进度
await listen('file-progress', (event) => {
  const { progress, file_name } = event.payload
  console.log(`${file_name} 传输进度: ${progress}%`)
})
```

### 后端发送事件 (Rust)

```rust
use bey_gui::gui::{EventEmitter, GuiState};

// 在后台任务中发送事件
async fn background_task(emitter: Arc<EventEmitter>) {
    // 发送设备上线事件
    emitter.emit_device_online("device-123", "My Device").ok();
    
    // 发送新消息通知
    emitter.emit_new_message(
        "msg-456",
        "device-789",
        "Friend's Device",
        "Hello from backend!",
        "private"
    ).ok();
    
    // 发送文件传输进度
    emitter.emit_file_progress(
        "transfer-001",
        "document.pdf",
        500_000,      // 已传输字节
        1_000_000,    // 总字节
        100_000,      // 速度 bytes/sec
    ).ok();
}
```

## 完成 GUI 实现的后续步骤

1. **创建 Tauri 配置文件** (`tauri.conf.json`)
   - 定义应用窗口属性
   - 配置权限和安全策略

2. **开发前端界面**
   - 使用 Vue/React/Svelte 等框架
   - 实现 UI 组件和交互逻辑
   - 集成 Tauri API 调用

3. **注册 Tauri 命令处理器**
   - 在 `GuiApp::run()` 中注册所有命令
   - 使用 `tauri::generate_handler![]` 宏

4. **实现事件监听和分发**
   - 在应用启动时设置事件监听
   - 将后端事件转发到前端

5. **测试和优化**
   - 编写集成测试
   - 性能优化
   - 错误处理改进

## 总结

本 API 设计提供了完整的双向通信能力：

✅ **前端调用后端** - 通过 Commands 实现配置管理、消息发送、文件传输等功能
✅ **后端推送前端** - 通过 Events 实现实时通知、列表更新、进度反馈等功能
✅ **类型安全** - 使用 Rust 类型系统确保 API 的类型安全
✅ **可扩展** - 易于添加新的命令和事件类型
✅ **解耦设计** - 清晰的模块划分，便于维护和测试
