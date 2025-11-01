# BEY 分布式功能模块 (bey-func)

BEY 分布式功能模块提供了集成网络、存储、消息和剪切板的高级API，基于 Token 元类和接收器模块实现分布式服务。

## 功能特性

### 🚀 核心功能

1. **消息系统** (`message_func`)
   - 私信（点对点）
   - 群聊消息
   - 广播消息
   - 基于 Token 的消息路由

2. **剪切板同步** (`clipboard_func`)
   - 添加/删除剪切板内容
   - 差异同步
   - 群组同步
   - 点对点同步
   - 标记来源设备 DNS ID

3. **存储功能** (`storage_func`)
   - 云存储上传/下载
   - 点对点文件传输
   - 大文件流式传输
   - 云存储更新通知

### 📐 架构设计

```text
┌──────────────────────────────────────────────────────────┐
│                    BEY 分布式功能层                       │
├──────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ 消息功能      │  │ 剪切板功能    │  │ 存储功能      │  │
│  │ MessageFunc  │  │ ClipboardFunc│  │ StorageFunc  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│          ↓                 ↓                 ↓           │
│  ┌────────────────────────────────────────────────────┐ │
│  │         BeyFuncManager (统一管理器)                 │ │
│  └────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────┐
│              BEY 网络层 (bey-net)                         │
│  - Token 接收器和路由                                     │
│  - TransportEngine (发送/接收)                           │
└──────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────┐
│              BEY 存储层 (bey-storage)                     │
│  - 对象存储、云存储                                        │
│  - 剪切板、消息持久化                                      │
└──────────────────────────────────────────────────────────┘
```

## 快速开始

### 创建管理器

```rust
use bey_func::BeyFuncManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建分布式功能管理器
    let manager = BeyFuncManager::new("my_device", "./storage").await?;

    // 启动网络服务（自动注册所有处理器）
    manager.start().await?;

    Ok(())
}
```

### 消息功能

```rust
// 发送私信
let msg_id = manager.send_private_message("peer_device", b"Hello!").await?;

// 发送群聊消息
let msg_id = manager.send_group_message("group1", b"Hi everyone!").await?;

// 广播消息
let count = manager.broadcast_message(b"Important announcement").await?;
```

### 剪切板同步

```rust
// 添加剪切板内容
let entry_id = manager.add_clipboard("text", b"clipboard content").await?;

// 同步到对等设备
manager.sync_clipboard_to_peer("peer_device").await?;

// 同步到群组
manager.sync_clipboard_to_group("group1").await?;

// 发送差异更新
manager.clipboard.send_diff_to_peer("peer_device", timestamp).await?;
```

### 存储功能

```rust
// 上传到云存储
let file_hash = manager.upload_to_cloud("document.txt", &data).await?;

// 从云存储下载
let data = manager.download_from_cloud(&file_hash).await?;

// 发送文件到对等设备
manager.send_file_to_peer("peer_device", "file.txt", &data).await?;

// 发送大文件
let stream_id = manager.storage_func
    .send_large_file_to_peer("peer_device", "large.bin", &data).await?;
```

## Token 类型

### 消息令牌

- `bey.message.private` - 私信
- `bey.message.group` - 群聊
- `bey.message.broadcast` - 广播

### 剪切板令牌

- `bey.clipboard.add` - 添加条目
- `bey.clipboard.delete` - 删除条目
- `bey.clipboard.sync` - 完整同步
- `bey.clipboard.diff` - 差异同步

### 存储令牌

- `bey.storage.file` - 文件传输
- `bey.storage.cloud.upload` - 云存储上传
- `bey.storage.cloud.download` - 云存储下载
- `bey.storage.cloud.notify` - 云存储通知

## 自动处理

所有 Token 类型都会自动注册处理器，当接收到相应的令牌时：

1. **消息接收** - 自动保存到本地消息数据库
2. **剪切板同步** - 自动合并到本地剪切板
3. **文件接收** - 自动保存到对象存储
4. **云存储通知** - 自动记录更新信息

## API 文档

完整的 API 文档可以通过以下命令查看：

```bash
cargo doc --package bey-func --open
```

## 依赖关系

- `bey-net` - 网络传输和 Token 路由
- `bey-storage` - 数据持久化
- `error` - 错误处理
- `tokio` - 异步运行时
- `serde` - 序列化/反序列化

## 设计原则

1. **高级抽象** - 提供简单易用的高级 API
2. **自动化** - 自动注册处理器和路由
3. **分布式** - 所有功能都支持分布式场景
4. **类型安全** - 使用 Token 元类确保类型安全
5. **性能优化** - 无 unwrap()，错误处理完善

## 示例程序

查看 `examples/` 目录获取更多使用示例。

## 测试

运行单元测试：

```bash
cargo test --package bey-func
```

## 贡献

欢迎贡献！请遵循项目的代码规范：

1. 使用中文注释和文档
2. 遵循测试驱动原则
3. 禁止使用 unwrap()
4. 使用 error 模块处理错误

## 许可证

本项目遵循项目根目录的许可证。
