//! # 错误代码常量模块
//!
//! 定义bey-transport包使用的所有错误代码常量，避免硬编码

/// mTLS管理器错误代码
pub mod mtls {
    /// 创建证书配置失败
    pub const CREATE_CERT_CONFIG_FAILED: u32 = 5001;
    /// 初始化证书管理器失败
    pub const INIT_CERT_MANAGER_FAILED: u32 = 5002;
    /// 配置缓存TTL无效
    pub const INVALID_CACHE_TTL: u32 = 5003;
    /// 生成服务器配置失败
    pub const GENERATE_SERVER_CONFIG_FAILED: u32 = 5004;
    /// 生成客户端配置失败
    pub const GENERATE_CLIENT_CONFIG_FAILED: u32 = 5005;
    /// 证书验证失败
    pub const CERT_VERIFICATION_FAILED: u32 = 5006;
    /// 证书吊销失败
    pub const CERT_REVOCATION_FAILED: u32 = 5007;
}

/// 策略引擎错误代码
pub mod policy {
    /// 无效的正则表达式
    pub const INVALID_REGEX: u32 = 6001;
    /// 策略集合不存在
    pub const POLICY_SET_NOT_FOUND: u32 = 6002;
    /// 策略评估失败
    pub const POLICY_EVALUATION_FAILED: u32 = 6003;
    /// 策略集合操作失败
    pub const POLICY_SET_OPERATION_FAILED: u32 = 6004;
}

/// 连接池错误代码
pub mod pool {
    /// 连接池已满
    pub const POOL_FULL: u32 = 7001;
    /// 连接创建失败
    pub const CONNECTION_CREATION_FAILED: u32 = 7002;
    /// 连接健康检查失败
    pub const HEALTH_CHECK_FAILED: u32 = 7003;
    /// 连接超时
    pub const CONNECTION_TIMEOUT: u32 = 7004;
}

/// 传输层错误代码
pub mod transport {
    /// 传输层初始化失败
    pub const INIT_FAILED: u32 = 2001;
    /// 创建mTLS管理器失败
    pub const CREATE_MTLS_MANAGER_FAILED: u32 = 2002;
    /// 连接失败
    pub const CONNECTION_FAILED: u32 = 2003;
    /// 发送消息失败
    pub const SEND_MESSAGE_FAILED: u32 = 2004;
    /// 接收消息失败
    pub const RECEIVE_MESSAGE_FAILED: u32 = 2005;
}
