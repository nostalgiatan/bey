//! # 消息功能模块
//!
//! 提供基于网络的消息发送和接收功能，支持私信、群聊和广播。
//! 使用 Token 元类创建高级API。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::sync::Arc;
use bey_net::{TransportEngine, Token, TokenMeta, TokenHandler, NetResult};
use bey_storage::{UnifiedStorageManager, MessageType, Message};
use async_trait::async_trait;
use tracing::{info, debug};

use crate::FuncResult;

/// 消息令牌类型
const MESSAGE_TOKEN_TYPE: &str = "bey.message";
const MESSAGE_PRIVATE_TOKEN: &str = "bey.message.private";
const MESSAGE_GROUP_TOKEN: &str = "bey.message.group";
const MESSAGE_BROADCAST_TOKEN: &str = "bey.message.broadcast";

/// 消息功能模块
pub struct MessageFunc {
    device_id: String,
    engine: Arc<TransportEngine>,
    storage: Arc<UnifiedStorageManager>,
}

impl MessageFunc {
    /// 创建新的消息功能实例
    pub fn new(
        device_id: String,
        engine: Arc<TransportEngine>,
        storage: Arc<UnifiedStorageManager>,
    ) -> Self {
        Self {
            device_id,
            engine,
            storage,
        }
    }

    /// 注册消息处理器
    pub async fn register_handlers(&self, engine: &TransportEngine) -> FuncResult<()> {
        let handler = MessageHandler {
            device_id: self.device_id.clone(),
            storage: Arc::clone(&self.storage),
        };

        engine.register_handler(Arc::new(handler)).await
            .map_err(|e| ErrorInfo::new(7101, format!("注册消息处理器失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        info!("消息处理器已注册");
        Ok(())
    }

    /// 发送私信
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对方设备ID
    /// * `content` - 消息内容
    ///
    /// # 返回值
    ///
    /// 返回消息ID或错误
    pub async fn send_private_message(&self, peer_id: &str, content: &[u8]) -> FuncResult<String> {
        // 保存到本地存储
        let msg_id = self.storage.message.send_message(
            MessageType::Private,
            peer_id.to_string(),
            content.to_vec(),
            "text".to_string(),
        ).await
            .map_err(|e| ErrorInfo::new(7102, format!("保存消息失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        // 创建消息令牌
        let meta = TokenMeta::new(MESSAGE_PRIVATE_TOKEN.to_string(), self.device_id.clone())
            .with_receiver(peer_id.to_string());

        let mut payload = Vec::new();
        payload.extend_from_slice(msg_id.as_bytes());
        payload.push(0); // 分隔符
        payload.extend_from_slice(content);

        let token = Token::new(meta, payload);

        // 发送令牌
        self.engine.send_token(token).await
            .map_err(|e| ErrorInfo::new(7103, format!("发送消息失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        debug!("发送私信成功: {} -> {}", peer_id, msg_id);
        Ok(msg_id)
    }

    /// 发送群聊消息
    ///
    /// # 参数
    ///
    /// * `group_id` - 群组ID
    /// * `content` - 消息内容
    ///
    /// # 返回值
    ///
    /// 返回消息ID或错误
    pub async fn send_group_message(&self, group_id: &str, content: &[u8]) -> FuncResult<String> {
        // 保存到本地存储
        let msg_id = self.storage.message.send_message(
            MessageType::Group,
            group_id.to_string(),
            content.to_vec(),
            "text".to_string(),
        ).await
            .map_err(|e| ErrorInfo::new(7104, format!("保存消息失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        // 创建消息令牌
        let meta = TokenMeta::new(MESSAGE_GROUP_TOKEN.to_string(), self.device_id.clone());

        let mut payload = Vec::new();
        payload.extend_from_slice(group_id.as_bytes());
        payload.push(0); // 分隔符
        payload.extend_from_slice(msg_id.as_bytes());
        payload.push(0); // 分隔符
        payload.extend_from_slice(content);

        let token = Token::new(meta, payload);

        // 通过群组名称发送
        self.engine.send_to_group_by_name(group_id, token.payload.clone(), MESSAGE_GROUP_TOKEN).await
            .map_err(|e| ErrorInfo::new(7105, format!("发送群消息失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        debug!("发送群消息成功: {} -> {}", group_id, msg_id);
        Ok(msg_id)
    }

    /// 广播消息
    ///
    /// # 参数
    ///
    /// * `content` - 消息内容
    ///
    /// # 返回值
    ///
    /// 返回发送到的设备数量或错误
    pub async fn broadcast_message(&self, content: &[u8]) -> FuncResult<usize> {
        // 创建消息令牌
        let meta = TokenMeta::new(MESSAGE_BROADCAST_TOKEN.to_string(), self.device_id.clone());
        let token = Token::new(meta, content.to_vec());

        // 广播
        let count = self.engine.broadcast(token.payload, MESSAGE_BROADCAST_TOKEN).await
            .map_err(|e| ErrorInfo::new(7106, format!("广播消息失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        debug!("广播消息成功，发送到 {} 个设备", count);
        Ok(count)
    }
}

/// 消息处理器
struct MessageHandler {
    device_id: String,
    storage: Arc<UnifiedStorageManager>,
}

#[async_trait]
impl TokenHandler for MessageHandler {
    fn token_types(&self) -> Vec<String> {
        vec![
            MESSAGE_PRIVATE_TOKEN.to_string(),
            MESSAGE_GROUP_TOKEN.to_string(),
            MESSAGE_BROADCAST_TOKEN.to_string(),
        ]
    }

    async fn handle_token(&self, token: Token) -> NetResult<Option<Token>> {
        match token.meta.token_type.as_str() {
            MESSAGE_PRIVATE_TOKEN => {
                self.handle_private_message(token).await?;
            }
            MESSAGE_GROUP_TOKEN => {
                self.handle_group_message(token).await?;
            }
            MESSAGE_BROADCAST_TOKEN => {
                self.handle_broadcast_message(token).await?;
            }
            _ => {
                debug!("未知消息类型: {}", token.meta.token_type);
            }
        }

        Ok(None)
    }
}

impl MessageHandler {
    /// 处理私信
    async fn handle_private_message(&self, token: Token) -> NetResult<()> {
        // 解析payload
        let payload = &token.payload;
        if let Some(sep_pos) = payload.iter().position(|&b| b == 0) {
            let msg_id = String::from_utf8_lossy(&payload[..sep_pos]).to_string();
            let content = &payload[sep_pos + 1..];

            // 保存到本地存储
            let _ = self.storage.message.send_message(
                MessageType::Private,
                self.device_id.clone(),
                content.to_vec(),
                "text".to_string(),
            ).await;

            info!("收到私信: {} 来自 {}", msg_id, token.meta.sender_id);
        }

        Ok(())
    }

    /// 处理群消息
    async fn handle_group_message(&self, token: Token) -> NetResult<()> {
        // 解析payload
        let payload = &token.payload;
        let parts: Vec<&[u8]> = payload.split(|&b| b == 0).collect();
        
        if parts.len() >= 3 {
            let group_id = String::from_utf8_lossy(parts[0]).to_string();
            let msg_id = String::from_utf8_lossy(parts[1]).to_string();
            let content = parts[2];

            // 保存到本地存储
            let _ = self.storage.message.send_message(
                MessageType::Group,
                group_id.clone(),
                content.to_vec(),
                "text".to_string(),
            ).await;

            info!("收到群消息: {} 来自 {} (群组: {})", msg_id, token.meta.sender_id, group_id);
        }

        Ok(())
    }

    /// 处理广播消息
    async fn handle_broadcast_message(&self, token: Token) -> NetResult<()> {
        info!("收到广播消息 来自 {}: {} 字节", token.meta.sender_id, token.payload.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_message_func_creation() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let storage_path = temp_dir.path();

        let engine_config = bey_net::EngineConfig::default();
        let engine = bey_net::TransportEngine::new(engine_config).await.expect("创建引擎失败");
        
        let storage = bey_storage::UnifiedStorageManager::new(
            "test_device".to_string(),
            storage_path.to_path_buf(),
        ).await.expect("创建存储失败");

        let message_func = MessageFunc::new(
            "test_device".to_string(),
            Arc::new(engine),
            Arc::new(storage),
        );

        assert_eq!(message_func.device_id, "test_device");
    }
}
