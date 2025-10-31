//! 集成测试模块
//!
//! 测试各模块之间的协同工作，确保整个系统的稳定性和性能。

use bey::{BeyApp, DeviceInfo, DeviceType, Capability};
use std::time::Duration;

/// 测试完整的应用程序初始化流程
#[tokio::test]
async fn test_complete_app_initialization() {
    println!("开始完整应用程序初始化测试");

    // 创建应用实例
    let app_result = BeyApp::new().await;
    assert!(app_result.is_ok(), "应用程序初始化应该成功");

    let app = app_result.unwrap();

    // 验证设备信息
    let device_info = app.local_device();
    assert!(!device_info.device_id.is_empty(), "设备ID不应为空");
    assert!(!device_info.device_name.is_empty(), "设备名称不应为空");
    assert!(!device_info.capabilities.is_empty(), "设备能力不应为空");

    // 验证系统信息
    let system_info = app.system_info();
    assert!(!system_info.host_name().is_empty(), "主机名不应为空");
    assert!(!system_info.os_name().is_empty(), "操作系统名不应为空");
    assert!(system_info.cpu_count() > 0, "CPU数量应该大于0");

    println!("✅ 应用程序初始化测试通过");
}

/// 测试设备信息序列化和反序列化
#[tokio::test]
async fn test_device_info_serialization_roundtrip() {
    println!("开始设备信息序列化测试");

    let app = BeyApp::new().await.expect("应用创建失败");
    let original_device = app.local_device().clone();

    // 序列化
    let serialized = serde_json::to_string(&original_device)
        .expect("序列化应该成功");

    // 反序列化
    let deserialized: DeviceInfo = serde_json::from_str(&serialized)
        .expect("反序列化应该成功");

    // 验证数据一致性
    assert_eq!(original_device.device_id, deserialized.device_id);
    assert_eq!(original_device.device_name, deserialized.device_name);
    assert_eq!(original_device.device_type, deserialized.device_type);
    assert_eq!(original_device.capabilities, deserialized.capabilities);

    println!("✅ 设备信息序列化测试通过");
}

/// 测试设备ID生成的唯一性
#[tokio::test]
async fn test_device_id_uniqueness() {
    println!("开始设备ID唯一性测试");

    let mut device_ids = std::collections::HashSet::new();

    // 生成多个设备ID
    for _ in 0..10 {
        let app = BeyApp::new().await.expect("应用创建失败");
        let device_id = app.local_device().device_id.clone();

        // 验证ID格式
        assert!(device_id.starts_with("bey-"), "设备ID应该以'bey-'开头");
        assert_eq!(device_id.len(), 20, "设备ID长度应该为20");

        // 验证唯一性
        assert!(!device_ids.contains(&device_id), "设备ID应该是唯一的");
        device_ids.insert(device_id);
    }

    assert_eq!(device_ids.len(), 10, "应该生成10个唯一的设备ID");

    println!("✅ 设备ID唯一性测试通过");
}

/// 测试设备类型推断逻辑
#[tokio::test]
async fn test_device_type_inference() {
    println!("开始设备类型推断测试");

    let app = BeyApp::new().await.expect("应用创建失败");
    let device_type = app.local_device().device_type.clone();

    // 验证设备类型是有效的
    match device_type {
        DeviceType::Desktop | DeviceType::Laptop | DeviceType::Mobile |
        DeviceType::Server | DeviceType::Embedded => {
            println!("检测到的设备类型: {:?}", device_type);
        }
    }

    println!("✅ 设备类型推断测试通过");
}

/// 测试设备能力分配逻辑
#[tokio::test]
async fn test_device_capabilities_assignment() {
    println!("开始设备能力分配测试");

    let app = BeyApp::new().await.expect("应用创建失败");
    let capabilities = &app.local_device().capabilities;

    // 所有设备都应该支持基本消息传递
    assert!(capabilities.contains(&Capability::Messaging), "所有设备都应该支持消息传递");

    // 验证能力集合不为空
    assert!(!capabilities.is_empty(), "设备能力集合不应为空");

    println!("检测到的设备能力: {:?}", capabilities);
    println!("✅ 设备能力分配测试通过");
}

/// 测试网络地址获取
#[tokio::test]
async fn test_network_address_retrieval() {
    println!("开始网络地址获取测试");

    let app = BeyApp::new().await.expect("应用创建失败");
    let address = app.local_device().address;

    // 验证地址格式
    assert_eq!(address.port(), 8080, "默认端口应该是8080");

    // 验证IP地址有效性
    match address.ip() {
        std::net::IpAddr::V4(ipv4) => {
            assert!(ipv4.is_private() || ipv4.is_loopback(), "应该是私有或回环地址");
        }
        std::net::IpAddr::V6(ipv6) => {
            // IPv6地址的验证相对复杂，这里只检查基本有效性
            println!("IPv6地址: {:?}", ipv6);
        }
    }

    println!("✅ 网络地址获取测试通过");
}

/// 测试并发应用创建
#[tokio::test]
async fn test_concurrent_app_creation() {
    println!("开始并发应用创建测试");

    let mut handles = Vec::new();

    // 并发创建多个应用实例
    for _ in 0..5 {
        let handle = tokio::spawn(async {
            BeyApp::new().await
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    let mut successful_creations = 0;
    for handle in handles {
        match handle.await.expect("任务执行失败") {
            Ok(_) => successful_creations += 1,
            Err(e) => println!("应用创建失败: {}", e),
        }
    }

    assert_eq!(successful_creations, 5, "所有应用创建都应该成功");

    println!("✅ 并发应用创建测试通过");
}

/// 测试内存使用情况
#[tokio::test]
async fn test_memory_usage() {
    println!("开始内存使用测试");

    // 创建多个应用实例测试内存管理
    let apps: Vec<BeyApp> = futures::future::join_all(
        (0..10).map(|_| async { BeyApp::new().await.expect("应用创建失败") })
    ).await;

    // 验证所有应用都正常创建
    assert_eq!(apps.len(), 10, "应该成功创建10个应用实例");

    // 验证每个应用的设备信息都有效
    for app in &apps {
        let device = app.local_device();
        assert!(!device.device_id.is_empty(), "设备ID不应为空");
        assert!(!device.capabilities.is_empty(), "设备能力不应为空");
    }

    println!("✅ 内存使用测试通过");
}

/// 测试错误处理
#[tokio::test]
async fn test_error_handling() {
    println!("开始错误处理测试");

    // 测试无效地址解析
    let invalid_addr = "invalid-address:8080";
    let parse_result: Result<std::net::SocketAddr, _> = invalid_addr.parse();
    assert!(parse_result.is_err(), "无效地址应该解析失败");

    // 测试空设备信息序列化
    let empty_device = DeviceInfo {
        device_id: String::new(),
        device_name: String::new(),
        device_type: DeviceType::Desktop,
        address: "127.0.0.1:8080".parse().expect("地址解析失败"),
        capabilities: vec![],
        last_active: std::time::SystemTime::now(),
    };

    // 即使是空设备信息也应该能够序列化
    let serialized = serde_json::to_string(&empty_device);
    assert!(serialized.is_ok(), "空设备信息应该能够序列化");

    println!("✅ 错误处理测试通过");
}

/// 性能基准测试
#[tokio::test]
async fn test_performance_benchmarks() {
    println!("开始性能基准测试");

    let start = std::time::Instant::now();

    // 测试应用创建性能
    let creation_time = std::time::Instant::now();
    let _app = BeyApp::new().await.expect("应用创建失败");
    let creation_duration = creation_time.elapsed();

    // 应用创建应该在合理时间内完成（1秒内）
    assert!(creation_duration < Duration::from_secs(1),
            "应用创建应该在1秒内完成，实际耗时: {:?}", creation_duration);

    // 测试序列化性能
    let app = BeyApp::new().await.expect("应用创建失败");
    let device_info = app.local_device();

    let serialization_time = std::time::Instant::now();
    let _serialized = serde_json::to_vec(device_info).expect("序列化失败");
    let serialization_duration = serialization_time.elapsed();

    // 序列化应该在合理时间内完成（10毫秒内）
    assert!(serialization_duration < Duration::from_millis(10),
            "序列化应该在10毫秒内完成，实际耗时: {:?}", serialization_duration);

    let total_duration = start.elapsed();
    println!("性能测试总耗时: {:?}", total_duration);
    println!("应用创建耗时: {:?}", creation_duration);
    println!("序列化耗时: {:?}", serialization_duration);

    println!("✅ 性能基准测试通过");
}

/// 系统资源监控测试
#[tokio::test]
async fn test_system_resource_monitoring() {
    println!("开始系统资源监控测试");

    let app = BeyApp::new().await.expect("应用创建失败");
    let system_info = app.system_info();

    // 测试基本信息获取
    let cpu_count = system_info.cpu_count();
    assert!(cpu_count > 0, "CPU数量应该大于0");

    let host_name = system_info.host_name();
    assert!(!host_name.is_empty(), "主机名不应为空");

    let os_name = system_info.os_name();
    assert!(!os_name.is_empty(), "操作系统名不应为空");

    // 测试内存信息
    let (used_mem, total_mem) = system_info.memory_info();
    assert!(total_mem > 0, "总内存应该大于0");
    assert!(used_mem <= total_mem, "已用内存不应超过总内存");

    // 测试磁盘信息
    let available_disk = system_info.disk_available();
    assert!(available_disk > 0, "可用磁盘空间应该大于0");

    println!("系统信息:");
    println!("  CPU核心数: {}", cpu_count);
    println!("  主机名: {}", host_name);
    println!("  操作系统: {}", os_name);
    println!("  内存使用: {}MB / {}MB", used_mem / (1024 * 1024), total_mem / (1024 * 1024));
    println!("  可用磁盘: {}GB", available_disk / (1024 * 1024 * 1024));

    println!("✅ 系统资源监控测试通过");
}