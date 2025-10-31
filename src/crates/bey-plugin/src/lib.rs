//! # BEY 插件系统
//!
//! 为 BEY 项目提供完整的插件架构，支持动态加载、生命周期管理和处理流程集成。
//!
//! ## 功能特性
//!
//! - **插件生命周期管理** - 初始化、启动、停止、清理
//! - **事件钩子系统** - 在关键处理流程中插入自定义逻辑
//! - **插件依赖管理** - 自动处理插件间的依赖关系
//! - **插件隔离** - 每个插件运行在独立的上下文中
//! - **性能监控** - 跟踪插件执行时间和资源使用
//!
//! ## 架构设计
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │               插件管理器 (PluginManager)              │
//! │  - 加载/卸载插件                                      │
//! │  - 生命周期管理                                       │
//! │  - 依赖解析                                           │
//! └──────────────────────────────────────────────────────┘
//!                          ↓
//! ┌──────────────────────────────────────────────────────┐
//! │              事件总线 (EventBus)                      │
//! │  - 事件分发                                           │
//! │  - 钩子调用                                           │
//! │  - 异步处理                                           │
//! └──────────────────────────────────────────────────────┘
//!                          ↓
//! ┌─────────────┬─────────────┬─────────────┬──────────┐
//! │ 网络插件     │ 存储插件     │ 消息插件     │ 自定义   │
//! │ NetworkPlug │ StoragePlug │ MessagePlug │ Custom   │
//! └─────────────┴─────────────┴─────────────┴──────────┘
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use bey_plugin::{PluginManager, Plugin, PluginContext, PluginResult};
//! use async_trait::async_trait;
//!
//! // 定义自定义插件
//! struct MyPlugin;
//!
//! #[async_trait]
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &str { "my_plugin" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     
//!     async fn on_init(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
//!         // 初始化逻辑
//!         Ok(())
//!     }
//!     
//!     async fn on_event(&mut self, event: &str, data: &[u8], ctx: &mut PluginContext) -> PluginResult<()> {
//!         // 处理事件
//!         Ok(())
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建插件管理器
//! let mut manager = PluginManager::new();
//!
//! // 注册插件
//! manager.register(Box::new(MyPlugin)).await?;
//!
//! // 启动所有插件
//! manager.start_all().await?;
//!
//! // 发送事件
//! manager.emit_event("network.message_received", b"data").await?;
//!
//! // 停止所有插件
//! manager.stop_all().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 插件钩子点
//!
//! ### 网络层钩子
//! - `network.before_send` - 消息发送前
//! - `network.after_send` - 消息发送后
//! - `network.before_receive` - 消息接收前
//! - `network.after_receive` - 消息接收后
//! - `network.connection_established` - 连接建立
//! - `network.connection_closed` - 连接关闭
//!
//! ### 存储层钩子
//! - `storage.before_write` - 数据写入前
//! - `storage.after_write` - 数据写入后
//! - `storage.before_read` - 数据读取前
//! - `storage.after_read` - 数据读取后
//! - `storage.before_delete` - 数据删除前
//! - `storage.after_delete` - 数据删除后
//!
//! ### 消息层钩子
//! - `message.before_send` - 消息发送前
//! - `message.after_send` - 消息发送后
//! - `message.received` - 消息接收
//! - `message.processed` - 消息处理完成
//!
//! ### 剪切板钩子
//! - `clipboard.before_sync` - 同步前
//! - `clipboard.after_sync` - 同步后
//! - `clipboard.entry_added` - 条目添加
//! - `clipboard.entry_deleted` - 条目删除

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::sync::Arc;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, Duration};
use tracing::{info, debug, warn, error as log_error};

// 导出子模块
pub mod lifecycle;
pub mod event_bus;
pub mod hooks;
pub mod context;

// 重新导出主要类型
pub use lifecycle::{PluginState, PluginMetadata};
pub use event_bus::{EventBus, Event, EventPriority};
pub use hooks::{HookPoint, HookRegistry};
pub use context::PluginContext;

/// 插件结果类型
pub type PluginResult<T> = std::result::Result<T, ErrorInfo>;

/// 插件特征
///
/// 所有插件必须实现此特征
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 获取插件名称
    fn name(&self) -> &str;
    
    /// 获取插件版本
    fn version(&self) -> &str;
    
    /// 获取插件描述
    fn description(&self) -> &str {
        ""
    }
    
    /// 获取插件作者
    fn author(&self) -> &str {
        ""
    }
    
    /// 获取插件依赖列表
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// 初始化插件
    ///
    /// 在插件加载时调用一次
    async fn on_init(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
        let _ = ctx;
        Ok(())
    }
    
    /// 启动插件
    ///
    /// 在所有插件初始化完成后调用
    async fn on_start(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
        let _ = ctx;
        Ok(())
    }
    
    /// 停止插件
    ///
    /// 在插件管理器关闭前调用
    async fn on_stop(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
        let _ = ctx;
        Ok(())
    }
    
    /// 清理插件
    ///
    /// 在插件卸载时调用
    async fn on_cleanup(&mut self, ctx: &mut PluginContext) -> PluginResult<()> {
        let _ = ctx;
        Ok(())
    }
    
    /// 处理事件
    ///
    /// 当订阅的事件发生时调用
    async fn on_event(&mut self, event: &str, data: &[u8], ctx: &mut PluginContext) -> PluginResult<()> {
        let _ = (event, data, ctx);
        Ok(())
    }
    
    /// 获取订阅的事件列表
    fn subscribed_events(&self) -> Vec<String> {
        Vec::new()
    }
}

/// 插件管理器
///
/// 负责插件的加载、卸载和生命周期管理
pub struct PluginManager {
    /// 已注册的插件
    plugins: DashMap<String, PluginEntry>,
    /// 事件总线
    event_bus: Arc<EventBus>,
    /// 钩子注册表
    hook_registry: Arc<HookRegistry>,
    /// 管理器状态
    running: Arc<tokio::sync::RwLock<bool>>,
}

/// 插件条目
struct PluginEntry {
    /// 插件实例
    plugin: Box<dyn Plugin>,
    /// 插件元数据
    metadata: PluginMetadata,
    /// 插件状态
    state: PluginState,
    /// 插件上下文
    context: PluginContext,
    /// 性能统计
    stats: PluginStats,
}

/// 插件性能统计
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginStats {
    /// 初始化时间（毫秒）
    init_time_ms: u64,
    /// 事件处理次数
    event_count: u64,
    /// 平均事件处理时间（微秒）
    avg_event_time_us: u64,
    /// 最后活跃时间
    last_active: SystemTime,
}

impl Default for PluginStats {
    fn default() -> Self {
        Self {
            init_time_ms: 0,
            event_count: 0,
            avg_event_time_us: 0,
            last_active: SystemTime::now(),
        }
    }
}

impl PluginManager {
    /// 创建新的插件管理器
    pub fn new() -> Self {
        Self {
            plugins: DashMap::new(),
            event_bus: Arc::new(EventBus::new()),
            hook_registry: Arc::new(HookRegistry::new()),
            running: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }
    
    /// 注册插件
    ///
    /// # 参数
    ///
    /// * `plugin` - 插件实例
    ///
    /// # 返回值
    ///
    /// 返回注册结果
    pub async fn register(&self, mut plugin: Box<dyn Plugin>) -> PluginResult<()> {
        let name = plugin.name().to_string();
        
        // 检查插件是否已注册
        if self.plugins.contains_key(&name) {
            return Err(ErrorInfo::new(8001, format!("插件已注册: {}", name))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Warning));
        }
        
        // 创建插件上下文
        let mut context = PluginContext::new(
            name.clone(),
            Arc::clone(&self.event_bus),
            Arc::clone(&self.hook_registry),
        );
        
        // 初始化插件
        let start = SystemTime::now();
        plugin.on_init(&mut context).await
            .map_err(|e| ErrorInfo::new(8002, format!("插件初始化失败: {}", e))
                .with_category(ErrorCategory::System))?;
        
        let init_time = start.elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64;
        
        // 创建元数据
        let metadata = PluginMetadata {
            name: name.clone(),
            version: plugin.version().to_string(),
            description: plugin.description().to_string(),
            author: plugin.author().to_string(),
            dependencies: plugin.dependencies(),
        };
        
        // 订阅事件
        for event in plugin.subscribed_events() {
            self.event_bus.subscribe(&event, &name).await;
        }
        
        let mut stats = PluginStats::default();
        stats.init_time_ms = init_time;
        
        // 保存插件
        let entry = PluginEntry {
            plugin,
            metadata: metadata.clone(),
            state: PluginState::Initialized,
            context,
            stats,
        };
        
        self.plugins.insert(name.clone(), entry);
        
        info!("插件已注册: {} v{} (初始化耗时: {}ms)", name, metadata.version, init_time);
        Ok(())
    }
    
    /// 卸载插件
    ///
    /// # 参数
    ///
    /// * `name` - 插件名称
    ///
    /// # 返回值
    ///
    /// 返回卸载结果
    pub async fn unregister(&self, name: &str) -> PluginResult<()> {
        let mut entry = self.plugins.get_mut(name)
            .ok_or_else(|| ErrorInfo::new(8003, format!("插件未找到: {}", name))
                .with_category(ErrorCategory::Validation))?;
        
        // 停止插件（如果正在运行）
        if entry.state == PluginState::Running {
            // Use raw pointers
            let context_ptr = &mut entry.context as *mut PluginContext;
            let plugin_ptr = &mut entry.plugin as *mut Box<dyn Plugin>;
            
            unsafe {
                (*plugin_ptr).on_stop(&mut *context_ptr).await
                    .map_err(|e| ErrorInfo::new(8004, format!("插件停止失败: {}", e)))?;
            }
        }
        
        // 清理插件
        let context_ptr = &mut entry.context as *mut PluginContext;
        let plugin_ptr = &mut entry.plugin as *mut Box<dyn Plugin>;
        
        unsafe {
            (*plugin_ptr).on_cleanup(&mut *context_ptr).await
                .map_err(|e| ErrorInfo::new(8005, format!("插件清理失败: {}", e)))?;
        }
        
        entry.state = PluginState::Unloaded;
        
        drop(entry);
        self.plugins.remove(name);
        
        info!("插件已卸载: {}", name);
        Ok(())
    }
    
    /// 启动所有插件
    pub async fn start_all(&self) -> PluginResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(ErrorInfo::new(8006, "插件管理器已在运行".to_string())
                .with_category(ErrorCategory::Validation));
        }
        
        // 按依赖顺序启动插件
        let order = self.resolve_dependencies()?;
        
        for name in order {
            if let Some(mut entry) = self.plugins.get_mut(&name) {
                if entry.state == PluginState::Initialized {
                    // Use raw pointers
                    let context_ptr = &mut entry.context as *mut PluginContext;
                    let plugin_ptr = &mut entry.plugin as *mut Box<dyn Plugin>;
                    
                    unsafe {
                        (*plugin_ptr).on_start(&mut *context_ptr).await
                            .map_err(|e| ErrorInfo::new(8007, format!("插件启动失败: {}", e)))?;
                    }
                    
                    entry.state = PluginState::Running;
                    info!("插件已启动: {}", name);
                }
            }
        }
        
        *running = true;
        info!("所有插件已启动");
        Ok(())
    }
    
    /// 停止所有插件
    pub async fn stop_all(&self) -> PluginResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        // 按依赖顺序的逆序停止插件
        let mut order = self.resolve_dependencies()?;
        order.reverse();
        
        for name in order {
            if let Some(mut entry) = self.plugins.get_mut(&name) {
                if entry.state == PluginState::Running {
                    // Use raw pointers to avoid borrow checker issues when calling plugin methods
                    let context_ptr = &mut entry.context as *mut PluginContext;
                    let plugin_ptr = &mut entry.plugin as *mut Box<dyn Plugin>;
                    
                    // Safety: We're the only ones with access to this entry, and we're not
                    // creating any references that outlive this scope
                    unsafe {
                        (*plugin_ptr).on_stop(&mut *context_ptr).await
                            .map_err(|e| ErrorInfo::new(8008, format!("插件停止失败: {}", e)))?;
                    }
                    
                    entry.state = PluginState::Stopped;
                    info!("插件已停止: {}", name);
                }
            }
        }
        
        *running = false;
        info!("所有插件已停止");
        Ok(())
    }
    
    /// 发送事件到插件
    ///
    /// # 参数
    ///
    /// * `event_name` - 事件名称
    /// * `data` - 事件数据
    ///
    /// # 返回值
    ///
    /// 返回处理结果
    pub async fn emit_event(&self, event_name: &str, data: &[u8]) -> PluginResult<()> {
        let subscribers = self.event_bus.get_subscribers(event_name).await;
        
        for plugin_name in subscribers {
            if let Some(mut entry) = self.plugins.get_mut(&plugin_name) {
                if entry.state == PluginState::Running {
                    let start = SystemTime::now();
                    
                    // Use raw pointers to avoid borrow checker issues
                    let context_ptr = &mut entry.context as *mut PluginContext;
                    let plugin_ptr = &mut entry.plugin as *mut Box<dyn Plugin>;
                    
                    // Safety: We're the only ones with access to this entry
                    let result = unsafe {
                        (*plugin_ptr).on_event(event_name, data, &mut *context_ptr).await
                    };
                    
                    match result {
                        Ok(_) => {
                            let elapsed = start.elapsed()
                                .unwrap_or(Duration::from_secs(0))
                                .as_micros() as u64;
                            
                            // 更新统计
                            entry.stats.event_count += 1;
                            let total = entry.stats.avg_event_time_us * (entry.stats.event_count - 1) + elapsed;
                            entry.stats.avg_event_time_us = total / entry.stats.event_count;
                            entry.stats.last_active = SystemTime::now();
                            
                            debug!("插件 {} 处理事件 {} 完成 ({}μs)", plugin_name, event_name, elapsed);
                        }
                        Err(e) => {
                            warn!("插件 {} 处理事件 {} 失败: {}", plugin_name, event_name, e);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 获取插件列表
    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        self.plugins.iter()
            .map(|entry| entry.metadata.clone())
            .collect()
    }
    
    /// 获取插件状态
    pub fn get_plugin_state(&self, name: &str) -> Option<PluginState> {
        self.plugins.get(name).map(|entry| entry.state)
    }
    
    /// 获取插件统计信息
    pub fn get_plugin_stats(&self, name: &str) -> Option<PluginStats> {
        self.plugins.get(name).map(|entry| entry.stats.clone())
    }
    
    /// 解析插件依赖顺序
    fn resolve_dependencies(&self) -> PluginResult<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = Vec::new();
        
        for entry in self.plugins.iter() {
            let name = entry.key().clone();
            if !visited.contains(&name) {
                self.visit_plugin(&name, &mut visited, &mut stack, &mut order)?;
            }
        }
        
        Ok(order)
    }
    
    /// 访问插件（深度优先搜索）
    fn visit_plugin(
        &self,
        name: &str,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut Vec<String>,
        order: &mut Vec<String>,
    ) -> PluginResult<()> {
        if stack.contains(&name.to_string()) {
            return Err(ErrorInfo::new(8009, format!("检测到循环依赖: {}", name))
                .with_category(ErrorCategory::Validation));
        }
        
        if visited.contains(name) {
            return Ok(());
        }
        
        stack.push(name.to_string());
        
        if let Some(entry) = self.plugins.get(name) {
            for dep in &entry.metadata.dependencies {
                if !self.plugins.contains_key(dep) {
                    return Err(ErrorInfo::new(8010, format!("依赖的插件未找到: {}", dep))
                        .with_category(ErrorCategory::Validation));
                }
                self.visit_plugin(dep, visited, stack, order)?;
            }
        }
        
        stack.pop();
        visited.insert(name.to_string());
        order.push(name.to_string());
        
        Ok(())
    }
    
    /// 获取事件总线引用
    pub fn event_bus(&self) -> Arc<EventBus> {
        Arc::clone(&self.event_bus)
    }
    
    /// 获取钩子注册表引用
    pub fn hook_registry(&self) -> Arc<HookRegistry> {
        Arc::clone(&self.hook_registry)
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    struct TestPlugin {
        name: String,
        init_called: bool,
    }
    
    impl TestPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                init_called: false,
            }
        }
    }
    
    #[async_trait]
    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            &self.name
        }
        
        fn version(&self) -> &str {
            "1.0.0"
        }
        
        async fn on_init(&mut self, _ctx: &mut PluginContext) -> PluginResult<()> {
            self.init_called = true;
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert_eq!(manager.list_plugins().len(), 0);
    }
    
    #[tokio::test]
    async fn test_plugin_registration() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test"));
        
        let result = manager.register(plugin).await;
        assert!(result.is_ok());
        assert_eq!(manager.list_plugins().len(), 1);
    }
    
    #[tokio::test]
    async fn test_plugin_lifecycle() {
        let manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test"));
        
        manager.register(plugin).await.expect("注册失败");
        manager.start_all().await.expect("启动失败");
        
        let state = manager.get_plugin_state("test");
        assert_eq!(state, Some(PluginState::Running));
        
        manager.stop_all().await.expect("停止失败");
        let state = manager.get_plugin_state("test");
        assert_eq!(state, Some(PluginState::Stopped));
    }
}
