//! BEY分布式存储系统演示程序
//!
//! 展示基于现有BEY网络基础设施的分布式对象存储功能

use std::path::Path;
use bey_storage::{create_default_bey_storage, BeyStorageManager};
use error::{ErrorInfo, ErrorCategory};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("🚀 启动BEY分布式存储系统演示程序\n");

    // 1. 创建BEY存储管理器
    println!("🏗️  创建BEY分布式存储管理器...");
    let storage = create_default_bey_storage().await?;
    println!("✅ BEY存储管理器创建成功\n");

    // 2. 测试文件存储
    println!("📁 测试文件存储功能...");
    test_file_storage(&storage).await?;
    println!("✅ 文件存储测试完成\n");

    // 3. 测试文件读取
    println!("📖 测试文件读取功能...");
    test_file_reading(&storage).await?;
    println!("✅ 文件读取测试完成\n");

    // 4. 测试文件搜索
    println!("🔍 测试文件搜索功能...");
    test_file_search(&storage).await?;
    println!("✅ 文件搜索测试完成\n");

    // 5. 获取系统统计信息
    println!("📊 获取系统统计信息...");
    let stats = storage.get_storage_statistics().await?;
    print_storage_statistics(&stats);
    println!("✅ 统计信息获取完成\n");

    // 6. 列出所有文件
    println!("📋 列出所有文件...");
    list_all_files(&storage).await?;
    println!("✅ 文件列表获取完成\n");

    // 7. 清理测试文件
    println!("🧹 清理测试文件...");
    cleanup_test_files(&storage).await?;
    println!("✅ 清理完成\n");

    println!("🎉 BEY分布式存储系统演示完成！");
    println!("💡 系统特性:");
    println!("   - 基于现有BEY网络基础设施");
    println!("   - 集成设备发现、安全传输、文件传输");
    println!("   - 智能压缩和加密");
    println!("   - 分布式副本管理");
    println!("   - 统一的存储抽象层");

    Ok(())
}

/// 测试文件存储功能
async fn test_file_storage(storage: &BeyStorageManager) -> Result<(), ErrorInfo> {
    use bey_storage::StoreOptions;

    // 创建测试数据
    let test_data = "Hello, BEY Distributed Storage System! 这是一个基于现有基础设施的分布式存储测试文件。".repeat(50);
    let data_bytes = test_data.as_bytes().to_vec();

    // 存储文件
    let metadata = storage.store_file(
        Path::new("/demo/bey_test.txt"),
        data_bytes.clone(),
        StoreOptions {
            overwrite: true,
            tags: vec!["test".to_string(), "demo".to_string()],
            expires_at: None,
        }
    ).await?;

    println!("  ✓ 文件存储成功:");
    println!("    - 文件ID: {}", metadata.file_id);
    println!("    - 文件名: {}", metadata.filename);
    println!("    - 文件大小: {} bytes", metadata.size);
    println!("    - 创建时间: {:?}", metadata.created_at);

    if let Some(ref comp_info) = metadata.compression_info {
        println!("    - 压缩信息: {} -> {} bytes (节省 {:.1}%)",
                 comp_info.original_size,
                 comp_info.compressed_size,
                 (1.0 - comp_info.compression_ratio) * 100.0);
    }

    Ok(())
}

/// 测试文件读取功能
async fn test_file_reading(storage: &BeyStorageManager) -> Result<(), ErrorInfo> {
    use bey_storage::ReadOptions;

    // 读取文件
    let read_data = storage.read_file(
        Path::new("/demo/bey_test.txt"),
        ReadOptions {
            version: None,
            verify_integrity: true,
        }
    ).await?;

    println!("  ✓ 文件读取成功: {} bytes", read_data.len());

    // 验证数据完整性
    let expected_data = "Hello, BEY Distributed Storage System! 这是一个基于现有基础设施的分布式存储测试文件。".repeat(50);
    let expected_bytes = expected_data.as_bytes();

    if read_data == expected_bytes {
        println!("  ✓ 数据完整性验证通过");
    } else {
        return Err(ErrorInfo::new(7001, "数据完整性验证失败".to_string())
            .with_category(ErrorCategory::Validation));
    }

    Ok(())
}

/// 测试文件搜索功能
async fn test_file_search(storage: &BeyStorageManager) -> Result<(), ErrorInfo> {
    use bey_storage::SearchFilters;
    use std::time::SystemTime;

    // 搜索包含"bey"的文件
    let search_results = storage.search_files("bey", None).await?;
    println!("  ✓ 搜索 'bey' 找到 {} 个文件", search_results.len());

    // 使用过滤器搜索
    let filters = SearchFilters {
        mime_types: vec!["text/plain".to_string()],
        tags: vec!["test".to_string()],
        size_range: Some((100, 10000)),
        time_range: Some((SystemTime::UNIX_EPOCH, SystemTime::now())),
    };

    let filtered_results = storage.search_files("bey", Some(filters)).await?;
    println!("  ✓ 过滤搜索找到 {} 个文件", filtered_results.len());

    Ok(())
}

/// 打印存储统计信息
fn print_storage_statistics(stats: &bey_storage::StorageStatistics) {
    println!("📈 BEY分布式存储统计:");
    println!("  总文件数: {}", stats.total_files);
    println!("  总存储量: {} bytes", stats.total_size);
    println!("  在线节点数: {}", stats.online_nodes);
    println!("  可用空间: {} bytes", stats.available_space);
    println!("  功能状态:");
    println!("    - 压缩功能: {}", if stats.compression_enabled { "✓" } else { "✗" });
    println!("    - 加密功能: {}", if stats.encryption_enabled { "✓" } else { "✗" });
    println!("    - 副本数量: {}", stats.replica_count);
}

/// 列出所有文件
async fn list_all_files(storage: &BeyStorageManager) -> Result<(), ErrorInfo> {
    let files = storage.list_directory(Path::new("/"), true).await?;

    println!("  系统中共有 {} 个文件:", files.len());
    for (index, file) in files.iter().enumerate().take(5) {
        println!("    {}. {} ({} bytes, 标签: {:?})",
                 index + 1,
                 file.filename,
                 file.size,
                 file.tags);
    }

    if files.len() > 5 {
        println!("    ... 还有 {} 个文件", files.len() - 5);
    }

    Ok(())
}

/// 清理测试文件
async fn cleanup_test_files(storage: &BeyStorageManager) -> Result<(), ErrorInfo> {
    use bey_storage::DeleteOptions;

    let deleted = storage.delete_file(
        Path::new("/demo/bey_test.txt"),
        DeleteOptions {
            force: true,
            local_only: false,
        }
    ).await?;

    if deleted {
        println!("  ✓ 测试文件删除成功");
    } else {
        println!("  ⚠️ 测试文件删除失败或不存在");
    }

    Ok(())
}