//! # 安全传输层集成测试
//!
//! 测试 SecureTransport 的核心功能

use bey_transport::{SecureTransport, TransportConfig, TransportMessage, TransportResult};
use std::time::Duration;

/// 初始化日志（仅执行一次）
fn init_logging() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    
    INIT.call_once(|| {
        // 尝试初始化日志，如果已经初始化则忽略错误
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();
    });
}

/// 创建测试用传输配置
async fn create_test_transport_config(port: u16) -> TransportResult<TransportConfig> {
    let temp_dir = std::env::temp_dir().join(format!("bey-test-{}", port));
    Ok(TransportConfig::new()
        .with_port(port)
        .with_certificates_dir(&temp_dir)
        .with_max_connections(10)
        .with_connection_timeout(Duration::from_secs(5)))
}

#[tokio::test]
async fn test_transport_config_creation() {
    init_logging();

    let config = TransportConfig::new()
        .with_port(8443)
        .with_max_connections(100)
        .with_require_client_cert(true);

    assert_eq!(config.port(), 8443);
    assert_eq!(config.max_connections(), 100);
    assert!(config.require_client_cert());
}

#[tokio::test]
async fn test_secure_transport_creation() {
    init_logging();

    let config = create_test_transport_config(0).await.expect("创建配置失败");
    let device_id = "test-device-001".to_string();

    let transport_result = SecureTransport::new(config, device_id).await;
    assert!(transport_result.is_ok(), "安全传输层创建应该成功");

    let transport = transport_result.expect("传输层创建失败");
    assert_eq!(transport.active_connections_count().await, 0);
}

#[tokio::test]
async fn test_certificate_operations() {
    init_logging();

    let config = create_test_transport_config(0).await.expect("创建配置失败");
    let device_id = "test-device-cert".to_string();

    let transport = SecureTransport::new(config, device_id).await.expect("传输层创建失败");

    // 测试证书更新
    let update_result = transport.update_certificates().await;
    assert!(update_result.is_ok(), "证书更新应该成功");

    // 测试获取mTLS统计信息
    let stats = transport.get_mtls_stats().await;
    assert!(stats.certificate_renewals > 0, "应该有证书更新统计");
}

#[tokio::test]
async fn test_policy_engine_integration() {
    init_logging();

    let config = create_test_transport_config(0).await.expect("创建配置失败");
    let device_id = "test-device-policy".to_string();

    let transport = SecureTransport::new(config, device_id).await.expect("传输层创建失败");

    // 测试获取策略引擎统计信息
    let stats = transport.get_policy_stats().await;
    assert_eq!(stats.policy_sets_count, 0, "初始策略集合数量应为0");
}

#[tokio::test]
async fn test_connection_management() {
    init_logging();

    let config = create_test_transport_config(0).await.expect("创建配置失败");
    let device_id = "test-device-conn".to_string();

    let transport = SecureTransport::new(config, device_id).await.expect("传输层创建失败");

    // 初始状态应该没有活跃连接
    assert_eq!(transport.active_connections_count().await, 0);
    assert!(transport.active_connections().await.is_empty());

    // 测试断开不存在的连接
    let fake_addr = "127.0.0.1:9999".parse().expect("地址解析失败");
    let disconnect_result = transport.disconnect(fake_addr).await;
    assert!(disconnect_result.is_err(), "断开不存在的连接应该失败");
}

#[tokio::test]
async fn test_message_serialization() {
    init_logging();

    let message = TransportMessage {
        id: "test-msg-001".to_string(),
        message_type: "test".to_string(),
        content: serde_json::json!({"key": "value"}),
        timestamp: std::time::SystemTime::now(),
        sender_id: "test-sender".to_string(),
        receiver_id: Some("test-receiver".to_string()),
    };

    // 测试消息序列化
    let serialized = serde_json::to_vec(&message);
    assert!(serialized.is_ok(), "消息序列化应该成功");

    // 测试消息反序列化
    let serialized_data = serialized.expect("序列化失败");
    let deserialized: TransportMessage = serde_json::from_slice(&serialized_data)
        .expect("消息反序列化应该成功");

    assert_eq!(deserialized.id, message.id);
    assert_eq!(deserialized.message_type, message.message_type);
    assert_eq!(deserialized.sender_id, message.sender_id);
}
