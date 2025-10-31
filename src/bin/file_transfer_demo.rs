//! # 文件传输演示程序
//!
//! 展示 BEY 文件传输模块的基本功能，包括文件上传、下载、进度监控等。

use std::path::PathBuf;
use std::time::SystemTime;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error, Level};

// 使用 bey-file-transfer 模块
use bey_file_transfer::{
    TransferManager, TransferConfig, TransferOptions, TransferDirection, TransferMetadata,
    TransferPriority, TransferResult
};

#[tokio::main]
async fn main() -> TransferResult<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("开始 BEY 文件传输演示");

    // 创建传输配置
    let config = TransferConfig {
        enable_encryption: true,
        max_concurrency: 2,
        chunk_size: 64 * 1024, // 64KB chunks
        max_retries: 3,
        timeout_seconds: 300,
        heartbeat_interval_seconds: 10,
        buffer_size: 32 * 1024, // 32KB buffer
    };

    info!("创建传输管理器...");

    // 创建传输管理器
    let manager = match TransferManager::new(config).await {
        Ok(manager) => {
            info!("传输管理器创建成功");
            manager
        }
        Err(e) => {
            error!("传输管理器创建失败: {}", e);
            return Err(e);
        }
    };

    // 创建测试文件
    let test_file_path = PathBuf::from("/tmp/test_transfer_file.txt");
    let target_path = PathBuf::from("/tmp/transferred_file.txt");

    info!("创建测试文件: {:?}", test_file_path);

    // 创建测试数据
    let test_data = "这是一个用于测试 BEY 文件传输系统的示例文件。\n".repeat(1000);

    // 写入测试文件
    tokio::fs::write(&test_file_path, test_data).await.map_err(|e| {
        error!("创建测试文件失败: {}", e);
        e
    })?;

    info!("测试文件创建完成，大小: {} 字节", test_file_path.metadata()?.len());

    // 创建传输元数据
    let _metadata = TransferMetadata {
        mime_type: "text/plain".to_string(),
        file_extension: "txt".to_string(),
        created_at: SystemTime::now(),
        modified_at: SystemTime::now(),
        properties: std::collections::HashMap::new(),
    };

    // 创建传输选项
    let _options = TransferOptions {
        priority: TransferPriority::High,
        user_id: "demo_user".to_string(),
        permission_token: "demo_token".to_string(),
        tags: vec!["demo".to_string(), "test".to_string()],
        attributes: std::collections::HashMap::new(),
    };

    info!("开始创建传输任务...");

    // 创建传输任务
    let task_id = manager.create_transfer(
        test_file_path.clone(),
        target_path.clone(),
        TransferDirection::Upload,
    ).await?;
    info!("传输任务创建成功，任务ID: {}", task_id);

    // 订阅传输进度
    info!("订阅传输进度...");
    let mut progress_rx = manager.subscribe_progress(&task_id).await?;

    // 启动进度监控任务
    let _progress_task_id = task_id.clone();
    let progress_handle = tokio::spawn(async move {
        info!("开始监控传输进度...");
        while let Ok(progress) = progress_rx.recv().await {
            info!(
                "传输进度更新 - 任务: {}, 进度: {:.1}%, 已传输: {}/{} 字节, 速度: {} 字节/秒",
                progress.task_id,
                progress.percentage,
                progress.transferred_bytes,
                progress.total_bytes,
                progress.speed
            );

            // 如果传输完成或失败，退出循环
            if progress.percentage >= 100.0 || progress.error.is_some() {
                break;
            }
        }
        info!("传输进度监控结束");
    });

    // 开始传输
    info!("开始执行传输任务...");
    match manager.start_transfer(&task_id).await {
        Ok(()) => {
            info!("传输任务完成");
        }
        Err(e) => {
            error!("传输任务失败: {}", e);

            // 清理测试文件
            let _ = tokio::fs::remove_file(&test_file_path).await;
            let _ = tokio::fs::remove_file(&target_path).await;

            return Err(e);
        }
    }

    // 等待进度监控完成
    let _ = progress_handle.await;

    // 验证传输结果
    info!("验证传输结果...");

    if target_path.exists() {
        let original_size = test_file_path.metadata()?.len();
        let transferred_size = target_path.metadata()?.len();

        if original_size == transferred_size {
            info!("传输验证成功！文件大小匹配: {} 字节", transferred_size);

            // 读取并比较文件内容
            let original_content = tokio::fs::read_to_string(&test_file_path).await?;
            let transferred_content = tokio::fs::read_to_string(&target_path).await?;

            if original_content == transferred_content {
                info!("内容验证成功！文件内容完全匹配");
            } else {
                warn!("内容验证失败！文件内容不匹配");
            }
        } else {
            error!("传输验证失败！文件大小不匹配: 原始 {} 字节 vs 传输 {} 字节",
                   original_size, transferred_size);
        }
    } else {
        error!("传输验证失败！目标文件不存在");
    }

    // 清理测试文件
    info!("清理测试文件...");
    let _ = tokio::fs::remove_file(&test_file_path).await;
    let _ = tokio::fs::remove_file(&target_path).await;

    info!("BEY 文件传输演示完成");
    Ok(())
}

/// 模拟传输进度更新（用于演示）
#[allow(dead_code)]
async fn simulate_progress_updates(task_id: String, total_size: u64) {
    let mut transferred = 0u64;
    let chunk_size = total_size / 20; // 分20次更新进度

    while transferred < total_size {
        transferred = std::cmp::min(transferred + chunk_size, total_size);
        let percentage = (transferred as f64 / total_size as f64) * 100.0;

        info!(
            "模拟进度更新 - 任务: {}, 进度: {:.1}%, 已传输: {}/{} 字节",
            task_id, percentage, transferred, total_size
        );

        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transfer_config() {
        let config = TransferConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.chunk_size, 1024 * 1024);
        assert!(config.enable_encryption);
    }

    #[tokio::test]
    async fn test_transfer_options() {
        let options = TransferOptions::default();
        assert_eq!(options.priority, TransferPriority::Normal);
        assert_eq!(options.user_id, "");
        assert_eq!(options.permission_token, "");
        assert!(options.tags.is_empty());
        assert!(options.attributes.is_empty());
    }

    #[tokio::test]
    async fn test_transfer_metadata() {
        let metadata = TransferMetadata {
            mime_type: "text/plain".to_string(),
            file_extension: "txt".to_string(),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            properties: std::collections::HashMap::new(),
        };

        assert_eq!(metadata.mime_type, "text/plain");
        assert_eq!(metadata.file_extension, "txt");
        assert!(metadata.properties.is_empty());
    }
}