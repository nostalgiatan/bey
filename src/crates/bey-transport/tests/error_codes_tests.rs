//! # 错误代码模块测试
//!
//! 测试错误代码常量的正确性和一致性

use bey_transport::error_codes::*;

#[test]
fn test_mtls_error_codes_uniqueness() {
    // 测试 mTLS 错误代码的唯一性
    let codes = vec![
        mtls::CREATE_CERT_CONFIG_FAILED,
        mtls::INIT_CERT_MANAGER_FAILED,
        mtls::INVALID_CACHE_TTL,
        mtls::GENERATE_SERVER_CONFIG_FAILED,
        mtls::GENERATE_CLIENT_CONFIG_FAILED,
        mtls::CERT_VERIFICATION_FAILED,
        mtls::CERT_REVOCATION_FAILED,
    ];
    
    // 检查是否有重复
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(
                codes[i], codes[j],
                "错误代码不应重复: {} == {}",
                codes[i], codes[j]
            );
        }
    }
}

#[test]
fn test_policy_error_codes_uniqueness() {
    // 测试策略引擎错误代码的唯一性
    let codes = vec![
        policy::INVALID_REGEX,
        policy::POLICY_SET_NOT_FOUND,
        policy::POLICY_EVALUATION_FAILED,
        policy::POLICY_SET_OPERATION_FAILED,
    ];
    
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(
                codes[i], codes[j],
                "错误代码不应重复: {} == {}",
                codes[i], codes[j]
            );
        }
    }
}

#[test]
fn test_pool_error_codes_uniqueness() {
    // 测试连接池错误代码的唯一性
    let codes = vec![
        pool::POOL_FULL,
        pool::CONNECTION_CREATION_FAILED,
        pool::HEALTH_CHECK_FAILED,
        pool::CONNECTION_TIMEOUT,
    ];
    
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(
                codes[i], codes[j],
                "错误代码不应重复: {} == {}",
                codes[i], codes[j]
            );
        }
    }
}

#[test]
fn test_transport_error_codes_uniqueness() {
    // 测试传输层错误代码的唯一性
    let codes = vec![
        transport::INIT_FAILED,
        transport::CREATE_MTLS_MANAGER_FAILED,
        transport::CONNECTION_FAILED,
        transport::SEND_MESSAGE_FAILED,
        transport::RECEIVE_MESSAGE_FAILED,
    ];
    
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(
                codes[i], codes[j],
                "错误代码不应重复: {} == {}",
                codes[i], codes[j]
            );
        }
    }
}

#[test]
fn test_error_code_ranges() {
    // 测试错误代码范围不重叠
    assert!(transport::INIT_FAILED >= 2000 && transport::INIT_FAILED < 3000);
    assert!(mtls::CREATE_CERT_CONFIG_FAILED >= 5000 && mtls::CREATE_CERT_CONFIG_FAILED < 6000);
    assert!(policy::INVALID_REGEX >= 6000 && policy::INVALID_REGEX < 7000);
    assert!(pool::POOL_FULL >= 7000 && pool::POOL_FULL < 8000);
}

#[test]
fn test_specific_error_code_values() {
    // 测试特定错误代码的值
    assert_eq!(mtls::CREATE_CERT_CONFIG_FAILED, 5001);
    assert_eq!(mtls::INIT_CERT_MANAGER_FAILED, 5002);
    assert_eq!(mtls::INVALID_CACHE_TTL, 5003);
    
    assert_eq!(policy::INVALID_REGEX, 6001);
    assert_eq!(policy::POLICY_SET_NOT_FOUND, 6002);
    assert_eq!(policy::POLICY_EVALUATION_FAILED, 6003);
    
    assert_eq!(pool::POOL_FULL, 7001);
    assert_eq!(pool::CONNECTION_CREATION_FAILED, 7002);
    
    assert_eq!(transport::INIT_FAILED, 2001);
    assert_eq!(transport::CREATE_MTLS_MANAGER_FAILED, 2002);
}
