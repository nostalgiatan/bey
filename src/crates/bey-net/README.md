# BEY Network Module (bey-net)

BEY 网络模块 - 完整的、生产就绪的网络通信框架

## 概述

`bey-net` 是 BEY 生态系统的核心网络模块，提供完整的网络通信能力。它采用现代化的架构设计，将复杂的网络操作封装在简单易用的高层API中。

## 核心特性

### 🚀 高性能

- **零拷贝设计**: 令牌直接序列化，无额外复制
- **异步非阻塞**: 全异步架构，基于tokio
- **流量控制**: TCP友好的拥塞控制（慢启动、拥塞避免、快速恢复）
- **智能调度**: 基于优先级的令牌调度
- **并行传输**: 大文件流水线并行传输

### 🔒 安全性

- **证书认证**: 基于 bey-identity 的完整证书验证
- **AES-256-GCM加密**: 自动令牌加密/解密
- **密钥派生**: 从证书安全派生主密钥
- **密码学安全随机数**: 使用OsRng生成Nonce

### 🎯 简单易用

- **完全简化的API**: 其他模块只需简单调用
- **自动化管理**: 加密、优先级、流量控制全自动
- **零配置**: 开箱即用的默认配置
- **后台任务**: 自动维护设备列表、超时处理

### 📊 可观测性

- **性能监控**: 实时吞吐量、延迟统计
- **延迟分析**: 百分位延迟（p50, p90, p95, p99）
- **错误追踪**: 详细的错误分类和统计
- **资源监控**: 连接数、流数、队列大小

### 🔄 可靠性

- **自动重传**: 超时自动重试（可配置）
- **确认机制**: 令牌确认（requires_ack）
- **流式传输**: 大文件自动分块和重组
- **设备发现**: mDNS自动发现和维护

## 架构组件

### 1. 令牌系统 (`token.rs`)

定义网络传输的基本单位，支持优先级和确认机制。

```rust
use bey_net::{Token, TokenMeta, TokenPriority};

// 创建令牌
let meta = TokenMeta::new("message".to_string(), "sender".to_string())
    .with_priority(TokenPriority::High)
    .with_ack(true);
let token = Token::new(meta, data);
```

### 2. 有限状态机 (`state_machine.rs`)

管理连接生命周期的9种状态转换。

```rust
Idle → Connecting → Connected → Authenticating → 
Authenticated → Transferring → ...
```

### 3. 流式传输 (`stream.rs`)

大文件自动分块和流式传输。

```rust
// 自动分块（默认64KB）
let chunks = stream_manager.create_send_stream(
    stream_id,
    large_data,
    "file".to_string()
).await?;
```

### 4. 优先级队列 (`priority_queue.rs`)

基于二叉堆的优先级令牌调度。

```rust
// 自动按优先级排序: Critical > High > Normal > Low
priority_queue.enqueue(token).await?;
let next_token = priority_queue.dequeue().await?;
```

### 5. 流量控制 (`flow_control.rs`)

TCP友好的拥塞控制算法。

```rust
// 自动流量控制
if flow_controller.can_send(size).await {
    flow_controller.on_send(size).await?;
    // 发送数据
}
```

### 6. 性能监控 (`metrics.rs`)

全面的性能指标收集。

```rust
// 自动收集指标
metrics.record_send(bytes).await;
metrics.record_rtt(duration).await;
let stats = metrics.get_metrics().await;
```

### 7. 传输引擎 (`engine.rs`)

集成所有组件的核心引擎，提供简化的高层API。

## 快速开始

### 基本使用（推荐方式 - 使用消息处理器）

从版本 0.1.0 开始，推荐使用消息处理器模式而不是手动调用 `receive()`：

```rust
use bey_net::{TransportEngine, EngineConfig, TokenHandler, Token, NetResult};
use std::sync::Arc;

// 定义消息处理器
struct MyMessageHandler;

#[async_trait::async_trait]
impl TokenHandler for MyMessageHandler {
    fn token_types(&self) -> Vec<String> {
        vec!["chat_message".to_string(), "notification".to_string()]
    }
    
    async fn handle_token(&self, token: Token) -> NetResult<Option<Token>> {
        println!("收到消息: {} 来自 {}", 
            token.meta.token_type, 
            token.meta.sender_id);
        
        // 处理消息
        // ...
        
        Ok(None)  // 或返回响应令牌
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建引擎
    let config = EngineConfig::default();
    let engine = TransportEngine::new(config).await?;
    
    // 注册消息处理器（引擎会自动接收并路由消息）
    engine.register_handler(Arc::new(MyMessageHandler)).await?;
    
    // 启动服务器（自动开始接收消息）
    engine.start_server().await?;
    
    // 发送消息（自动：加密、优先级、流量控制）
    engine.send_to("device-name", data, "chat_message").await?;
    
    // 引擎会自动接收消息并调用注册的处理器
    // 不需要手动调用 receive()
    
    Ok(())
}
```

### 传统方式（已废弃，但仍可用）

⚠️ **注意**: `receive()` API 已废弃，推荐使用上述消息处理器模式。

```rust,ignore
// 已废弃：不推荐使用
#[allow(deprecated)]
if let Some((sender, msg_type, data)) = engine.receive().await? {
    println!("收到来自 {}: {}", sender, msg_type);
}
```

### 大文件传输

```rust
// 自动分块、流式传输
let stream_id = engine.send_large_file(
    "device-name",
    large_file_data,
    "file"
).await?;
```

### 群发消息

```rust
// 发送到指定的多个设备
engine.send_to_group(
    vec!["device1", "device2", "device3"],
    data,
    "group_message"
).await?;

// 发送到特定组的所有成员
engine.send_to_group_by_name(
    "team-alpha",
    data,
    "team_message"
).await?;

// 广播到所有设备
engine.broadcast(data, "broadcast").await?;
```

### 性能监控

```rust
// 获取性能统计
let stats = engine.get_performance_stats().await;
println!("发送速率: {:.2} MB/s", stats.send_rate / 1_048_576.0);

// 打印详细摘要
engine.print_performance_summary().await;
```

## 配置选项

```rust
use std::time::Duration;

let config = EngineConfig {
    // 基本配置
    name: "my-device".to_string(),
    port: 8080,
    enable_auth: true,
    enable_encryption: true,
    enable_mdns: true,
    
    // 优先级队列配置
    ack_timeout: Duration::from_secs(5),
    max_retries: 3,
    
    // 流量控制配置
    initial_window: 65536,      // 64KB
    max_window: 1048576,        // 1MB
    
    // 流配置
    stream_chunk_size: 65536,   // 64KB
    
    ..Default::default()
};
```

## API 文档

完整的API文档可通过以下命令查看：

```bash
cargo doc --package bey-net --open
```

## 技术细节

- **mDNS服务类型**: `_bey._tcp.local`
- **设备发现间隔**: 15秒
- **设备过期时间**: 30秒
- **加密算法**: AES-256-GCM
- **密钥派生**: SHA-256(证书PEM || 引擎名称)
- **Nonce生成**: OsRng (密码学安全)
- **确认超时**: 5秒（可配置）
- **最大重试**: 3次（可配置）
- **默认块大小**: 64KB（可配置）

## 性能指标

在测试环境中：
- **吞吐量**: 可达 100+ MB/s
- **延迟**: p99 < 10ms (局域网)
- **并发连接**: 支持 1000+ 连接
- **内存效率**: 零拷贝设计，最小内存占用

## 内存和性能优化

### 零拷贝设计

引擎默认启用零拷贝优化，避免不必要的数据复制：

```rust
let config = EngineConfig {
    enable_zero_copy: true,  // 默认启用
    ..Default::default()
};
```

### 内存池

引擎使用对象池来复用令牌对象，减少内存分配：

```rust
let config = EngineConfig {
    token_pool_size: 100,  // 预分配100个令牌槽位
    ..Default::default()
};
```

### 批量处理

使用批量接收来提高吞吐量：

```rust
// 批量接收令牌
let tokens = receiver.receive_batch(100, ReceiverMode::NonBlocking).await?;
```

### 流控制调优

根据网络条件调整窗口大小：

```rust
let config = EngineConfig {
    initial_window: 131072,      // 128KB (高速网络)
    max_window: 2097152,         // 2MB (高速网络)
    ..Default::default()
};
```

### 块大小优化

根据文件大小和网络条件调整块大小：

```rust
let config = EngineConfig {
    stream_chunk_size: 131072,  // 128KB (大文件传输)
    ..Default::default()
};
```

### 后台任务间隔

调整后台任务间隔来平衡性能和开销：

- 优先级队列检查: 1秒（固定）
- 指标更新: 5秒（可在代码中调整）
- 设备发现: 15秒（可在mDNS配置中调整）

### 性能监控

使用内置监控来识别瓶颈：

```rust
let stats = engine.get_performance_stats().await;
let fc_stats = engine.get_flow_control_stats().await;

// 检查是否有流量控制瓶颈
if fc_stats.bytes_in_flight >= fc_stats.congestion_window {
    println!("警告：达到拥塞窗口限制");
}

// 检查重传率
if stats.retransmit_count > stats.tokens_sent / 10 {
    println!("警告：高重传率，可能存在网络问题");
}
```

## 依赖项

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
error = { path = "../error" }
bey-transport = { path = "../bey-transport" }
bey-identity = { path = "../bey-identity" }
aes-gcm = "0.10"
sha2 = "0.10"
base64 = "0.22"
uuid = { version = "1", features = ["v4"] }
mdns = "3.0"
```

## 贡献指南

欢迎贡献！请遵循以下原则：

1. **测试驱动**: 所有代码必须有测试
2. **无警告**: 编译不得产生警告
3. **中文注释**: 所有注释和文档必须是中文
4. **禁用unsafe**: 除非绝对必要，否则禁止使用unsafe
5. **错误处理**: 禁止unwrap()，必须正确处理错误

## 许可证

本项目遵循项目根目录的许可证。

## 相关模块

- `bey-transport`: QUIC传输层
- `bey-identity`: 身份认证和证书管理
- ~~`bey-file-transfer`~~: 已移除，文件传输功能已集成到 bey-net
- `bey-storage`: 存储服务（使用bey-net进行文件传输）

## API 变更历史

### v0.1.0

**重大变更：消息接收模式重构**

- ✅ **新增**: 自动接收循环 - 引擎启动后自动在后台接收消息
- ✅ **新增**: `register_handler()` API - 推荐的消息处理方式
- ⚠️ **废弃**: `receive()` 和 `receive_blocking()` - 仍可用但不推荐
- 📝 **迁移指南**: 参见 "快速开始" 部分的示例代码

**原因**: 手动调用 `receive()` 的模式不符合事件驱动架构。新的处理器模式：
- 更符合Rust异步编程习惯
- 自动化处理，减少开发者负担
- 支持多种消息类型的独立处理
- 更好的性能和扩展性

**文件传输模块移除**

- ❌ **移除**: `bey-file-transfer` 模块
- ✅ **原因**: 功能已被 bey-net 完全覆盖
- ✅ **替代方案**: 使用 `send_large_file()` 和 `receive_large_file()`

## 联系方式

如有问题或建议，请在GitHub提交Issue。
