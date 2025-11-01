# BEY - 局域网协作中心

一个去中心化的局域网协作平台，提供对象剪切板、文件传输、消息传递、权限控制、分布式云空间、证书管理和传输优先级控制功能。

## 🚀 核心特性

- **去中心化架构**: 无需中央服务器，提高可靠性和隐私性
- **局域网优化**: 低延迟、高速度的本地传输
- **IPv4/IPv6双栈支持**: 智能检测与自动回退，确保最大兼容性
- **mDNS设备发现**: 零配置自动发现局域网设备
- **多功能集成**: 剪切板、文件传输、消息推送一体化
- **资源贡献**: 利用闲置磁盘空间创建分布式存储
- **权限管理**: 基于证书的细粒度权限控制
- **极致性能**: 零隐式转换，内存安全优化
- **TUI界面**: 基于ratatui的交互式终端界面

## 📁 项目结构

```
bey/
├── src/
│   ├── lib.rs                    # 核心库
│   ├── main.rs                   # 主程序入口（支持GUI/TUI/无界面模式）
│   ├── app.rs                    # 应用程序管理器
│   └── crates/                   # 内部模块
│       ├── error/               # 错误处理框架（零依赖）
│       ├── sys/                 # 系统监控模块
│       ├── bey-types/           # 共享类型定义
│       ├── bey-identity/        # 身份与证书管理
│       ├── bey-transport/       # QUIC+TLS安全传输
│       ├── bey-net/             # 网络通信层（mDNS/UDP/流控制）
│       ├── bey-storage/         # 分布式存储与加密
│       ├── bey-func/            # 功能层（消息/文件/剪贴板）
│       ├── bey-plugin/          # 插件系统
│       └── bey-tui/             # 终端用户界面
├── benches/                      # 性能基准测试
├── tests/                        # 集成测试
├── Cargo.toml                    # 项目配置
└── README.md                     # 项目文档
```

## 🛠️ 技术栈

### 核心技术
- **Rust 2024 Edition**: 系统编程语言，内存安全、高性能
- **Tokio**: 异步运行时
- **QUIC (Quinn)**: 现代传输协议，基于UDP
- **TLS 1.3 (Rustls)**: 端到端加密通信
- **mDNS**: 局域网设备自动发现
- **Ratatui**: 终端用户界面框架

### 网络支持
- **IPv4/IPv6双栈**: 自动检测IPv6可用性，智能回退到IPv4
- **mDNS多播**: 支持IPv4和IPv6多播地址
- **持久化回退状态**: 检测到IPv6不支持后持久化使用IPv4

### 依赖管理原则
- 按需添加依赖，避免过度依赖
- 零隐式转换，确保最高性能
- 内存安全优先，杜绝生产代码中的 `unwrap()` 操作
- 完整的错误处理和类型安全

## 🏗️ 架构设计

### 核心模块

1. **错误处理框架** (`error`)
   - 零外部依赖的自定义错误处理
   - 支持错误链、上下文信息、严重程度分类
   - 完整的错误类别和错误码管理
   - 生产代码杜绝 `unwrap()` 操作

2. **系统监控** (`sys`)
   - 跨平台系统信息获取（CPU、内存、磁盘）
   - 异步热监控和条件钩子
   - 推荐线程数和性能优化建议

3. **网络通信层** (`bey-net`)
   - **mDNS设备发现**: 零配置自动发现局域网设备
   - **UDP设备发现**: 广播式设备发现
   - **IPv4/IPv6双栈**: 自动检测和智能回退
   - **流控制**: 令牌桶算法实现流量控制
   - **优先级队列**: 支持高/中/低优先级消息
   - **状态机**: 连接状态管理

4. **安全传输层** (`bey-transport`)
   - QUIC协议实现高性能传输
   - TLS 1.3端到端加密
   - 自动证书生成和管理
   - 双向认证支持

5. **身份管理** (`bey-identity`)
   - 设备身份和证书管理
   - X.509证书生成与验证
   - CA证书管理

6. **分布式存储** (`bey-storage`)
   - 云存储抽象层
   - 数据压缩（Lz4/Zstd/Gzip）
   - AES-GCM加密
   - 密钥管理

7. **功能层** (`bey-func`)
   - 消息传递
   - 文件传输
   - 剪贴板同步
   - 插件系统集成

8. **TUI界面** (`bey-tui`)
   - 基于ratatui的终端界面
   - 实时日志查看
   - 设备列表管理
   - 交互式操作菜单
   - 消息发送（私信、群聊、广播）
   - 剪切板同步操作
   - 文件传输和云存储操作
   - 表单输入支持

## 🚀 快速开始

### 环境要求

- Rust 1.70+ (推荐使用 rustup 安装)
- 支持 Tokio 异步运行时
- 支持 IPv4 网络（IPv6 可选）

### 安装和运行

1. **克隆项目**
   ```bash
   git clone <repository-url>
   cd bey
   ```

2. **编译项目**
   ```bash
   # 默认编译（包含配置文件支持）
   cargo build --release
   
   # 编译TUI版本
   cargo build --release --features tui
   
   # 编译GUI版本（实验性）
   cargo build --release --features gui
   ```

3. **运行应用程序**
   ```bash
   # 无界面模式（服务模式）
   cargo run --release
   
   # TUI模式
   cargo run --release --features tui
   
   # 使用配置文件
   BEY_CONFIG=config.toml cargo run --release --features tui
   ```

4. **运行测试**
   ```bash
   # 运行所有测试
   cargo test
   
   # 运行特定模块测试
   cargo test -p bey-net
   cargo test -p bey-storage
   
   # 运行性能基准测试
   cargo bench
   ```

### 基本使用

#### TUI界面操作

- **q**: 退出应用
- **Ctrl+C**: 强制退出
- **o**: 打开操作菜单
- **?**: 显示帮助信息
- **:**: 进入命令模式
- **Up/Down**: 在设备列表或菜单中导航
- **Esc**: 返回上一级或正常模式

在操作菜单中，您可以：
- 发送私信、群聊消息或广播消息
- 添加和同步剪切板内容
- 上传/下载文件到云存储
- 发送文件到其他设备

详细的TUI使用说明，请参阅 [TUI_DEMO.md](TUI_DEMO.md)。

#### 编程接口

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

## 🌐 网络支持

### IPv4/IPv6双栈支持

BEY 完全支持 IPv4 和 IPv6 双栈网络，并具有以下特性：

#### 自动检测与回退
- 启动时自动检测IPv6可用性
- 如果IPv6不支持（如 `Address family not supported by protocol, os error 97`），自动回退到IPv4
- 持久化IPv6支持状态，避免重复检测

#### mDNS多播地址
- **IPv4**: `224.0.0.251:5353`
- **IPv6**: `[ff02::fb]:5353` (link-local多播)

#### 智能发送策略
1. 如果IPv6可用，优先尝试IPv6多播
2. IPv6失败时立即回退到IPv4
3. 检测到IPv6不支持后，后续请求仅使用IPv4

#### 配置选项

```rust
use bey_net::mdns_discovery::MdnsDiscoveryConfig;

let config = MdnsDiscoveryConfig {
    enable_ipv6: true,  // 启用IPv6尝试（默认）
    ..Default::default()
};
```

### 常见网络问题

#### IPv6不支持
某些系统或网络环境可能不支持IPv6。BEY会自动处理此情况：
- 检测到 `os error 97` (Address family not supported)
- 自动禁用IPv6并持久化该状态
- 后续所有通信使用IPv4

#### 防火墙配置
确保以下端口开放：
- **mDNS**: UDP 5353
- **QUIC传输**: 自定义端口（默认8443）

## 📚 功能模块详解

### mDNS设备发现

零配置自动发现局域网内的其他 BEY 设备：

```rust
use bey_net::mdns_discovery::{MdnsDiscovery, MdnsDiscoveryConfig};

let config = MdnsDiscoveryConfig::default();
let device_info = MdnsDiscovery::create_default_device_info(
    device_id,
    device_name,
    device_type,
    8080,
    vec![local_addr],
);

let discovery = MdnsDiscovery::new(config, device_info).await?;
discovery.start().await?;

// 查询服务
let services = discovery.query_service("_bey._tcp", None).await?;
```

### 安全传输

基于 QUIC 的端到端加密通信：

```rust
use bey_transport::{SecureTransport, TransportConfig, TransportMessage};

let config = TransportConfig::new()
    .with_port(8443);

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