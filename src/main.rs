//! # BEY 主程序入口
//!
//! 根据编译条件启动 GUI 或 TUI 界面

use bey::app::{AppConfig, BeyAppManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("BEY 应用程序启动");

    // 加载配置
    let config = load_config()?;

    // 根据编译条件启动不同的界面
    #[cfg(feature = "gui")]
    {
        tracing::info!("启动 GUI 模式");
        gui::run(config).await?;
    }

    #[cfg(all(feature = "tui", not(feature = "gui")))]
    {
        tracing::info!("启动 TUI 模式");
        tui::run(config).await?;
    }

    #[cfg(not(any(feature = "gui", feature = "tui")))]
    {
        tracing::info!("启动无界面模式（服务模式）");
        headless::run(config).await?;
    }

    tracing::info!("BEY 应用程序关闭");
    Ok(())
}

/// 加载配置
///
/// 从配置文件或环境变量加载应用程序配置
fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    #[cfg(feature = "config")]
    {
        // 尝试从配置文件加载
        let config_path = std::env::var("BEY_CONFIG")
            .unwrap_or_else(|_| "config.toml".to_string());

        if std::path::Path::new(&config_path).exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: AppConfig = toml::from_str(&content)?;
            tracing::info!("从配置文件加载: {}", config_path);
            return Ok(config);
        }
    }
    
    tracing::info!("使用默认配置");
    Ok(AppConfig::default())
}

/// GUI 模块（使用 Tauri）
#[cfg(feature = "gui")]
mod gui {
    use super::*;

    pub async fn run(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("GUI 模式尚未实现，将在 bey-gui crate 中实现");
        // GUI 将在 src/crates/bey-gui 中使用 Tauri 实现
        println!("GUI 模式：请等待 bey-gui 模块完成");
        Ok(())
    }
}

/// TUI 模块（使用 ratatui）
#[cfg(feature = "tui")]
mod tui {
    use super::*;

    pub async fn run(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("TUI 模式尚未实现，将在 bey-tui crate 中实现");
        // TUI 将在 src/crates/bey-tui 中使用 ratatui 实现
        println!("TUI 模式：请等待 bey-tui 模块完成");
        Ok(())
    }
}

/// 无界面模式（服务模式）
mod headless {
    use super::*;

    pub async fn run(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("启动无界面服务模式");

        // 创建应用程序管理器
        let mut manager = BeyAppManager::new(config).await?;

        // 初始化所有模块
        manager.initialize().await?;
        tracing::info!("应用程序初始化完成");

        // 启动应用程序
        manager.start().await?;
        tracing::info!("应用程序已启动");

        // 打印设备信息
        let device = manager.local_device();
        println!("\n=== BEY 设备信息 ===");
        println!("设备 ID: {}", device.device_id);
        println!("设备名称: {}", device.device_name);
        println!("设备类型: {:?}", device.device_type);
        println!("网络地址: {}", device.address);
        println!("设备能力: {:?}", device.capabilities);
        println!("====================\n");

        // 等待退出信号
        tracing::info!("按 Ctrl+C 停止应用程序");
        tokio::signal::ctrl_c().await?;

        tracing::info!("收到停止信号，正在关闭应用程序...");
        manager.stop().await?;
        tracing::info!("应用程序已停止");

        Ok(())
    }
}
