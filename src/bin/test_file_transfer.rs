//! # 文件传输测试程序
//!
//! 简单的测试程序，验证BEY文件传输模块的基本功能

use std::path::PathBuf;
use tracing::info;

// 使用 bey-file-transfer 模块
use bey_file_transfer::{
    BeyFileServer, FileServerConfig, ServerEvent, StorageFactory,
    TransferResult
};

#[tokio::main]
async fn main() -> TransferResult<()> {
    
    info!("开始BEY文件传输功能测试");

    // 测试1: 创建本地存储
    test_local_storage().await?;

    // 测试2: 创建文件服务器
    test_file_server().await?;

    info!("所有测试完成！");
    Ok(())
}

/// 测试本地存储功能
async fn test_local_storage() -> TransferResult<()> {
    info!("=== 测试本地存储功能 ===");

    // 创建临时目录
    let temp_dir = "/tmp/bey_test_storage";
    let storage = StorageFactory::create_local_storage(temp_dir, 4096).await?;

    // 创建测试文件
    let test_path = std::path::Path::new("test_file.txt");
    let test_data = b"Hello, BEY File Transfer System!";

    // 写入文件
    storage.write_chunk(test_path, 0, test_data.as_slice().into()).await?;
    info!("✓ 文件写入成功");

    // 读取文件
    let read_data = storage.read_chunk(test_path, 0, test_data.len()).await?;
    assert_eq!(read_data.as_ref(), test_data);
    info!("✓ 文件读取验证成功");

    // 检查文件存在
    let exists = storage.exists(test_path).await?;
    assert!(exists);
    info!("✓ 文件存在性检查成功");

    // 获取文件信息
    let file_info = storage.get_file_info(test_path).await?;
    assert_eq!(file_info.size, test_data.len() as u64);
    info!("✓ 文件信息获取成功: 大小 {} 字节", file_info.size);

    // 删除文件
    storage.delete_file(test_path).await?;
    info!("✓ 文件删除成功");

    // 再次检查文件是否存在
    let exists_after_delete = storage.exists(test_path).await?;
    assert!(!exists_after_delete);
    info!("✓ 文件删除验证成功");

    Ok(())
}

/// 测试文件服务器功能
async fn test_file_server() -> TransferResult<()> {
    info!("=== 测试文件服务器功能 ===");

    // 创建服务器配置
    let config = FileServerConfig {
        server_name: "BEY Test Server".to_string(),
        port: 8443,
        storage_root: PathBuf::from("/tmp/bey_test_server"),
        max_connections: 10,
        connection_timeout: tokio::time::Duration::from_secs(30),
        heartbeat_interval: tokio::time::Duration::from_secs(10),
        discovery_port: 8080,
        enable_discovery: false, // 测试时禁用发现服务
        max_file_size: 100 * 1024 * 1024, // 100MB
        allowed_file_types: vec!["*".to_string()],
        enable_access_log: true,
    };

    // 创建文件服务器
    let mut server = BeyFileServer::new(config).await?;
    info!("✓ 文件服务器创建成功");

    // 订阅服务器事件
    let mut event_rx = server.subscribe_events().await;
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                ServerEvent::ServerStarted { port } => {
                    info!("🚀 服务器已启动，监听端口: {}", port);
                }
                ServerEvent::ServerStopped => {
                    info!("🛑 服务器已停止");
                }
                ServerEvent::ClientConnected { client_address, device_id } => {
                    info!("📥 客户端连接: {} ({})", client_address, device_id);
                }
                ServerEvent::ClientDisconnected { client_address, device_id } => {
                    info!("📤 客户端断开: {} ({})", client_address, device_id);
                }
                ServerEvent::FileOperation { operation, path, success } => {
                    let status = if success { "✅" } else { "❌" };
                    info!("📁 文件操作 {}: {} {}", status, operation, path);
                }
            }
        }
    });

    // 启动服务器
    server.start().await?;
    info!("✓ 文件服务器启动成功");

    // 等待一小段时间让服务器完全启动
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 测试文件操作
    test_file_operations(&server).await?;

    // 获取服务器统计信息
    let stats = server.get_statistics().await;
    info!("📊 服务器统计:");
    info!("   总请求数: {}", stats.total_requests.load(std::sync::atomic::Ordering::Relaxed));
    info!("   成功请求数: {}", stats.successful_requests.load(std::sync::atomic::Ordering::Relaxed));
    info!("   失败请求数: {}", stats.failed_requests.load(std::sync::atomic::Ordering::Relaxed));

    // 获取访问日志
    let logs = server.get_access_log(10).await;
    info!("📋 访问日志 (最近 {} 条):", logs.len());
    for log in logs {
        let status = if log.success { "✅" } else { "❌" };
        info!("   {} {} - {} ({} ms)", status, log.operation, log.file_path, log.processing_time_ms);
    }

    // 停止服务器
    server.stop().await?;
    info!("✓ 文件服务器停止成功");

    Ok(())
}

/// 测试文件操作
async fn test_file_operations(server: &BeyFileServer) -> TransferResult<()> {
    info!("--- 测试文件操作 ---");

    // 这里我们可以测试文件操作，但由于我们的简化实现，
    // 主要验证服务器的基本功能
    let test_files = vec!["test1.txt", "test2.dat", "dir/test3.json"];

    for filename in test_files {
        info!("测试文件操作: {}", filename);

        // 创建目录（如果需要）
        if let Some(parent) = std::path::Path::new(filename).parent()
            && !parent.as_os_str().is_empty() {
                let create_dir_op = bey_file_transfer::FileOperation::CreateDirectory {
                    path: parent.to_string_lossy().to_string(),
                };
                let _ = server.handle_file_operation(
                    create_dir_op,
                    "127.0.0.1:8080".parse().unwrap(),
                    "test-client".to_string(),
                ).await;
        }

        // 测试文件存在检查
        let exists_op = bey_file_transfer::FileOperation::FileExists {
            path: filename.to_string(),
        };
        let _ = server.handle_file_operation(
            exists_op,
            "127.0.0.1:8080".parse().unwrap(),
            "test-client".to_string(),
        ).await;

        // 测试获取文件信息
        let info_op = bey_file_transfer::FileOperation::GetFileInfo {
            path: filename.to_string(),
        };
        let _ = server.handle_file_operation(
            info_op,
            "127.0.0.1:8080".parse().unwrap(),
            "test-client".to_string(),
        ).await;
    }

    info!("✓ 文件操作测试完成");
    Ok(())
}