# BEY - 局域网中心项目

一个去中心化的局域网协作平台，提供对象剪切板、文件传输、消息传递、权限控制、分布式云空间、证书管理和传输优先级控制功能。

## 🚀 核心特性

- **去中心化架构**: 无需中央服务器，提高可靠性和隐私性
- **局域网优化**: 低延迟、高速度的本地传输
- **多功能集成**: 剪切板、文件传输、消息推送一体化
- **资源贡献**: 利用闲置磁盘空间创建分布式存储
- **权限管理**: 基于证书的细粒度权限控制
- **极致性能**: 零隐式转换，内存安全优化

## 📁 项目结构

```
bey/
├── src/
│   ├── lib.rs                    # 核心库
│   ├── main.rs                   # 主程序入口
│   ├── bin/
│   │   ├── demo.rs              # 完整演示程序
│   │   └── simple_demo.rs       # 简化演示程序
│   └── crates/
│       ├── error/               # 错误处理框架
│       │   ├── src/lib.rs       # 零依赖错误处理
│       │   └── Cargo.toml
│       ├── sys/                 # 系统监控模块
│       │   ├── src/lib.rs       # 跨平台系统信息获取
│       │   └── Cargo.toml
│       ├── bey-discovery/       # 设备发现模块
│       │   ├── src/lib.rs       # UDP广播设备发现
│       │   └── Cargo.toml
│       ├── bey-transport/       # 安全传输层
│       │   ├── src/lib.rs       # QUIC+TLS安全传输
│       │   └── Cargo.toml
│       └── bey-types/           # 共享类型定义
│           ├── src/lib.rs       # 模块间共享类型
│           └── Cargo.toml
├── Cargo.toml                   # 项目配置
└── README.md                    # 项目文档
```

## 🛠️ 技术栈

### 核心技术
- **Rust**: 系统编程语言，内存安全、高性能
- **Tokio**: 异步运行时，最小特性集配置
- **QUIC**: 现代传输协议，基于UDP
- **TLS 1.3**: 端到端加密通信
- **mDNS**: 局域网设备自动发现

### 依赖管理原则
- 按需添加依赖，避免过度依赖
- 零隐式转换，确保最高性能
- 内存安全优先，杜绝 unwrap() 危险操作
- 完整的错误处理和类型安全

## 🏗️ 架构设计

### 核心模块

1. **错误处理框架** (`error`)
   - 零外部依赖的自定义错误处理
   - 支持错误链、上下文信息、严重程度分类
   - 完整的错误类别和错误码管理

2. **系统监控** (`sys`)
   - 跨平台系统信息获取（CPU、内存、磁盘）
   - 异步热监控和条件钩子
   - 推荐线程数和性能优化建议

3. **设备发现** (`bey-discovery`)
   - UDP广播自动发现局域网设备
   - 实时心跳检测和设备状态维护
   - 事件驱动的设备上线/下线通知

4. **安全传输** (`bey-transport`)
   - QUIC协议实现高性能传输
   - TLS 1.3端到端加密
   - 自动证书生成和管理
   - 支持多种消息类型的统一传输

5. **共享类型** (`bey-types`)
   - 模块间共享的数据结构
   - 序列化友好的类型定义
   - 避免循环依赖的清晰架构

## 🚀 快速开始

### 环境要求

- Rust 1.70+ (推荐使用 rustup 安装)
- 支持 Tokio 异步运行时

### 安装和运行

1. **克隆项目**
   ```bash
   git clone <repository-url>
   cd bey
   ```

2. **运行演示程序**
   ```bash
   # 简化演示（推荐首次运行）
   cargo run --bin simple_demo

   # 完整功能演示
   cargo run --bin demo
   ```

3. **运行测试**
   ```bash
   # 运行所有测试
   cargo test

   # 运行特定模块测试
   cargo test -p bey-discovery
   cargo test -p bey-transport
   ```

### 基本使用

```rust
use bey::BeyApp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 BEY 应用实例
    let app = BeyApp::new().await?;

    // 获取本地设备信息
    let device = app.local_device();
    println!("设备 ID: {}", device.device_id);
    println!("设备名称: {}", device.device_name);

    // 获取系统信息
    let sys_info = app.system_info();
    println!("CPU 使用率: {:.1}%", sys_info.cpu_usage());

    Ok(())
}
```

## 📚 功能模块详解

### 设备发现

自动发现局域网内的其他 BEY 设备：

```rust
use bey_discovery::{DiscoveryService, DiscoveryConfig};
use std::time::Duration;

let config = DiscoveryConfig::new()
    .with_port(8080)
    .with_heartbeat_interval(Duration::from_secs(30));

let mut discovery = DiscoveryService::new(config, local_device).await?;
discovery.start().await?;

// 监听设备事件
while let Some(event) = discovery.next_event().await {
    match event {
        DeviceEvent::DeviceOnline(device) => {
            println!("新设备上线: {}", device.device_name);
        }
        DeviceEvent::DeviceOffline(device_id) => {
            println!("设备下线: {}", device_id);
        }
    }
}
```

### 安全传输

基于 QUIC 的端到端加密通信：

```rust
use bey_transport::{SecureTransport, TransportConfig, TransportMessage};

let config = TransportConfig::new()
    .with_port(8443)
    .with_certificates_dir("./certs")?;

let mut transport = SecureTransport::new(config).await?;
transport.start_server().await?;

// 连接到远程设备
let remote_addr: SocketAddr = "192.168.1.100:8443".parse()?;
let connection = transport.connect(remote_addr).await?;

// 发送消息
let message = TransportMessage::Message {
    message_id: "msg-001".to_string(),
    msg_type: MessageType::Normal,
    content: "Hello, BEY!".to_string(),
    sender: "device-001".to_string(),
    timestamp: SystemTime::now(),
};

transport.send_message(&connection, message).await?;
```

### 系统监控

实时系统资源监控：

```rust
use sys::SystemInfo;

let mut sys_info = SystemInfo::new().await;

// 获取系统信息
println!("操作系统: {} {}", sys_info.os_name(), sys_info.os_version());
println!("CPU 使用率: {:.1}%", sys_info.cpu_usage());
println!("内存使用率: {:.1}%", sys_info.memory_usage_percent());

// 刷新并监控变化
sys_info.refresh();
```

## 🧪 测试驱动开发

项目严格遵循测试驱动原则，所有功能都有完整的测试覆盖：

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test -p bey-discovery
cargo test -p bey-transport
cargo test -p sys

# 运行文档测试
cargo test --doc
```

## 🔒 安全性

- **证书管理**: 自动生成和管理 X.509 证书
- **TLS 1.3 加密**: 最新的传输层安全协议
- **双向认证**: 支持客户端和服务端的相互验证
- **内存安全**: Rust 的所有权系统防止内存安全漏洞

## 📈 性能优化

- **零拷贝**: 减少不必要的内存拷贝操作
- **异步 I/O**: 基于 Tokio 的高性能异步处理
- **连接复用**: QUIC 的多路复用特性
- **智能缓存**: 系统信息和设备状态的智能缓存

## 🤝 贡献指南

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 打开 Pull Request

### 代码规范

- 所有代码必须通过 `cargo clippy` 检查
- 所有新功能必须有对应的单元测试
- 遵循 Rust 官方代码风格指南
- 使用有意义的变量和函数命名
- 添加完整的中文注释和文档

## 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 🙏 致谢

- Tokio 异步运行时
- Quinn QUIC 实现
- Rustls TLS 库
- 所有开源项目的贡献者

## 📞 联系方式

- 项目主页: [GitHub Repository]
- 问题反馈: [GitHub Issues]
- 文档: [项目 Wiki]

---

**BEY** - 让局域网协作更简单、更安全、更高效！