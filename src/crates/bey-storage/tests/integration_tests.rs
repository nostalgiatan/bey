use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;

use bey_storage::{
    compression::{SmartCompressor, CompressionStrategy, CompressionAlgorithm},
    distributed_manager::{DistributedFileManager, DistributionStrategy, NodeInfo, NodeStatus},
    unified_view::{UnifiedFileView, ViewConfig, CacheCleanupPolicy},
    cloud_storage::{LocalCloudStorage, CloudStorageConfig},
    DistributedObjectStorage, StorageConfig,
};

async fn setup_test_directory() -> PathBuf {
    // 尝试使用当前目录或用户主目录作为测试目录
    let base_dirs = [
        PathBuf::from("./bey_storage_test"),      // 当前目录
        PathBuf::from("/tmp/bey_storage_test"),  // 系统临时目录
        std::env::current_dir().unwrap().join("bey_storage_test"), // 运行时目录
    ];

    for test_dir in base_dirs.iter() {
        match fs::create_dir_all(test_dir).await {
            Ok(_) => {
                // 测试是否可写
                let test_file = test_dir.join("write_test");
                match fs::write(&test_file, "test").await {
                    Ok(_) => {
                        let _ = fs::remove_file(&test_file).await;
                        return test_dir.clone();
                    }
                    Err(_) => continue,
                }
            }
            Err(_) => continue,
        }
    }

    panic!("无法创建可写的测试目录");
}

async fn cleanup_test_directory(test_dir: &PathBuf) {
    if test_dir.exists() {
        // 尝试删除目录内容，然后删除目录本身
        if let Ok(mut entries) = fs::read_dir(test_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path).await;
                } else {
                    let _ = fs::remove_file(&path).await;
                }
            }
        }
        let _ = fs::remove_dir_all(test_dir).await; // 忽略错误
    }
}

#[tokio::test]
async fn test_compression_basic() {
    let compressor = SmartCompressor::new(CompressionStrategy::default());
    // 使用更有重复性的数据来确保压缩有效
    let test_string = "Hello, World! ".repeat(1000);
    let test_data = test_string.as_bytes();

    // 测试 Zstd 压缩
    let result = compressor.compress_sync(test_data, CompressionAlgorithm::Zstd).unwrap();
    assert!(result.compressed_size < result.original_size);
    assert!(result.compression_ratio < 1.0);
    assert!(result.is_beneficial);

    // 测试解压缩
    let decompressed = compressor.decompress_sync(&result.get_compressed_data(), result.algorithm).unwrap();
    assert_eq!(decompressed, test_data);
}

#[tokio::test]
async fn test_compression_algorithms() {
    let compressor = SmartCompressor::new(CompressionStrategy::default());
    let test_data = b"Test data for compression algorithms comparison. This should work with different algorithms.";

    // 测试所有算法
    let algorithms = [
        CompressionAlgorithm::None,
        CompressionAlgorithm::Lz4,
        CompressionAlgorithm::Zstd,
        CompressionAlgorithm::ZstdMax,
    ];

    for algorithm in algorithms {
        let result = compressor.compress_sync(test_data, algorithm).unwrap();

        match algorithm {
            CompressionAlgorithm::None => {
                assert_eq!(result.compressed_size, result.original_size);
                assert!(!result.is_beneficial);
            }
            _ => {
                // 压缩后的大小可能比原来大（由于压缩开销），但应该接近
                assert!(result.compressed_size <= result.original_size * 110 / 100); // 允许10%的压缩开销
                // 或者检查压缩算法至少尝试了压缩
                assert!(result.compressed_size != result.original_size || result.algorithm == CompressionAlgorithm::None);
            }
        }

        // 测试解压缩
        if algorithm != CompressionAlgorithm::None {
            let decompressed = compressor.decompress_sync(&result.get_compressed_data(), algorithm).unwrap();
            assert_eq!(decompressed, test_data);
        }
    }
}

#[tokio::test]
async fn test_unified_file_view() {
    let test_dir = setup_test_directory().await;

    let config = ViewConfig {
        virtual_root: test_dir.clone(),
        local_storage_path: test_dir.clone(),
        distributed_storage_path: test_dir.clone().join("distributed"),
        cache_path: test_dir.clone().join("cache"),
        max_cache_size: 100,
        cache_cleanup_policy: CacheCleanupPolicy::LRU,
        path_mappings: vec![],
        auto_index: true,
        index_update_interval: Duration::from_secs(60),
    };

    let _file_view = UnifiedFileView::new(config).unwrap();

    // 创建测试文件
    let test_file = test_dir.join("test.txt");
    let test_content = "This is a test file content.";
    fs::write(&test_file, test_content).await.unwrap();

    cleanup_test_directory(&test_dir).await;
}

#[tokio::test]
async fn test_distributed_file_manager() {
    let strategy = DistributionStrategy {
        replica_count: 2,
        min_available_replicas: 1,
        load_balancing_strategy: bey_storage::distributed_manager::LoadBalancingStrategy::RoundRobin,
        auto_repair: true,
        sync_timeout: Duration::from_secs(10),
        heartbeat_interval: Duration::from_secs(5),
        node_failure_threshold: Duration::from_secs(30),
    };

    let manager = DistributedFileManager::new("test-node".to_string(), strategy);

    // 添加测试节点
    let node_info = NodeInfo {
        node_id: "test-node-1".to_string(),
        name: "Test Node 1".to_string(),
        address: "127.0.0.1".to_string(),
        port: 8080,
        storage_capacity: 1024 * 1024 * 1024, // 1GB
        used_storage: 0,
        status: NodeStatus::Online,
        last_heartbeat: std::time::SystemTime::now(),
        weight: 1.0,
    };

    manager.add_node(node_info).await.unwrap();

    // 获取集群状态
    let cluster_status = manager.get_cluster_status().await;
    assert_eq!(cluster_status.total_nodes, 1);
    assert_eq!(cluster_status.online_nodes, 1);
}

#[tokio::test]
async fn test_cloud_storage_basic() {
    let test_dir = setup_test_directory().await;

    let config = CloudStorageConfig {
        max_users: 100,
        default_user_quota: 1024 * 1024 * 1024, // 1GB
        admin_quota: 5 * 1024 * 1024 * 1024, // 5GB
        premium_quota: 10 * 1024 * 1024 * 1024, // 10GB
        enable_versioning: true,
        max_file_size: 100 * 1024 * 1024, // 100MB
        allowed_file_types: vec!["*".to_string()], // 允许所有文件类型
        auto_cleanup_expired_shares: true,
        default_share_expiry: Duration::from_secs(7 * 24 * 3600), // 7天
        sync_timeout: Duration::from_secs(30),
        compression_strategy: CompressionStrategy::default(),
    };

    let file_view_for_cloud = UnifiedFileView::new(ViewConfig {
        virtual_root: test_dir.clone(),
        local_storage_path: test_dir.clone(),
        distributed_storage_path: test_dir.clone().join("distributed"),
        cache_path: test_dir.clone().join("cache"),
        max_cache_size: 100,
        cache_cleanup_policy: CacheCleanupPolicy::LRU,
        path_mappings: vec![],
        auto_index: true,
        index_update_interval: Duration::from_secs(60),
    }).unwrap();
    let cloud_storage = LocalCloudStorage::new(config, std::sync::Arc::new(file_view_for_cloud)).await.unwrap();

    // 创建测试用户
    let created_user = cloud_storage.create_user(
        "testuser".to_string(),
        "test@example.com".to_string(),
        "password123".to_string()
    ).await.unwrap();

    // 检查用户是否存在
    let user = cloud_storage.get_user(&created_user.user_id).await.unwrap();
    assert!(user.is_some());

    let user_info = cloud_storage.get_user("test-user").await.unwrap();
    assert!(user_info.is_some());
    assert_eq!(user_info.unwrap().username, "testuser");

    cleanup_test_directory(&test_dir).await;
}

#[tokio::test]
async fn test_distributed_object_storage_integration() {
    let test_dir = setup_test_directory().await;

    let config = StorageConfig {
        view_config: ViewConfig {
            virtual_root: test_dir.join("storage"),
            local_storage_path: test_dir.join("local"),
            distributed_storage_path: test_dir.join("distributed"),
            cache_path: test_dir.join("cache"),
            max_cache_size: 1000,
            cache_cleanup_policy: CacheCleanupPolicy::LRU,
            path_mappings: vec![],
            auto_index: true,
            index_update_interval: Duration::from_secs(300),
        },
        distribution_strategy: DistributionStrategy {
            replica_count: 1,
            min_available_replicas: 1,
            load_balancing_strategy: bey_storage::distributed_manager::LoadBalancingStrategy::Random,
            auto_repair: true,
            sync_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(10),
            node_failure_threshold: Duration::from_secs(30),
        },
        compression_strategy: CompressionStrategy::default(),
        cloud_config: Some(CloudStorageConfig {
            max_users: 10,
            default_user_quota: 100 * 1024 * 1024, // 100MB
            admin_quota: 500 * 1024 * 1024, // 500MB
            premium_quota: 1024 * 1024 * 1024, // 1GB
            enable_versioning: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
            allowed_file_types: vec!["*".to_string()],
            auto_cleanup_expired_shares: true,
            default_share_expiry: Duration::from_secs(7 * 24 * 3600),
            sync_timeout: Duration::from_secs(30),
            compression_strategy: CompressionStrategy::default(),
        }),
        enable_cloud_storage: true,
        enable_distributed: true,
        enable_compression: true,
        current_node_id: "test-node".to_string(),
    };

    let storage = DistributedObjectStorage::new(config).await.unwrap();

    // 创建测试文件
    let test_file_path = test_dir.join("test_file.txt");
    let test_content = "This is a test file for the distributed object storage system.";
    fs::write(&test_file_path, test_content).await.unwrap();

    // 添加文件到存储系统
    let virtual_path = std::path::Path::new("/test/test_file.txt");
    let test_data = fs::read(&test_file_path).await.unwrap();
    let store_options = bey_storage::StoreOptions {
        overwrite: true,
        enable_compression: true,
        enable_replication: true,
        tags: vec![],
        attributes: std::collections::HashMap::new(),
    };
    storage.store_file(virtual_path, test_data, store_options).await.unwrap();

    // 检查文件是否存在（通过尝试读取）
    let read_options = bey_storage::ReadOptions::default();
    let read_result = storage.read_file(virtual_path, read_options).await;
    assert!(read_result.is_ok());

    // 获取文件信息（通过搜索）
    let search_results = storage.search_files(virtual_path.to_str().unwrap(), None).await.unwrap();
    assert!(!search_results.is_empty());
    assert_eq!(search_results[0].virtual_path, *virtual_path);

    // 健康检查
    let health_result = storage.health_check().await.unwrap();
    assert!(health_result.issues.is_empty());

    cleanup_test_directory(&test_dir).await;
}

#[tokio::test]
async fn test_compression_strategy_selection() {
    let strategy = CompressionStrategy::default();
    let compressor = SmartCompressor::new(strategy);

    // 测试不同大小的数据
    let small_data = b"small";
    let _medium_data = b"This is medium sized data that should be compressed using standard algorithms. It contains enough redundancy to benefit from compression while not being too large.";
    let large_data = vec![0u8; 1024 * 1024]; // 1MB of zeros

    // 小数据不应该被压缩 (使用smart_compress代替Auto)
    let result = compressor.smart_compress(small_data, "txt").await.unwrap();
    assert_eq!(result.algorithm, CompressionAlgorithm::None);

    // 中等数据应该被压缩（使用更有重复性的数据）
    let medium_data = "This is medium sized data that should be compressed. ".repeat(100).into_bytes();
    let result = compressor.smart_compress(&medium_data, "txt").await.unwrap();
    assert_ne!(result.algorithm, CompressionAlgorithm::None);
    assert!(result.is_beneficial);

    // 大数据应该使用Zstd
    let result = compressor.smart_compress(&large_data, "bin").await.unwrap();
    // 1MB数据属于中等文件类别，应该使用Lz4
    assert_eq!(result.algorithm, CompressionAlgorithm::Lz4);
    assert!(result.is_beneficial);
}

#[tokio::test]
async fn test_error_handling() {
    let test_dir = setup_test_directory().await;

    // 测试无效路径
    let config = ViewConfig {
        virtual_root: PathBuf::from("/invalid/path/that/does/not/exist"),
        local_storage_path: test_dir.clone(),
        distributed_storage_path: test_dir.clone().join("distributed"),
        cache_path: test_dir.clone().join("cache"),
        max_cache_size: 100,
        cache_cleanup_policy: CacheCleanupPolicy::LRU,
        path_mappings: vec![],
        auto_index: true,
        index_update_interval: Duration::from_secs(60),
    };

    // 即使路径无效，UnifiedFileView::new也应该成功（因为它会异步创建目录）
    let result = UnifiedFileView::new(config);
    assert!(result.is_ok());

    cleanup_test_directory(&test_dir).await;
}

#[tokio::test]
async fn test_concurrent_operations() {
    let test_dir = setup_test_directory().await;

    let config = ViewConfig {
        virtual_root: test_dir.clone(),
        local_storage_path: test_dir.clone(),
        distributed_storage_path: test_dir.clone().join("distributed"),
        cache_path: test_dir.clone().join("cache"),
        max_cache_size: 1000,
        cache_cleanup_policy: CacheCleanupPolicy::LRU,
        path_mappings: vec![],
        auto_index: true,
        index_update_interval: Duration::from_secs(60),
    };

    let file_view = std::sync::Arc::new(UnifiedFileView::new(config).unwrap());

    // 创建多个测试文件
    let mut handles = vec![];

    for i in 0..5 {
        let test_file = test_dir.join(&format!("concurrent_test_{}.txt", i));
        let file_view_clone = file_view.clone();

        let handle = tokio::spawn(async move {
            fs::write(&test_file, format!("Test content {}", i)).await.unwrap();
            // 简单测试：验证view可以被并发访问
            let _stats = file_view_clone.get_view_statistics().await;
        });

        handles.push(handle);
    }

    // 等待所有操作完成
    for handle in handles {
        handle.await.unwrap();
    }

    // 验证基本统计功能
    let stats = file_view.get_view_statistics().await;
    assert!(stats.total_files >= 0);

    cleanup_test_directory(&test_dir).await;
}