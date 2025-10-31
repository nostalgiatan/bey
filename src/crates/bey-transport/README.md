# BEY Transport Module (bey-transport)

BEY 传输模块 - 基于QUIC的安全高性能传输层

## 概述

`bey-transport` 是 BEY 生态系统的底层传输模块，基于 Quinn (QUIC协议的Rust实现) 提供安全、可靠、高性能的网络传输能力。

## 核心特性

### 🚀 高性能

- **QUIC协议**: 现代化的传输协议，优于TCP
- **0-RTT连接**: 快速连接建立
- **多路复用**: 单连接支持多个独立流
- **拥塞控制**: 内置高效的拥塞控制算法

### 🔒 安全性

- **TLS 1.3**: 强制使用最新的TLS版本
- **证书验证**: 完整的证书链验证
- **前向安全**: 确保历史通信安全
- **加密传输**: 所有数据默认加密

### 📦 可靠性

- **自动重传**: 丢包自动重传
- **流量控制**: 防止接收端过载
- **有序传输**: 保证数据按序到达
- **连接迁移**: 支持网络切换

## 快速开始

### 创建服务器

```rust
use bey_transport::{SecureTransport, TransportConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TransportConfig::default();
    let transport = SecureTransport::new(config).await?;
    
    // 绑定并监听
    transport.bind("0.0.0.0:8080".parse()?).await?;
    
    // 等待连接
    let connection = transport.accept().await?;
    println!("新连接建立");
    
    Ok(())
}
```

### 创建客户端

```rust
use bey_transport::{SecureTransport, TransportConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TransportConfig::default();
    let transport = SecureTransport::new(config).await?;
    
    // 连接到服务器
    let connection = transport.connect("192.168.1.100:8080".parse()?).await?;
    println!("连接成功");
    
    Ok(())
}
```

### 发送和接收数据

```rust
// 发送数据
let stream = connection.open_stream().await?;
stream.send(&data).await?;

// 接收数据
let data = stream.receive().await?;
```

## 配置选项

```rust
use bey_transport::TransportConfig;
use std::time::Duration;

let config = TransportConfig {
    // 连接超时
    connect_timeout: Duration::from_secs(10),
    
    // 空闲超时
    idle_timeout: Duration::from_secs(60),
    
    // 保活间隔
    keep_alive_interval: Some(Duration::from_secs(30)),
    
    // 最大并发流数
    max_concurrent_streams: 100,
    
    // 流接收窗口
    stream_receive_window: 1_048_576,  // 1MB
    
    // 连接接收窗口
    connection_receive_window: 10_485_760,  // 10MB
    
    ..Default::default()
};
```

## 策略系统

bey-transport 内置完整的策略系统，用于访问控制和流量管理。

### 策略类型

- **IP白名单/黑名单**: 基于IP地址的访问控制
- **速率限制**: 限制连接速率和数据速率
- **并发控制**: 限制并发连接数
- **流量配额**: 设置流量上限

### 使用策略

```rust
use bey_transport::{Policy, PolicyAction};

// 创建IP白名单策略
let policy = Policy::ip_whitelist(vec![
    "192.168.1.0/24".parse()?,
    "10.0.0.0/8".parse()?,
]);

// 应用策略
transport.add_policy(policy).await?;

// 创建速率限制策略
let rate_limit = Policy::rate_limit(
    1000,  // 每秒最多1000个连接
    100_000_000,  // 每秒最多100MB
);

transport.add_policy(rate_limit).await?;
```

## 性能调优

### 缓冲区大小

```rust
config.stream_receive_window = 2_097_152;  // 2MB
config.connection_receive_window = 20_971_520;  // 20MB
```

### 拥塞控制算法

```rust
// 可选：BBR, NewReno, Cubic
config.congestion_controller = CongestionController::BBR;
```

### 并发设置

```rust
config.max_concurrent_streams = 1000;
config.max_concurrent_connections = 10000;
```

## 错误处理

```rust
use bey_transport::TransportError;

match transport.connect(addr).await {
    Ok(conn) => println!("连接成功"),
    Err(TransportError::Timeout) => println!("连接超时"),
    Err(TransportError::ConnectionRefused) => println!("连接被拒绝"),
    Err(TransportError::CertificateError(e)) => println!("证书错误: {}", e),
    Err(e) => println!("其他错误: {}", e),
}
```

## 技术细节

- **协议**: QUIC (RFC 9000)
- **TLS版本**: TLS 1.3
- **UDP端口**: 可配置（默认8080）
- **连接迁移**: 支持
- **0-RTT**: 支持
- **多路复用**: 是

## 性能指标

在测试环境中：
- **吞吐量**: 可达 1+ Gbps
- **延迟**: 低至 <1ms (局域网)
- **并发连接**: 支持 10,000+ 连接
- **CPU效率**: 相比TCP+TLS更高

## API 文档

完整的API文档可通过以下命令查看：

```bash
cargo doc --package bey-transport --open
```

## 依赖项

```toml
[dependencies]
quinn = "0.11"
rustls = "0.23"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
thiserror = "2"
```

## 与 bey-net 的关系

`bey-transport` 是底层传输，`bey-net` 构建在其之上：

- `bey-transport`: QUIC/UDP层，提供可靠传输
- `bey-net`: 应用层，提供令牌、状态机、认证、加密等

大多数情况下，应用应该直接使用 `bey-net`，它提供更高级别的抽象。

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

- `bey-net`: 应用层网络框架（推荐使用）
- `bey-identity`: 身份认证和证书管理
- `quinn`: 底层QUIC实现

## 联系方式

如有问题或建议，请在GitHub提交Issue。
