//! # BEY 网络令牌系统
//!
//! 提供基于令牌的网络传输抽象，支持灵活的消息路由和处理。
//! 令牌是网络传输的基本单位，包含元数据和负载。
//!
//! ## 核心概念
//!
//! - **令牌(Token)**: 网络传输的基本单位，包含类型、元数据和负载
//! - **令牌元类(TokenMeta)**: 定义令牌的基本行为和属性
//! - **令牌处理器(TokenHandler)**: 处理特定类型令牌的处理器
//! - **令牌路由器(TokenRouter)**: 将令牌路由到对应的处理器

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::NetResult;

/// 令牌唯一标识符
pub type TokenId = String;

/// 令牌类型
pub type TokenType = String;

/// 令牌优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TokenPriority {
    /// 低优先级
    Low = 0,
    /// 正常优先级
    Normal = 1,
    /// 高优先级
    High = 2,
    /// 紧急优先级
    Critical = 3,
}

impl Default for TokenPriority {
    fn default() -> Self {
        TokenPriority::Normal
    }
}

/// 令牌元数据
///
/// 定义令牌的基本属性和元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    /// 令牌ID
    pub id: TokenId,
    /// 令牌类型
    pub token_type: TokenType,
    /// 发送者ID
    pub sender_id: String,
    /// 接收者ID（可选）
    pub receiver_id: Option<String>,
    /// 时间戳
    pub timestamp: u64,
    /// 优先级
    pub priority: TokenPriority,
    /// 是否需要确认
    pub requires_ack: bool,
    /// 是否加密
    pub encrypted: bool,
    /// 自定义属性
    pub attributes: HashMap<String, String>,
}

impl TokenMeta {
    /// 创建新的令牌元数据
    ///
    /// # 参数
    ///
    /// * `token_type` - 令牌类型
    /// * `sender_id` - 发送者ID
    ///
    /// # 返回值
    ///
    /// 返回新的令牌元数据
    pub fn new(token_type: TokenType, sender_id: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            token_type,
            sender_id,
            receiver_id: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            priority: TokenPriority::Normal,
            requires_ack: false,
            encrypted: false,
            attributes: HashMap::new(),
        }
    }

    /// 设置接收者ID
    pub fn with_receiver(mut self, receiver_id: String) -> Self {
        self.receiver_id = Some(receiver_id);
        self
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: TokenPriority) -> Self {
        self.priority = priority;
        self
    }

    /// 设置是否需要确认
    pub fn with_ack(mut self, requires_ack: bool) -> Self {
        self.requires_ack = requires_ack;
        self
    }

    /// 设置是否加密
    pub fn with_encryption(mut self, encrypted: bool) -> Self {
        self.encrypted = encrypted;
        self
    }

    /// 添加自定义属性
    pub fn with_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }
}

/// 网络令牌
///
/// 网络传输的基本单位，包含元数据和负载数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    /// 令牌元数据
    pub meta: TokenMeta,
    /// 负载数据
    pub payload: Vec<u8>,
}

impl Token {
    /// 创建新的令牌
    ///
    /// # 参数
    ///
    /// * `meta` - 令牌元数据
    /// * `payload` - 负载数据
    ///
    /// # 返回值
    ///
    /// 返回新的令牌
    pub fn new(meta: TokenMeta, payload: Vec<u8>) -> Self {
        Self { meta, payload }
    }

    /// 创建响应令牌
    ///
    /// # 参数
    ///
    /// * `request` - 请求令牌
    /// * `payload` - 响应负载
    ///
    /// # 返回值
    ///
    /// 返回响应令牌
    pub fn response(request: &Token, payload: Vec<u8>) -> Self {
        let mut meta = TokenMeta::new(
            format!("{}_response", request.meta.token_type),
            request.meta.receiver_id.clone().unwrap_or_default(),
        );
        meta.receiver_id = Some(request.meta.sender_id.clone());
        meta.priority = request.meta.priority;
        meta.encrypted = request.meta.encrypted;
        meta.attributes.insert("request_id".to_string(), request.meta.id.clone());

        Self::new(meta, payload)
    }

    /// 序列化令牌
    ///
    /// # 返回值
    ///
    /// 返回序列化后的字节数组或错误
    pub fn serialize(&self) -> NetResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| {
            ErrorInfo::new(4001, format!("令牌序列化失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
        })
    }

    /// 反序列化令牌
    ///
    /// # 参数
    ///
    /// * `data` - 字节数组
    ///
    /// # 返回值
    ///
    /// 返回令牌或错误
    pub fn deserialize(data: &[u8]) -> NetResult<Self> {
        serde_json::from_slice(data).map_err(|e| {
            ErrorInfo::new(4002, format!("令牌反序列化失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
        })
    }
}

/// 令牌处理器特征
///
/// 实现此特征以处理特定类型的令牌
#[async_trait]
pub trait TokenHandler: Send + Sync {
    /// 获取处理器支持的令牌类型
    fn token_types(&self) -> Vec<TokenType>;

    /// 处理令牌
    ///
    /// # 参数
    ///
    /// * `token` - 要处理的令牌
    ///
    /// # 返回值
    ///
    /// 返回处理结果（可选的响应令牌）或错误
    async fn handle_token(&self, token: Token) -> NetResult<Option<Token>>;

    /// 令牌处理前的验证
    ///
    /// # 参数
    ///
    /// * `token` - 要验证的令牌
    ///
    /// # 返回值
    ///
    /// 返回验证结果
    async fn validate_token(&self, token: &Token) -> NetResult<bool> {
        // 默认实现：始终验证通过
        Ok(true)
    }
}

/// 令牌路由器
///
/// 负责将令牌路由到对应的处理器
pub struct TokenRouter {
    /// 处理器映射表
    handlers: Arc<RwLock<HashMap<TokenType, Arc<dyn TokenHandler>>>>,
    /// 默认处理器
    default_handler: Option<Arc<dyn TokenHandler>>,
}

impl TokenRouter {
    /// 创建新的令牌路由器
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            default_handler: None,
        }
    }

    /// 注册令牌处理器
    ///
    /// # 参数
    ///
    /// * `handler` - 令牌处理器
    ///
    /// # 返回值
    ///
    /// 返回注册结果
    pub async fn register_handler(&self, handler: Arc<dyn TokenHandler>) -> NetResult<()> {
        let token_types = handler.token_types();
        let mut handlers = self.handlers.write().await;

        for token_type in token_types {
            handlers.insert(token_type.clone(), Arc::clone(&handler));
        }

        Ok(())
    }

    /// 设置默认处理器
    ///
    /// # 参数
    ///
    /// * `handler` - 默认处理器
    pub fn set_default_handler(&mut self, handler: Arc<dyn TokenHandler>) {
        self.default_handler = Some(handler);
    }

    /// 路由令牌到对应的处理器
    ///
    /// # 参数
    ///
    /// * `token` - 要路由的令牌
    ///
    /// # 返回值
    ///
    /// 返回处理结果或错误
    pub async fn route_token(&self, token: Token) -> NetResult<Option<Token>> {
        // 查找对应的处理器
        let handler = {
            let handlers = self.handlers.read().await;
            handlers.get(&token.meta.token_type).cloned()
        };

        // 如果找到处理器，使用它处理令牌
        if let Some(handler) = handler {
            // 验证令牌
            if !handler.validate_token(&token).await? {
                return Err(ErrorInfo::new(4003, "令牌验证失败".to_string())
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Warning));
            }

            // 处理令牌
            return handler.handle_token(token).await;
        }

        // 如果没有找到对应的处理器，使用默认处理器
        if let Some(default_handler) = &self.default_handler {
            return default_handler.handle_token(token).await;
        }

        // 没有可用的处理器
        Err(ErrorInfo::new(4004, format!("未找到令牌类型 {} 的处理器", token.meta.token_type))
            .with_category(ErrorCategory::NotImplemented)
            .with_severity(ErrorSeverity::Warning))
    }

    /// 注销令牌处理器
    ///
    /// # 参数
    ///
    /// * `token_type` - 令牌类型
    ///
    /// # 返回值
    ///
    /// 返回注销结果
    pub async fn unregister_handler(&self, token_type: &str) -> NetResult<()> {
        let mut handlers = self.handlers.write().await;
        handlers.remove(token_type);
        Ok(())
    }
}

impl Default for TokenRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_meta_creation() {
        let meta = TokenMeta::new("test_type".to_string(), "sender_123".to_string());
        
        assert_eq!(meta.token_type, "test_type");
        assert_eq!(meta.sender_id, "sender_123");
        assert_eq!(meta.priority, TokenPriority::Normal);
        assert!(!meta.requires_ack);
        assert!(!meta.encrypted);
    }

    #[test]
    fn test_token_meta_builder() {
        let meta = TokenMeta::new("test_type".to_string(), "sender_123".to_string())
            .with_receiver("receiver_456".to_string())
            .with_priority(TokenPriority::High)
            .with_ack(true)
            .with_encryption(true)
            .with_attribute("key".to_string(), "value".to_string());
        
        assert_eq!(meta.receiver_id, Some("receiver_456".to_string()));
        assert_eq!(meta.priority, TokenPriority::High);
        assert!(meta.requires_ack);
        assert!(meta.encrypted);
        assert_eq!(meta.attributes.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_token_serialization() {
        let meta = TokenMeta::new("test_type".to_string(), "sender_123".to_string());
        let token = Token::new(meta, vec![1, 2, 3, 4, 5]);
        
        let serialized = token.serialize().unwrap();
        let deserialized = Token::deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.meta.token_type, "test_type");
        assert_eq!(deserialized.meta.sender_id, "sender_123");
        assert_eq!(deserialized.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_token_priority_ordering() {
        assert!(TokenPriority::Critical > TokenPriority::High);
        assert!(TokenPriority::High > TokenPriority::Normal);
        assert!(TokenPriority::Normal > TokenPriority::Low);
    }

    #[tokio::test]
    async fn test_token_router() {
        let router = TokenRouter::new();
        assert!(router.handlers.read().await.is_empty());
    }
}
