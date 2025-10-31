//! # BEY 应用程序模块
//!
//! 集成所有子库，提供统一的应用程序接口。
//! 支持 GUI (Tauri) 和 TUI (ratatui) 两种界面模式。

use crate::{AppResult, BeyApp, DeviceInfo};
use error::ErrorInfo;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 应用程序配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    /// 应用程序名称
    pub app_name: String,
    /// 应用程序版本
    pub app_version: String,
    /// 存储路径
    pub storage_path: String,
    /// 网络端口
    pub network_port: u16,
    /// 启用GUI模式
    pub enable_gui: bool,
    /// 启用TUI模式
    pub enable_tui: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "BEY".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            storage_path: "./bey_data".to_string(),
            network_port: 8080,
            enable_gui: false,
            enable_tui: true,
        }
    }
}

/// 应用程序状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AppState {
    /// 初始化中
    Initializing,
    /// 运行中
    Running,
    /// 暂停
    Paused,
    /// 停止中
    Stopping,
    /// 已停止
    Stopped,
}

/// BEY 主应用程序管理器
///
/// 集成所有功能模块，提供统一的管理接口
pub struct BeyAppManager {
    /// 配置
    config: AppConfig,
    /// 核心应用
    core_app: BeyApp,
    /// 当前状态
    state: Arc<RwLock<AppState>>,
    /// 网络引擎
    net_engine: Option<Arc<bey_net::engine::TransportEngine>>,
    /// 功能管理器
    func_manager: Option<Arc<bey_func::BeyFuncManager>>,
    /// 插件管理器
    plugin_manager: Option<Arc<bey_plugin::PluginManager>>,
}

impl BeyAppManager {
    /// 创建新的应用程序管理器
    ///
    /// # 参数
    ///
    /// * `config` - 应用程序配置
    ///
    /// # 返回值
    ///
    /// 返回初始化的管理器实例或错误信息
    pub async fn new(config: AppConfig) -> AppResult<Self> {
        let core_app = BeyApp::new().await?;
        let state = Arc::new(RwLock::new(AppState::Initializing));

        Ok(Self {
            config,
            core_app,
            state,
            net_engine: None,
            func_manager: None,
            plugin_manager: None,
        })
    }

    /// 初始化所有模块
    ///
    /// # 返回值
    ///
    /// 成功返回 Ok(())，失败返回错误信息
    pub async fn initialize(&mut self) -> AppResult<()> {
        // 创建存储目录
        std::fs::create_dir_all(&self.config.storage_path)
            .map_err(|e| ErrorInfo::new(2001, format!("创建存储目录失败: {}", e)))?;

        // 初始化网络引擎
        let device_id = self.core_app.local_device().device_id.clone();
        let mut net_config = bey_net::engine::EngineConfig::default();
        net_config.name = device_id.clone();
        net_config.port = self.config.network_port;
        net_config.enable_encryption = true;

        let net_engine = bey_net::engine::TransportEngine::new(net_config).await
            .map_err(|e| ErrorInfo::new(2002, format!("初始化网络引擎失败: {:?}", e)))?;
        
        self.net_engine = Some(Arc::new(net_engine));

        // 初始化功能管理器
        let storage_path = self.config.storage_path.as_str();
        
        let func_manager = bey_func::BeyFuncManager::new(&device_id, storage_path).await
            .map_err(|e| ErrorInfo::new(2003, format!("初始化功能管理器失败: {:?}", e)))?;
        
        self.func_manager = Some(Arc::new(func_manager));

        // 初始化插件管理器
        let plugin_manager = bey_plugin::PluginManager::new();
        self.plugin_manager = Some(Arc::new(plugin_manager));

        // 更新状态
        *self.state.write().await = AppState::Running;

        Ok(())
    }

    /// 启动应用程序
    ///
    /// # 返回值
    ///
    /// 成功返回 Ok(())，失败返回错误信息
    pub async fn start(&mut self) -> AppResult<()> {
        // 启动网络引擎
        if let Some(engine) = &self.net_engine {
            engine.start_server().await
                .map_err(|e| ErrorInfo::new(2004, format!("启动网络服务失败: {:?}", e)))?;
        }

        // 启动功能管理器
        if let Some(manager) = &self.func_manager {
            manager.start().await
                .map_err(|e| ErrorInfo::new(2005, format!("启动功能管理器失败: {:?}", e)))?;
        }

        // 启动所有插件
        if let Some(plugin_mgr) = &self.plugin_manager {
            plugin_mgr.start_all().await
                .map_err(|e| ErrorInfo::new(2006, format!("启动插件失败: {:?}", e)))?;
        }

        Ok(())
    }

    /// 停止应用程序
    ///
    /// # 返回值
    ///
    /// 成功返回 Ok(())，失败返回错误信息
    pub async fn stop(&mut self) -> AppResult<()> {
        *self.state.write().await = AppState::Stopping;

        // 停止所有插件
        if let Some(plugin_mgr) = &self.plugin_manager {
            plugin_mgr.stop_all().await
                .map_err(|e| ErrorInfo::new(2007, format!("停止插件失败: {:?}", e)))?;
        }

        // 功能管理器和网络引擎没有 stop 方法，它们会在 drop 时自动清理
        // 清除引用以触发析构
        self.func_manager = None;
        self.net_engine = None;

        *self.state.write().await = AppState::Stopped;

        Ok(())
    }

    /// 获取当前状态
    pub async fn state(&self) -> AppState {
        *self.state.read().await
    }

    /// 获取配置
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// 获取核心应用
    pub fn core_app(&self) -> &BeyApp {
        &self.core_app
    }

    /// 获取本地设备信息
    pub fn local_device(&self) -> &DeviceInfo {
        self.core_app.local_device()
    }

    /// 获取网络引擎
    pub fn net_engine(&self) -> Option<Arc<bey_net::engine::TransportEngine>> {
        self.net_engine.clone()
    }

    /// 获取功能管理器
    pub fn func_manager(&self) -> Option<Arc<bey_func::BeyFuncManager>> {
        self.func_manager.clone()
    }

    /// 获取插件管理器
    pub fn plugin_manager(&self) -> Option<Arc<bey_plugin::PluginManager>> {
        self.plugin_manager.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_manager_creation() {
        let config = AppConfig::default();
        let manager_result = BeyAppManager::new(config).await;
        assert!(manager_result.is_ok(), "应用程序管理器创建应该成功");

        let manager = manager_result.unwrap();
        assert_eq!(manager.state().await, AppState::Initializing);
    }

    #[tokio::test]
    async fn test_app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.app_name, "BEY");
        assert_eq!(config.network_port, 8080);
        assert!(config.enable_tui);
    }
}
