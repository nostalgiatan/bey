//! # 消息同步模块
//!
//! 提供消息的同步功能，支持群聊、私信、差异同步。
//! 使用sled数据库进行持久化存储，通过bey-net模块进行实时同步。

use error::{ErrorInfo, ErrorCategory};
use sled::Db;
use std::sync::Arc;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

/// 消息同步结果类型
pub type MessageResult<T> = std::result::Result<T, ErrorInfo>;

/// 消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// 私信
    Private,
    /// 群聊
    Group,
}

/// 消息条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息ID
    pub id: String,
    /// 消息类型
    pub message_type: MessageType,
    /// 发送者设备ID
    pub sender_id: String,
    /// 接收者ID（私信时为设备ID，群聊时为群组ID）
    pub receiver_id: String,
    /// 消息内容
    pub content: Vec<u8>,
    /// 内容类型（text, image, file等）
    pub content_type: String,
    /// 时间戳
    pub timestamp: u64,
    /// 是否已读
    pub is_read: bool,
    /// 来源设备DNS ID
    pub source_device_id: String,
}

/// 消息同步事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageEvent {
    /// 新消息
    NewMessage(Message),
    /// 消息已读
    MarkAsRead { message_id: String, timestamp: u64 },
    /// 删除消息
    DeleteMessage { message_id: String, timestamp: u64 },
    /// 请求消息历史
    RequestHistory { 
        receiver_id: String, 
        since_timestamp: u64,
        message_type: MessageType,
    },
    /// 消息历史响应
    HistoryResponse { messages: Vec<Message> },
}

/// 消息管理器
pub struct MessageManager {
    /// 本地设备ID
    device_id: String,
    /// sled数据库
    db: Arc<Db>,
    /// 最大消息数
    max_messages: usize,
}

impl MessageManager {
    /// 创建新的消息管理器
    ///
    /// # 参数
    ///
    /// * `device_id` - 本地设备ID
    /// * `db_path` - sled数据库路径
    ///
    /// # 返回值
    ///
    /// 返回消息管理器实例或错误
    pub async fn new(device_id: String, db_path: std::path::PathBuf) -> MessageResult<Self> {
        // 创建数据库目录
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| ErrorInfo::new(6301, format!("创建数据库目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem))?;
        }

        // 打开sled数据库
        let db = sled::open(&db_path)
            .map_err(|e| ErrorInfo::new(6302, format!("打开数据库失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        info!("消息管理器初始化成功");
        Ok(Self {
            device_id,
            db: Arc::new(db),
            max_messages: 10000,
        })
    }

    /// 发送消息
    ///
    /// # 参数
    ///
    /// * `message_type` - 消息类型
    /// * `receiver_id` - 接收者ID
    /// * `content` - 消息内容
    /// * `content_type` - 内容类型
    ///
    /// # 返回值
    ///
    /// 返回消息ID或错误
    pub async fn send_message(
        &self,
        message_type: MessageType,
        receiver_id: String,
        content: Vec<u8>,
        content_type: String,
    ) -> MessageResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let message = Message {
            id: id.clone(),
            message_type,
            sender_id: self.device_id.clone(),
            receiver_id,
            content,
            content_type,
            timestamp,
            is_read: false,
            source_device_id: self.device_id.clone(),
        };

        // 序列化并存储
        let message_bytes = serde_json::to_vec(&message)
            .map_err(|e| ErrorInfo::new(6303, format!("序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        self.db.insert(id.as_bytes(), message_bytes)
            .map_err(|e| ErrorInfo::new(6304, format!("存储失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        // 限制消息数量
        let count = self.db.len();
        if count > self.max_messages {
            // 删除最旧的消息
            if let Some(oldest_key) = self.find_oldest_message_key() {
                let _ = self.db.remove(oldest_key);
            }
        }

        debug!("发送消息: {} (类型: {:?})", id, message_type);
        Ok(id)
    }

    /// 获取消息
    ///
    /// # 参数
    ///
    /// * `message_id` - 消息ID
    ///
    /// # 返回值
    ///
    /// 返回消息或错误
    pub async fn get_message(&self, message_id: &str) -> MessageResult<Message> {
        let message_bytes = self.db.get(message_id.as_bytes())
            .map_err(|e| ErrorInfo::new(6305, format!("查询失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6306, format!("消息不存在: {}", message_id))
                .with_category(ErrorCategory::Storage))?;

        let message: Message = serde_json::from_slice(&message_bytes)
            .map_err(|e| ErrorInfo::new(6307, format!("反序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        Ok(message)
    }

    /// 获取私信历史
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对方设备ID
    /// * `limit` - 最大返回数量（None表示全部）
    ///
    /// # 返回值
    ///
    /// 返回消息列表
    pub async fn get_private_messages(&self, peer_id: &str, limit: Option<usize>) -> Vec<Message> {
        let mut messages = Vec::new();

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(message) = serde_json::from_slice::<Message>(&value) {
                    if message.message_type == MessageType::Private 
                        && (message.receiver_id == peer_id || message.sender_id == peer_id) {
                        messages.push(message);
                    }
                }
            }
        }

        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        if let Some(limit) = limit {
            messages.truncate(limit);
        }

        messages
    }

    /// 获取群聊历史
    ///
    /// # 参数
    ///
    /// * `group_id` - 群组ID
    /// * `limit` - 最大返回数量（None表示全部）
    ///
    /// # 返回值
    ///
    /// 返回消息列表
    pub async fn get_group_messages(&self, group_id: &str, limit: Option<usize>) -> Vec<Message> {
        let mut messages = Vec::new();

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(message) = serde_json::from_slice::<Message>(&value) {
                    if message.message_type == MessageType::Group 
                        && message.receiver_id == group_id {
                        messages.push(message);
                    }
                }
            }
        }

        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        if let Some(limit) = limit {
            messages.truncate(limit);
        }

        messages
    }

    /// 标记消息为已读
    ///
    /// # 参数
    ///
    /// * `message_id` - 消息ID
    ///
    /// # 返回值
    ///
    /// 返回操作结果
    pub async fn mark_as_read(&self, message_id: &str) -> MessageResult<()> {
        let mut message = self.get_message(message_id).await?;
        message.is_read = true;

        let message_bytes = serde_json::to_vec(&message)
            .map_err(|e| ErrorInfo::new(6308, format!("序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        self.db.insert(message_id.as_bytes(), message_bytes)
            .map_err(|e| ErrorInfo::new(6309, format!("更新失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        debug!("标记消息已读: {}", message_id);
        Ok(())
    }

    /// 删除消息
    ///
    /// # 参数
    ///
    /// * `message_id` - 消息ID
    ///
    /// # 返回值
    ///
    /// 返回操作结果
    pub async fn delete_message(&self, message_id: &str) -> MessageResult<()> {
        self.db.remove(message_id.as_bytes())
            .map_err(|e| ErrorInfo::new(6310, format!("删除失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6311, format!("消息不存在: {}", message_id))
                .with_category(ErrorCategory::Storage))?;

        debug!("删除消息: {}", message_id);
        Ok(())
    }

    /// 处理同步事件
    ///
    /// # 参数
    ///
    /// * `event` - 同步事件
    ///
    /// # 返回值
    ///
    /// 返回处理结果
    pub async fn handle_sync_event(&self, event: MessageEvent) -> MessageResult<()> {
        match event {
            MessageEvent::NewMessage(message) => {
                self.merge_message(message).await?;
            }
            MessageEvent::MarkAsRead { message_id, .. } => {
                let _ = self.mark_as_read(&message_id).await;
            }
            MessageEvent::DeleteMessage { message_id, .. } => {
                let _ = self.delete_message(&message_id).await;
            }
            MessageEvent::RequestHistory { .. } => {
                // 这里应该通过net模块发送历史响应
                debug!("收到消息历史请求");
            }
            MessageEvent::HistoryResponse { messages } => {
                for message in messages {
                    self.merge_message(message).await?;
                }
            }
        }
        Ok(())
    }

    /// 合并远程消息
    async fn merge_message(&self, remote_message: Message) -> MessageResult<()> {
        // 检查消息是否已存在
        if self.db.contains_key(remote_message.id.as_bytes())
            .map_err(|e| ErrorInfo::new(6312, format!("查询失败: {}", e))
                .with_category(ErrorCategory::Database))? {
            return Ok(());
        }

        // 新消息，添加
        let message_bytes = serde_json::to_vec(&remote_message)
            .map_err(|e| ErrorInfo::new(6313, format!("序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        self.db.insert(remote_message.id.as_bytes(), message_bytes)
            .map_err(|e| ErrorInfo::new(6314, format!("存储失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        debug!("添加新的远程消息: {}", remote_message.id);
        Ok(())
    }

    /// 获取差异（自指定时间戳以来的消息）
    ///
    /// # 参数
    ///
    /// * `since_timestamp` - 起始时间戳
    ///
    /// # 返回值
    ///
    /// 返回差异消息列表
    pub async fn get_diff(&self, since_timestamp: u64) -> Vec<Message> {
        let mut diff = Vec::new();

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(message) = serde_json::from_slice::<Message>(&value) {
                    if message.timestamp > since_timestamp {
                        diff.push(message);
                    }
                }
            }
        }

        diff
    }

    /// 查找最旧消息的键
    fn find_oldest_message_key(&self) -> Option<Vec<u8>> {
        let mut oldest_key: Option<Vec<u8>> = None;
        let mut oldest_timestamp = u64::MAX;

        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                if let Ok(message) = serde_json::from_slice::<Message>(&value) {
                    if message.timestamp < oldest_timestamp {
                        oldest_timestamp = message.timestamp;
                        oldest_key = Some(key.to_vec());
                    }
                }
            }
        }

        oldest_key
    }

    /// 清空所有消息
    pub async fn clear(&self) -> MessageResult<()> {
        self.db.clear()
            .map_err(|e| ErrorInfo::new(6315, format!("清空失败: {}", e))
                .with_category(ErrorCategory::Database))?;
        
        info!("清空所有消息");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_message_manager_send_and_get() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("messages.db");
        
        let manager = MessageManager::new("device1".to_string(), db_path).await
            .expect("创建管理器失败");

        // 发送私信
        let message_id = manager.send_message(
            MessageType::Private,
            "device2".to_string(),
            b"Hello".to_vec(),
            "text".to_string(),
        ).await.expect("发送失败");

        // 获取消息
        let message = manager.get_message(&message_id).await.expect("获取失败");
        assert_eq!(message.content, b"Hello");
        assert_eq!(message.message_type, MessageType::Private);
        assert_eq!(message.receiver_id, "device2");
    }

    #[tokio::test]
    async fn test_message_manager_private_messages() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("messages.db");
        
        let manager = MessageManager::new("device1".to_string(), db_path).await
            .expect("创建管理器失败");

        // 发送多条私信
        for i in 0..5 {
            manager.send_message(
                MessageType::Private,
                "device2".to_string(),
                format!("Message {}", i).into_bytes(),
                "text".to_string(),
            ).await.expect("发送失败");
        }

        // 获取私信历史
        let messages = manager.get_private_messages("device2", Some(3)).await;
        assert_eq!(messages.len(), 3);
    }

    #[tokio::test]
    async fn test_message_manager_persistence() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("messages.db");
        
        // 创建管理器并发送消息
        let msg_id = {
            let manager = MessageManager::new("device1".to_string(), db_path.clone()).await
                .expect("创建管理器失败");
            manager.send_message(
                MessageType::Group,
                "group1".to_string(),
                b"Test".to_vec(),
                "text".to_string(),
            ).await.expect("发送失败")
        };

        // 创建新管理器，应该能加载之前的数据
        {
            let manager = MessageManager::new("device1".to_string(), db_path).await
                .expect("创建管理器失败");
            let message = manager.get_message(&msg_id).await.expect("获取失败");
            assert_eq!(message.content, b"Test");
        }
    }
}
