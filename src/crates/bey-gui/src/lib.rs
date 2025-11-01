//! # BEY GUI 模块
//!
//! 使用 Tauri 实现的图形用户界面
//!
//! ## 功能特性
//!
//! - 现代化的 Web 技术栈界面
//! - 实时设备列表更新
//! - 文件传输可视化进度
//! - 系统托盘集成
//! - 跨平台支持（Windows、macOS、Linux）
//!
//! ## 使用方法
//!
//! ```no_run
//! use bey_gui::GuiApp;
//! use bey_func::BeyFuncManager;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let manager = BeyFuncManager::new("my_device".to_string(), "./storage".into()).await?;
//!     let gui = GuiApp::new(manager);
//!     gui.run().await?;
//!     Ok(())
//! }
//! ```

use error::ErrorInfo;
use std::sync::Arc;
use bey_func::BeyFuncManager;

/// GUI 应用程序主结构
pub struct GuiApp {
    /// 功能管理器
    manager: Arc<BeyFuncManager>,
}

impl GuiApp {
    /// 创建新的 GUI 应用程序
    ///
    /// # 参数
    ///
    /// * `manager` - BEY 功能管理器
    ///
    /// # 返回
    ///
    /// 返回新创建的 GUI 应用程序实例
    pub fn new(manager: Arc<BeyFuncManager>) -> Self {
        Self { manager }
    }

    /// 运行 GUI 应用程序
    ///
    /// # 错误
    ///
    /// 如果 Tauri 初始化失败或运行过程中发生错误，返回错误信息
    pub async fn run(&self) -> Result<(), ErrorInfo> {
        tracing::info!("启动 BEY GUI 应用程序");
        
        // TODO: 初始化 Tauri 应用程序
        // tauri::Builder::default()
        //     .setup(|app| {
        //         // 初始化应用程序
        //         Ok(())
        //     })
        //     .invoke_handler(tauri::generate_handler![
        //         // 注册命令处理器
        //     ])
        //     .run(tauri::generate_context!())
        //     .expect("error while running tauri application");
        
        tracing::warn!("GUI 模式尚未完全实现");
        println!("GUI 模式正在开发中，请使用 TUI 模式: cargo run --features tui");
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gui_app_creation() {
        // 测试 GUI 应用程序创建
        // 由于需要完整的功能管理器，这里只是框架测试
    }
}
