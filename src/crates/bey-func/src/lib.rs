//! # BEY 分布式功能模块
//!
//! 提供分布式服务的高级API，集成网络传输、存储、消息和剪切板功能。
//! 基于 Token 元类和接收器模块，实现以下功能：
//!
//! - **消息发送** - 私信、群聊、广播
//! - **剪切板同步** - 添加、删除、差异同步
//! - **云存储** - 文件上传、下载、分发
//! - **对象传输** - 点对点文件传输
//!
//! ## 架构设计
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                    BEY 分布式功能层                       │
//! ├──────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
//! │  │ 消息功能      │  │ 剪切板功能    │  │ 存储功能      │  │
//! │  │ MessageFunc  │  │ ClipboardFunc│  │ StorageFunc  │  │
//! │  └──────────────┘  └──────────────┘  └──────────────┘  │
//! │          ↓                 ↓                 ↓           │
//! │  ┌────────────────────────────────────────────────────┐ │
//! │  │         BeyFuncManager (统一管理器)                 │ │
//! │  └────────────────────────────────────────────────────┘ │
//! └──────────────────────────────────────────────────────────┘
//!                          ↓
//! ┌──────────────────────────────────────────────────────────┐
//! │              BEY 网络层 (bey-net)                         │
//! │  - Token 接收器和路由                                     │
//! │  - TransportEngine (发送/接收)                           │
//! └──────────────────────────────────────────────────────────┘
//!                          ↓
//! ┌──────────────────────────────────────────────────────────┐
//! │              BEY 存储层 (bey-storage)                     │
//! │  - 对象存储、云存储                                        │
//! │  - 剪切板、消息持久化                                      │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use bey_func::BeyFuncManager;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建分布式功能管理器
//! let manager = BeyFuncManager::new("my_device", "./storage").await?;
//!
//! // 启动网络服务
//! manager.start().await?;
//!
//! // 发送消息
//! manager.send_private_message("peer_device", b"Hello!").await?;
//! manager.send_group_message("group1", b"Hi everyone!").await?;
//!
//! // 同步剪切板
//! manager.add_clipboard("text", b"clipboard content").await?;
//! manager.sync_clipboard_to_group("group1").await?;
//!
//! // 云存储操作
//! let file_hash = manager.upload_to_cloud("doc.txt", &data).await?;
//! let data = manager.download_from_cloud(&file_hash).await?;
//!
//! // 点对点文件传输
//! manager.send_file_to_peer("peer_device", "file.txt", &data).await?;
//! # Ok(())
//! # }
//! ```

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::path::PathBuf;
use std::sync::Arc;

// 导出子模块
pub mod message_func;
pub mod clipboard_func;
pub mod storage_func;

// 重新导出主要类型
pub use message_func::MessageFunc;
pub use clipboard_func::ClipboardFunc;
pub use storage_func::StorageFunc;

/// 分布式功能结果类型
pub type FuncResult<T> = std::result::Result<T, ErrorInfo>;

/// BEY 分布式功能管理器
///
/// 统一管理所有分布式功能，提供高级API
pub struct BeyFuncManager {
    /// 设备ID
    device_id: String,
    /// 网络引擎
    engine: Arc<bey_net::TransportEngine>,
    /// 消息功能
    pub message: MessageFunc,
    /// 剪切板功能
    pub clipboard: ClipboardFunc,
    /// 存储功能
    pub storage_func: StorageFunc,
}

impl BeyFuncManager {
    /// 使用现有网络引擎创建分布式功能管理器
    ///
    /// # 参数
    ///
    /// * `device_id` - 设备唯一标识
    /// * `engine` - 网络传输引擎实例
    /// * `storage_root` - 存储根目录
    ///
    /// # 返回值
    ///
    /// 返回管理器实例或错误
    pub async fn new_with_engine(
        device_id: &str,
        engine: Arc<bey_net::TransportEngine>,
        storage_root: &str,
    ) -> FuncResult<Self> {
        // 初始化存储管理器
        let storage = bey_storage::UnifiedStorageManager::new(
            device_id.to_string(),
            PathBuf::from(storage_root),
        ).await
            .map_err(|e| ErrorInfo::new(7002, format!("创建存储管理器失败: {}", e))
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Error))?;

        let storage = Arc::new(storage);

        // 创建功能模块
        let message = MessageFunc::new(
            device_id.to_string(),
            Arc::clone(&engine),
            Arc::clone(&storage),
        );

        let clipboard = ClipboardFunc::new(
            device_id.to_string(),
            Arc::clone(&engine),
            Arc::clone(&storage),
        );

        let storage_func = StorageFunc::new(
            device_id.to_string(),
            Arc::clone(&engine),
            Arc::clone(&storage),
        );

        Ok(Self {
            device_id: device_id.to_string(),
            engine,
            message,
            clipboard,
            storage_func,
        })
    }

    /// 创建新的分布式功能管理器（包含独立的网络引擎）
    ///
    /// # 参数
    ///
    /// * `device_id` - 设备唯一标识
    /// * `storage_root` - 存储根目录
    ///
    /// # 返回值
    ///
    /// 返回管理器实例或错误
    ///
    /// # 注意
    ///
    /// 此方法会创建独立的网络引擎实例。如果在同一进程中需要多个管理器，
    /// 建议使用 `new_with_engine()` 方法共享同一个引擎实例。
    pub async fn new(device_id: &str, storage_root: &str) -> FuncResult<Self> {
        // 初始化网络引擎
        let mut engine_config = bey_net::EngineConfig {
            name: device_id.to_string(),
            ..Default::default()
        };
        engine_config.enable_encryption = true;
        engine_config.enable_auth = false;  // 禁用引擎层认证（传输层已处理）
        let engine = bey_net::TransportEngine::new(engine_config).await
            .map_err(|e| ErrorInfo::new(7001, format!("创建网络引擎失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        Self::new_with_engine(device_id, Arc::new(engine), storage_root).await
    }

    /// 仅注册消息处理器（不启动网络服务器）
    ///
    /// 当网络服务器由外部管理时使用此方法
    ///
    /// # 返回值
    ///
    /// 返回注册结果
    pub async fn register_handlers_only(&self) -> FuncResult<()> {
        // 注册消息处理器
        self.message.register_handlers(&self.engine).await?;
        self.clipboard.register_handlers(&self.engine).await?;
        self.storage_func.register_handlers(&self.engine).await?;

        tracing::info!("BEY 分布式功能管理器处理器已注册: {}", self.device_id);
        Ok(())
    }

    /// 启动网络服务
    ///
    /// 启动网络引擎，开始接收和处理消息
    ///
    /// # 返回值
    ///
    /// 返回启动结果
    pub async fn start(&self) -> FuncResult<()> {
        // 注册消息处理器
        self.message.register_handlers(&self.engine).await?;
        self.clipboard.register_handlers(&self.engine).await?;
        self.storage_func.register_handlers(&self.engine).await?;

        // 启动网络服务器
        self.engine.start_server().await
            .map_err(|e| ErrorInfo::new(7003, format!("启动网络服务失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        tracing::info!("BEY 分布式功能管理器已启动: {}", self.device_id);
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
        self.message.send_private_message(peer_id, content).await
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
        self.message.send_group_message(group_id, content).await
    }

    /// 广播消息
    ///
    /// # 参数
    ///
    /// * `content` - 消息内容
    ///
    /// # 返回值
    ///
    /// 返回发送结果
    pub async fn broadcast_message(&self, content: &[u8]) -> FuncResult<usize> {
        self.message.broadcast_message(content).await
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
        self.clipboard.add_clipboard(content_type, content).await
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
    pub async fn sync_clipboard_to_group(&self, group_id: &str) -> FuncResult<()> {
        self.clipboard.sync_to_group(group_id).await
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
    pub async fn sync_clipboard_to_peer(&self, peer_id: &str) -> FuncResult<()> {
        self.clipboard.sync_to_peer(peer_id).await
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
        self.storage_func.upload_to_cloud(filename, data).await
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
        self.storage_func.download_from_cloud(file_hash).await
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
        self.storage_func.send_file_to_peer(peer_id, filename, data).await
    }

    /// 获取设备ID
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// 获取网络引擎
    pub fn engine(&self) -> &bey_net::TransportEngine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_bey_func_manager_creation() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let storage_path = temp_dir.path().to_str().expect("路径转换失败");

        let manager = BeyFuncManager::new("test_device", storage_path).await;
        assert!(manager.is_ok());
    }
}
