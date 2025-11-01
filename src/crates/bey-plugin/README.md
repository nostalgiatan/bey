# BEY 插件系统 (bey-plugin)

BEY 插件系统为项目提供完整的插件架构，支持动态加载、生命周期管理和处理流程集成。

## 功能特性

### 🔌 核心功能

1. **插件生命周期管理**
   - 初始化 (Init)
   - 启动 (Start)
   - 运行 (Running)
   - 停止 (Stop)
   - 清理 (Cleanup)

2. **事件总线系统**
   - 事件订阅/发布
   - 优先级支持
   - 异步处理

3. **钩子系统**
   - 30+ 预定义钩子点
   - 网络层、存储层、消息层、剪切板层钩子
   - 钩子链处理

4. **插件依赖管理**
   - 自动依赖解析
   - 循环依赖检测
   - 按依赖顺序加载

5. **性能监控**
   - 初始化时间
   - 事件处理次数
   - 平均处理时间

## 架构设计

```text
┌──────────────────────────────────────────────────────┐
│           插件管理器 (PluginManager)                  │
│  ┌────────────────────────────────────────────────┐  │
│  │ 插件注册表                                      │  │
│  │ - 插件实例                                      │  │
│  │ - 元数据                                        │  │
│  │ - 状态                                          │  │
│  │ - 上下文                                        │  │
│  └────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
              ↓                    ↓
┌─────────────────────┐  ┌─────────────────────────┐
│  事件总线 (EventBus) │  │ 钩子注册表 (HookRegistry)│
│  - 事件订阅          │  │ - 钩子注册               │
│  - 事件分发          │  │ - 钩子执行               │
│  - 优先级管理        │  │ - 钩子链                 │
└─────────────────────┘  └─────────────────────────┘
```

## 快速开始

### 创建自定义插件

```rust
use bey_plugin::{Plugin, PluginContext, PluginResult};
use async_trait::async_trait;

struct LoggerPlugin {
    log_count: u64,
}

#[async_trait]
impl Plugin for LoggerPlugin {
    fn name(&self) -> &str {
        "logger"
    }
    
    fn version(&self) -> &str {
        "1.0.0"
    }
    
    fn description(&self) -> &str {
        "记录所有事件的日志插件"
    }
    
    fn subscribed_events(&self) -> Vec<String> {
        vec![
            "network.message_received".to_string(),
            "storage.after_write".to_string(),
        ]
    }
    
    async fn on_init(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
        println!("日志插件初始化");
        self.log_count = 0;
        Ok(())
    }
    
    async fn on_event(&mut self, event: &str, data: &[u8], ctx: &mut PluginContext) -> PluginResult<()> {
        self.log_count += 1;
        println!("事件 [{}]: {} 字节 (总计: {})", event, data.len(), self.log_count);
        Ok(())
    }
}
```

### 使用插件管理器

```rust
use bey_plugin::PluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建插件管理器
    let manager = PluginManager::new();
    
    // 注册插件
    manager.register(Box::new(LoggerPlugin { log_count: 0 })).await?;
    
    // 启动所有插件
    manager.start_all().await?;
    
    // 发送事件
    manager.emit_event("network.message_received", b"Hello World").await?;
    
    // 停止所有插件
    manager.stop_all().await?;
    
    Ok(())
}
```

## 钩子系统

### 预定义钩子点

#### 网络层钩子
- `network.before_send` - 消息发送前
- `network.after_send` - 消息发送后
- `network.before_receive` - 消息接收前
- `network.after_receive` - 消息接收后
- `network.connection_established` - 连接建立
- `network.connection_closed` - 连接关闭

#### 存储层钩子
- `storage.before_write` - 数据写入前
- `storage.after_write` - 数据写入后
- `storage.before_read` - 数据读取前
- `storage.after_read` - 数据读取后
- `storage.before_delete` - 数据删除前
- `storage.after_delete` - 数据删除后

#### 消息层钩子
- `message.before_send` - 消息发送前
- `message.after_send` - 消息发送后
- `message.received` - 消息接收
- `message.processed` - 消息处理完成

#### 剪切板钩子
- `clipboard.before_sync` - 同步前
- `clipboard.after_sync` - 同步后
- `clipboard.entry_added` - 条目添加
- `clipboard.entry_deleted` - 条目删除

#### 云存储钩子
- `cloud_storage.before_upload` - 文件上传前
- `cloud_storage.after_upload` - 文件上传后
- `cloud_storage.before_download` - 文件下载前
- `cloud_storage.after_download` - 文件下载后

### 使用钩子

```rust
use bey_plugin::{Hook, HookResult, HookPoint};
use async_trait::async_trait;

struct EncryptionHook;

#[async_trait]
impl Hook for EncryptionHook {
    async fn execute(&self, data: Vec<u8>) -> HookResult<Vec<u8>> {
        // 加密数据
        let encrypted = encrypt(&data);
        Ok(encrypted)
    }
}

// 注册钩子
let registry = manager.hook_registry();
registry.register(HookPoint::NetworkBeforeSend, Arc::new(EncryptionHook));
```

## 插件依赖管理

```rust
struct PluginB;

#[async_trait]
impl Plugin for PluginB {
    fn name(&self) -> &str { "plugin_b" }
    fn version(&self) -> &str { "1.0.0" }
    
    // 声明依赖
    fn dependencies(&self) -> Vec<String> {
        vec!["plugin_a".to_string()]
    }
}

// 插件管理器会自动按依赖顺序加载
// plugin_a 会在 plugin_b 之前初始化和启动
```

## 性能监控

```rust
// 获取插件统计信息
if let Some(stats) = manager.get_plugin_stats("logger") {
    println!("初始化时间: {}ms", stats.init_time_ms);
    println!("事件处理次数: {}", stats.event_count);
    println!("平均处理时间: {}μs", stats.avg_event_time_us);
}
```

## 集成到 BEY 模块

### 网络层集成

```rust
// 在 bey-net 的 TransportEngine 中
async fn send_message(&self, data: Vec<u8>) -> Result<()> {
    // 触发发送前钩子
    let data = self.plugin_manager.hook_registry()
        .execute(HookPoint::NetworkBeforeSend, data).await?;
    
    // 发送消息
    let result = self.do_send(data).await;
    
    // 发送事件
    self.plugin_manager.emit_event("network.after_send", &[]).await?;
    
    result
}
```

### 存储层集成

```rust
// 在 bey-storage 中
async fn write_data(&self, key: &str, data: Vec<u8>) -> Result<()> {
    // 触发写入前钩子
    let data = self.plugin_manager.hook_registry()
        .execute(HookPoint::StorageBeforeWrite, data).await?;
    
    // 写入数据
    self.do_write(key, data).await?;
    
    // 发送事件
    self.plugin_manager.emit_event("storage.after_write", key.as_bytes()).await?;
    
    Ok(())
}
```

## API 文档

完整的 API 文档可以通过以下命令查看：

```bash
cargo doc --package bey-plugin --open
```

## 依赖关系

- `error` - 错误处理
- `tokio` - 异步运行时
- `async-trait` - 异步特征支持
- `dashmap` - 并发哈希表
- `serde` - 序列化支持
- `tracing` - 日志记录

## 设计原则

1. **模块化** - 插件之间相互独立
2. **可扩展** - 易于添加新功能
3. **性能优先** - 最小化开销
4. **类型安全** - 使用 Rust 类型系统
5. **错误处理** - 完善的错误处理机制
6. **测试驱动** - 完整的测试覆盖

## 测试

运行单元测试：

```bash
cargo test --package bey-plugin
```

运行集成测试：

```bash
cargo test --package bey-plugin --test integration_tests
```

## 示例插件

查看 `examples/` 目录获取更多插件示例：
- `logger_plugin.rs` - 日志记录插件
- `metrics_plugin.rs` - 性能监控插件
- `encryption_plugin.rs` - 加密插件

## 贡献

欢迎贡献！请遵循项目的代码规范：

1. 使用中文注释和文档
2. 遵循测试驱动原则
3. 禁止使用 unwrap()
4. 使用 error 模块处理错误

## 许可证

本项目遵循项目根目录的许可证。
