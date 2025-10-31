//! # 剪切板功能模块
//!
//! 提供基于网络的剪切板同步功能，支持点对点、群组和广播同步。
//! 实现差异同步和冲突解决。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::sync::Arc;
use bey_net::{TransportEngine, Token, TokenMeta, TokenHandler, NetResult};
use bey_storage::{UnifiedStorageManager, ClipboardEntry, ClipboardEvent};
use async_trait::async_trait;
use tracing::{info, debug};

use crate::FuncResult;

/// 剪切板令牌类型
const CLIPBOARD_ADD_TOKEN: &str = "bey.clipboard.add";
const CLIPBOARD_DELETE_TOKEN: &str = "bey.clipboard.delete";
const CLIPBOARD_SYNC_TOKEN: &str = "bey.clipboard.sync";
const CLIPBOARD_DIFF_TOKEN: &str = "bey.clipboard.diff";

/// 剪切板功能模块
pub struct ClipboardFunc {
    device_id: String,
    engine: Arc<TransportEngine>,
    storage: Arc<UnifiedStorageManager>,
}

impl ClipboardFunc {
    /// 创建新的剪切板功能实例
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

    /// 注册剪切板处理器
    pub async fn register_handlers(&self, engine: &TransportEngine) -> FuncResult<()> {
        let handler = ClipboardHandler {
            device_id: self.device_id.clone(),
            storage: Arc::clone(&self.storage),
        };

        engine.register_handler(Arc::new(handler)).await
            .map_err(|e| ErrorInfo::new(7201, format!("注册剪切板处理器失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        info!("剪切板处理器已注册");
        Ok(())
    }

    /// 添加剪切板内容
    ///
    /// # 参数
    ///
    /// * `content_type` - 内容类型
    /// * `content` - 内容数据
    ///
    /// # 返回值
    ///
    /// 返回剪切板条目ID或错误
    pub async fn add_clipboard(&self, content_type: &str, content: &[u8]) -> FuncResult<String> {
        // 保存到本地存储
        let entry_id = self.storage.clipboard.add_entry(
            content.to_vec(),
            content_type.to_string(),
        ).await
            .map_err(|e| ErrorInfo::new(7202, format!("添加剪切板失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        debug!("添加剪切板条目: {}", entry_id);
        Ok(entry_id)
    }

    /// 删除剪切板条目
    ///
    /// # 参数
    ///
    /// * `entry_id` - 条目ID
    ///
    /// # 返回值
    ///
    /// 返回删除结果
    pub async fn delete_clipboard(&self, entry_id: &str) -> FuncResult<()> {
        self.storage.clipboard.delete_entry(entry_id).await
            .map_err(|e| ErrorInfo::new(7203, format!("删除剪切板失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        debug!("删除剪切板条目: {}", entry_id);
        Ok(())
    }

    /// 同步剪切板到对等设备
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对等设备ID
    ///
    /// # 返回值
    ///
    /// 返回同步结果
    pub async fn sync_to_peer(&self, peer_id: &str) -> FuncResult<()> {
        // 获取所有剪切板条目
        let entries = self.storage.clipboard.list_entries().await;

        // 序列化条目列表
        let entries_json = serde_json::to_vec(&entries)
            .map_err(|e| ErrorInfo::new(7204, format!("序列化剪切板失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 创建同步令牌
        let meta = TokenMeta::new(CLIPBOARD_SYNC_TOKEN.to_string(), self.device_id.clone())
            .with_receiver(peer_id.to_string());

        let token = Token::new(meta, entries_json);

        // 发送令牌
        self.engine.send_token(token).await
            .map_err(|e| ErrorInfo::new(7205, format!("发送剪切板同步失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        info!("同步剪切板到对等设备: {} ({} 个条目)", peer_id, entries.len());
        Ok(())
    }

    /// 同步剪切板到群组
    ///
    /// # 参数
    ///
    /// * `group_id` - 群组ID
    ///
    /// # 返回值
    ///
    /// 返回同步结果
    pub async fn sync_to_group(&self, group_id: &str) -> FuncResult<()> {
        // 获取所有剪切板条目
        let entries = self.storage.clipboard.list_entries().await;

        // 序列化条目列表
        let entries_json = serde_json::to_vec(&entries)
            .map_err(|e| ErrorInfo::new(7206, format!("序列化剪切板失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 通过群组发送
        self.engine.send_to_group_by_name(group_id, entries_json, CLIPBOARD_SYNC_TOKEN).await
            .map_err(|e| ErrorInfo::new(7207, format!("发送群组剪切板同步失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        info!("同步剪切板到群组: {} ({} 个条目)", group_id, entries.len());
        Ok(())
    }

    /// 发送剪切板差异
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对等设备ID
    /// * `since_timestamp` - 起始时间戳
    ///
    /// # 返回值
    ///
    /// 返回同步结果
    pub async fn send_diff_to_peer(&self, peer_id: &str, since_timestamp: u64) -> FuncResult<()> {
        // 获取差异
        let diff = self.storage.clipboard.get_diff(since_timestamp).await;

        if diff.is_empty() {
            debug!("没有剪切板差异需要同步");
            return Ok(());
        }

        // 序列化差异
        let diff_json = serde_json::to_vec(&diff)
            .map_err(|e| ErrorInfo::new(7208, format!("序列化差异失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 创建差异令牌
        let meta = TokenMeta::new(CLIPBOARD_DIFF_TOKEN.to_string(), self.device_id.clone())
            .with_receiver(peer_id.to_string());

        let token = Token::new(meta, diff_json);

        // 发送令牌
        self.engine.send_token(token).await
            .map_err(|e| ErrorInfo::new(7209, format!("发送差异失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        info!("发送剪切板差异到: {} ({} 个条目)", peer_id, diff.len());
        Ok(())
    }
}

/// 剪切板处理器
struct ClipboardHandler {
    device_id: String,
    storage: Arc<UnifiedStorageManager>,
}

#[async_trait]
impl TokenHandler for ClipboardHandler {
    fn token_types(&self) -> Vec<String> {
        vec![
            CLIPBOARD_ADD_TOKEN.to_string(),
            CLIPBOARD_DELETE_TOKEN.to_string(),
            CLIPBOARD_SYNC_TOKEN.to_string(),
            CLIPBOARD_DIFF_TOKEN.to_string(),
        ]
    }

    async fn handle_token(&self, token: Token) -> NetResult<Option<Token>> {
        match token.meta.token_type.as_str() {
            CLIPBOARD_SYNC_TOKEN => {
                self.handle_sync(token).await?;
            }
            CLIPBOARD_DIFF_TOKEN => {
                self.handle_diff(token).await?;
            }
            CLIPBOARD_ADD_TOKEN => {
                self.handle_add(token).await?;
            }
            CLIPBOARD_DELETE_TOKEN => {
                self.handle_delete(token).await?;
            }
            _ => {
                debug!("未知剪切板令牌类型: {}", token.meta.token_type);
            }
        }

        Ok(None)
    }
}

impl ClipboardHandler {
    /// 处理完整同步
    async fn handle_sync(&self, token: Token) -> NetResult<()> {
        // 反序列化条目列表
        let entries: Vec<ClipboardEntry> = serde_json::from_slice(&token.payload)
            .map_err(|e| ErrorInfo::new(7210, format!("反序列化剪切板失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 合并所有条目
        for entry in entries {
            let event = ClipboardEvent::Add(entry);
            let _ = self.storage.clipboard.handle_sync_event(event).await;
        }

        info!("处理剪切板同步 来自 {}", token.meta.sender_id);
        Ok(())
    }

    /// 处理差异同步
    async fn handle_diff(&self, token: Token) -> NetResult<()> {
        // 反序列化差异
        let diff: Vec<ClipboardEntry> = serde_json::from_slice(&token.payload)
            .map_err(|e| ErrorInfo::new(7211, format!("反序列化差异失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 合并差异
        for entry in diff {
            let event = ClipboardEvent::Update(entry);
            let _ = self.storage.clipboard.handle_sync_event(event).await;
        }

        info!("处理剪切板差异 来自 {}", token.meta.sender_id);
        Ok(())
    }

    /// 处理添加操作
    async fn handle_add(&self, token: Token) -> NetResult<()> {
        debug!("处理剪切板添加 来自 {}", token.meta.sender_id);
        Ok(())
    }

    /// 处理删除操作
    async fn handle_delete(&self, token: Token) -> NetResult<()> {
        debug!("处理剪切板删除 来自 {}", token.meta.sender_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_clipboard_func_creation() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let storage_path = temp_dir.path();

        let engine_config = bey_net::EngineConfig::default();
        let engine = bey_net::TransportEngine::new(engine_config).await.expect("创建引擎失败");
        
        let storage = bey_storage::UnifiedStorageManager::new(
            "test_device".to_string(),
            storage_path.to_path_buf(),
        ).await.expect("创建存储失败");

        let clipboard_func = ClipboardFunc::new(
            "test_device".to_string(),
            Arc::new(engine),
            Arc::new(storage),
        );

        assert_eq!(clipboard_func.device_id, "test_device");
    }
}
