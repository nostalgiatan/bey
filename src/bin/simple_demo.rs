//! # BEY 项目简化演示程序
//!
//! 这个演示程序展示 BEY 局域网中心项目的核心功能。

use error::ErrorInfo;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, Level};

/// 演示结果类型
type DemoResult<T> = std::result::Result<T, ErrorInfo>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    // 显示欢迎信息
    println!("🎉 欢迎使用 BEY 局域网中心项目！");
    println!("📖 这是一个去中心化的局域网协作平台演示");
    println!();

    // 运行演示
    if let Err(e) = run_demo().await {
        eprintln!("❌ 演示运行失败: {}", e);
        return Err(e.into());
    }

    println!("\n🎉 演示程序运行成功！");
    println!("💡 提示：项目包含以下核心功能：");
    println!("   🔍 设备发现 - 自动发现局域网内的 BEY 设备");
    println!("   🔐 安全传输 - 基于 QUIC 的端到端加密通信");
    println!("   📋 剪切板同步 - 跨设备的剪切板内容同步");
    println!("   📁 文件传输 - 安全的文件传输和共享");
    println!("   💬 消息传递 - 实时的消息推送和通知");

    Ok(())
}

/// 运行主要演示
async fn run_demo() -> DemoResult<()> {
    info!("🚀 启动 BEY 演示程序");

    // 演示 1: 初始化 BEY 应用
    demo_bey_app().await?;

    // 演示 2: 系统信息监控
    demo_system_monitoring().await?;

    // 演示 3: 错误处理
    demo_error_handling().await?;

    info!("✅ BEY 演示程序完成");
    Ok(())
}

/// 演示 BEY 应用初始化
async fn demo_bey_app() -> DemoResult<()> {
    info!("\n📱 === 演示 1: BEY 应用初始化 ===");

    match bey::BeyApp::new().await {
        Ok(app) => {
            let device = app.local_device();
            info!("✅ BEY 应用初始化成功");
            info!("   设备 ID: {}", device.device_id);
            info!("   设备名称: {}", device.device_name);
            info!("   设备类型: {:?}", device.device_type);
            info!("   网络地址: {}", device.address);
            info!("   设备能力: {:?}", device.capabilities);

            // 显示系统信息
            let sys_info = app.system_info();
            info!("   操作系统: {} {}", sys_info.os_name(), sys_info.os_version());
            info!("   CPU 使用率: {:.1}%", sys_info.cpu_usage());
            info!("   内存使用率: {:.1}%", sys_info.memory_usage_percent());
            info!("   物理核心数: {}", sys_info.physical_cpu_count());

            Ok(())
        }
        Err(e) => {
            info!("❌ BEY 应用初始化失败: {}", e);
            Err(e)
        }
    }
}

/// 演示系统信息监控
async fn demo_system_monitoring() -> DemoResult<()> {
    info!("\n🖥️  === 演示 2: 系统信息监控 ===");

    let mut sys_info = sys::SystemInfo::new().await;

    // 连续监控几次
    for i in 1..=3 {
        info!("   📊 第 {} 次系统状态检查:", i);
        info!("      CPU 使用率: {:.1}%", sys_info.cpu_usage());
        info!("      内存使用: {} / {} MB",
              sys_info.memory_info().0 / (1024 * 1024),
              sys_info.memory_info().1 / (1024 * 1024));
        info!("      磁盘使用率: {:.1}%", sys_info.disk_usage_percent());

        if i < 3 {
            sys_info.refresh();
            sleep(Duration::from_secs(1)).await;
        }
    }

    info!("✅ 系统监控演示完成");
    Ok(())
}

/// 演示错误处理
async fn demo_error_handling() -> DemoResult<()> {
    info!("\n⚠️  === 演示 3: 错误处理框架 ===");

    // 创建不同类型的错误
    let errors = vec![
        ErrorInfo::new(1001, "这是一个网络错误".to_string())
            .with_category(error::ErrorCategory::Network)
            .with_severity(error::ErrorSeverity::Error),

        ErrorInfo::new(2001, "这是一个配置错误".to_string())
            .with_category(error::ErrorCategory::Configuration)
            .with_severity(error::ErrorSeverity::Warning)
            .with_context("在读取配置文件时".to_string())
            .with_context("路径: /etc/bey/config.toml".to_string()),

        ErrorInfo::new(3001, "这是一个系统错误".to_string())
            .with_category(error::ErrorCategory::System)
            .with_severity(error::ErrorSeverity::Critical),
    ];

    for (i, error) in errors.iter().enumerate() {
        info!("   错误 {}:", i + 1);
        info!("      {}", error);
        info!("      严重程度: {}", error.severity());
        info!("      错误类别: {}", error.category());
    }

    info!("✅ 错误处理演示完成");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_demo_bey_app() {
        let result = demo_bey_app().await;
        assert!(result.is_ok(), "BEY 应用演示应该成功");
    }

    #[tokio::test]
    async fn test_demo_system_monitoring() {
        let result = demo_system_monitoring().await;
        assert!(result.is_ok(), "系统监控演示应该成功");
    }

    #[tokio::test]
    async fn test_demo_error_handling() {
        let result = demo_error_handling().await;
        assert!(result.is_ok(), "错误处理演示应该成功");
    }
}