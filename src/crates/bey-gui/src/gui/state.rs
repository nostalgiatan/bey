//! # GUI 状态管理
//!
//! 管理 GUI 应用程序的全局状态

use std::sync::Arc;
use tokio::sync::RwLock;
use bey_func::BeyFuncManager;
use super::commands::TauriCommandHandler;
use super::events::EventEmitter;

/// GUI 应用状态
///
/// 包含所有 GUI 需要的共享状态和服务
pub struct GuiState {
    /// BEY 功能管理器
    func_manager: Arc<BeyFuncManager>,
    /// 命令处理器
    command_handler: Arc<TauriCommandHandler>,
    /// 事件发射器
    event_emitter: Option<Arc<EventEmitter>>,
    /// 应用配置
    config: Arc<RwLock<GuiConfig>>,
}

/// GUI 配置
#[derive(Debug, Clone)]
pub struct GuiConfig {
    /// 窗口标题
    pub window_title: String,
    /// 主题
    pub theme: String,
    /// 语言
    pub language: String,
    /// 是否启用通知
    pub notifications_enabled: bool,
    /// 是否自动启动
    pub auto_start: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            window_title: "BEY - 局域网协作平台".to_string(),
            theme: "light".to_string(),
            language: "zh-CN".to_string(),
            notifications_enabled: true,
            auto_start: false,
        }
    }
}

impl GuiState {
    /// 创建新的 GUI 状态
    pub fn new(func_manager: Arc<BeyFuncManager>) -> Self {
        let command_handler = Arc::new(TauriCommandHandler::new(func_manager.clone()));
        
        Self {
            func_manager,
            command_handler,
            event_emitter: None,
            config: Arc::new(RwLock::new(GuiConfig::default())),
        }
    }

    /// 设置事件发射器
    pub fn set_event_emitter(&mut self, emitter: EventEmitter) {
        self.event_emitter = Some(Arc::new(emitter));
    }

    /// 获取功能管理器
    pub fn func_manager(&self) -> &Arc<BeyFuncManager> {
        &self.func_manager
    }

    /// 获取命令处理器
    pub fn command_handler(&self) -> &Arc<TauriCommandHandler> {
        &self.command_handler
    }

    /// 获取事件发射器
    pub fn event_emitter(&self) -> Option<&Arc<EventEmitter>> {
        self.event_emitter.as_ref()
    }

    /// 获取配置
    pub async fn config(&self) -> GuiConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config<F>(&self, updater: F)
    where
        F: FnOnce(&mut GuiConfig),
    {
        let mut config = self.config.write().await;
        updater(&mut config);
    }

    /// 启动后台任务
    ///
    /// 启动监听后端事件并转发到前端的任务
    pub async fn start_background_tasks(&self) {
        if let Some(emitter) = &self.event_emitter {
            let emitter = emitter.clone();
            let func_manager = self.func_manager.clone();

            // 启动设备发现监听任务
            tokio::spawn(async move {
                Self::device_discovery_task(func_manager, emitter).await;
            });
        }
    }

    /// 设备发现任务
    ///
    /// 监听新设备的发现并发送事件到前端
    async fn device_discovery_task(
        _func_manager: Arc<BeyFuncManager>,
        emitter: Arc<EventEmitter>,
    ) {
        // TODO: 实现设备发现监听
        // 这需要与 bey-func 模块集成，监听设备发现事件
        
        tracing::info!("设备发现任务已启动");
        
        // 示例：定期检查设备列表变化
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            
            // TODO: 检查设备列表变化
            // 如果有新设备，发送事件
            // emitter.emit_device_online(device_id, device_name).ok();
        }
    }
}
