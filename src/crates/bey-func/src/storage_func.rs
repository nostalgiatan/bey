//! # 存储功能模块
//!
//! 提供基于网络的文件传输和云存储功能。
//! 支持点对点文件传输、云存储分发。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::sync::Arc;
use bey_net::{TransportEngine, Token, TokenMeta, TokenHandler, NetResult};
use bey_storage::UnifiedStorageManager;
use async_trait::async_trait;
use tracing::{info, debug};

use crate::FuncResult;

/// 存储令牌类型
const STORAGE_FILE_TRANSFER_TOKEN: &str = "bey.storage.file";
const STORAGE_CLOUD_UPLOAD_TOKEN: &str = "bey.storage.cloud.upload";
const STORAGE_CLOUD_DOWNLOAD_TOKEN: &str = "bey.storage.cloud.download";
const STORAGE_CLOUD_NOTIFY_TOKEN: &str = "bey.storage.cloud.notify";

/// 存储功能模块
pub struct StorageFunc {
    device_id: String,
    engine: Arc<TransportEngine>,
    storage: Arc<UnifiedStorageManager>,
}

impl StorageFunc {
    /// 创建新的存储功能实例
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

    /// 注册存储处理器
    pub async fn register_handlers(&self, engine: &TransportEngine) -> FuncResult<()> {
        let handler = StorageHandler {
            device_id: self.device_id.clone(),
            storage: Arc::clone(&self.storage),
        };

        engine.register_handler(Arc::new(handler)).await
            .map_err(|e| ErrorInfo::new(7301, format!("注册存储处理器失败: {}", e))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error))?;

        info!("存储处理器已注册");
        Ok(())
    }

    /// 上传文件到云存储
    ///
    /// # 参数
    ///
    /// * `filename` - 文件名
    /// * `data` - 文件数据
    ///
    /// # 返回值
    ///
    /// 返回文件哈希或错误
    pub async fn upload_to_cloud(&self, filename: &str, data: &[u8]) -> FuncResult<String> {
        // 上传到本地云存储
        let file_hash = self.storage.cloud_storage.upload_file(filename, data).await
            .map_err(|e| ErrorInfo::new(7302, format!("上传到云存储失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        // 通知其他设备
        self.notify_cloud_upload(&file_hash, filename).await?;

        info!("文件上传到云存储成功: {} -> {}", filename, file_hash);
        Ok(file_hash)
    }

    /// 从云存储下载文件
    ///
    /// # 参数
    ///
    /// * `file_hash` - 文件哈希
    ///
    /// # 返回值
    ///
    /// 返回文件数据或错误
    pub async fn download_from_cloud(&self, file_hash: &str) -> FuncResult<Vec<u8>> {
        // 从本地云存储下载
        let data = self.storage.cloud_storage.download_file(file_hash).await
            .map_err(|e| ErrorInfo::new(7303, format!("从云存储下载失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        info!("从云存储下载文件成功: {} ({} 字节)", file_hash, data.len());
        Ok(data)
    }

    /// 发送文件到对等设备
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对等设备ID
    /// * `filename` - 文件名
    /// * `data` - 文件数据
    ///
    /// # 返回值
    ///
    /// 返回发送结果
    pub async fn send_file_to_peer(&self, peer_id: &str, filename: &str, data: &[u8]) -> FuncResult<()> {
        // 先存储到对象存储
        let object_id = format!("{}_{}", self.device_id, filename);
        self.storage.object_storage.store(&object_id, data).await
            .map_err(|e| ErrorInfo::new(7304, format!("存储对象失败: {}", e))
                .with_category(ErrorCategory::Storage))?;

        // 创建文件传输令牌
        let meta = TokenMeta::new(STORAGE_FILE_TRANSFER_TOKEN.to_string(), self.device_id.clone())
            .with_receiver(peer_id.to_string());

        let mut payload = Vec::new();
        payload.extend_from_slice(filename.as_bytes());
        payload.push(0); // 分隔符
        payload.extend_from_slice(data);

        let token = Token::new(meta, payload);

        // 发送令牌
        self.engine.send_token(token).await
            .map_err(|e| ErrorInfo::new(7305, format!("发送文件失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        info!("发送文件到对等设备: {} -> {} ({} 字节)", peer_id, filename, data.len());
        Ok(())
    }

    /// 发送大文件到对等设备
    ///
    /// # 参数
    ///
    /// * `peer_id` - 对等设备ID
    /// * `filename` - 文件名
    /// * `data` - 文件数据
    ///
    /// # 返回值
    ///
    /// 返回流ID或错误
    pub async fn send_large_file_to_peer(&self, peer_id: &str, filename: &str, data: &[u8]) -> FuncResult<String> {
        // 使用 bey-net 的大文件传输功能
        let stream_id = self.engine.send_large_file(peer_id, data.to_vec(), filename).await
            .map_err(|e| ErrorInfo::new(7306, format!("发送大文件失败: {}", e))
                .with_category(ErrorCategory::Network))?;

        info!("发送大文件到对等设备: {} -> {} ({} 字节)", peer_id, filename, data.len());
        Ok(stream_id)
    }

    /// 通知其他设备云存储更新
    async fn notify_cloud_upload(&self, file_hash: &str, filename: &str) -> FuncResult<()> {
        // 创建通知令牌
        let meta = TokenMeta::new(STORAGE_CLOUD_NOTIFY_TOKEN.to_string(), self.device_id.clone());

        let mut payload = Vec::new();
        payload.extend_from_slice(file_hash.as_bytes());
        payload.push(0); // 分隔符
        payload.extend_from_slice(filename.as_bytes());

        let token = Token::new(meta, payload);

        // 广播通知
        let _ = self.engine.broadcast(token.payload, STORAGE_CLOUD_NOTIFY_TOKEN).await;

        debug!("广播云存储更新通知: {}", file_hash);
        Ok(())
    }
}

/// 存储处理器
struct StorageHandler {
    device_id: String,
    storage: Arc<UnifiedStorageManager>,
}

#[async_trait]
impl TokenHandler for StorageHandler {
    fn token_types(&self) -> Vec<String> {
        vec![
            STORAGE_FILE_TRANSFER_TOKEN.to_string(),
            STORAGE_CLOUD_UPLOAD_TOKEN.to_string(),
            STORAGE_CLOUD_DOWNLOAD_TOKEN.to_string(),
            STORAGE_CLOUD_NOTIFY_TOKEN.to_string(),
        ]
    }

    async fn handle_token(&self, token: Token) -> NetResult<Option<Token>> {
        match token.meta.token_type.as_str() {
            STORAGE_FILE_TRANSFER_TOKEN => {
                self.handle_file_transfer(token).await?;
            }
            STORAGE_CLOUD_NOTIFY_TOKEN => {
                self.handle_cloud_notify(token).await?;
            }
            _ => {
                debug!("未知存储令牌类型: {}", token.meta.token_type);
            }
        }

        Ok(None)
    }
}

impl StorageHandler {
    /// 处理文件传输
    async fn handle_file_transfer(&self, token: Token) -> NetResult<()> {
        // 解析payload
        let payload = &token.payload;
        if let Some(sep_pos) = payload.iter().position(|&b| b == 0) {
            let filename = String::from_utf8_lossy(&payload[..sep_pos]).to_string();
            let file_data = &payload[sep_pos + 1..];

            // 存储到对象存储
            let object_id = format!("received_{}_{}", token.meta.sender_id, filename);
            let _ = self.storage.object_storage.store(&object_id, file_data).await;

            info!("收到文件: {} 来自 {} ({} 字节)", filename, token.meta.sender_id, file_data.len());
        }

        Ok(())
    }

    /// 处理云存储通知
    async fn handle_cloud_notify(&self, token: Token) -> NetResult<()> {
        // 解析payload
        let payload = &token.payload;
        if let Some(sep_pos) = payload.iter().position(|&b| b == 0) {
            let file_hash = String::from_utf8_lossy(&payload[..sep_pos]).to_string();
            let filename = String::from_utf8_lossy(&payload[sep_pos + 1..]).to_string();

            info!("云存储更新通知: {} ({}) 来自 {}", filename, file_hash, token.meta.sender_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_storage_func_creation() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let storage_path = temp_dir.path();

        let engine_config = bey_net::EngineConfig::default();
        let engine = bey_net::TransportEngine::new(engine_config).await.expect("创建引擎失败");
        
        let storage = bey_storage::UnifiedStorageManager::new(
            "test_device".to_string(),
            storage_path.to_path_buf(),
        ).await.expect("创建存储失败");

        let storage_func = StorageFunc::new(
            "test_device".to_string(),
            Arc::new(engine),
            Arc::new(storage),
        );

        assert_eq!(storage_func.device_id, "test_device");
    }
}
