//! # BEY 存储模块
//!
//! 提供完整的存储解决方案，包括：
//! - **对象存储**：文件原样存储和传输
//! - **云存储**：分布式存储，使用sled数据库和zstd压缩
//! - **剪切板同步**：跨设备剪切板数据同步
//! - **消息系统**：支持私信和群聊的消息系统
//!
//! ## 架构概览
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    BEY 存储系统                              │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌───────────────┐  ┌───────────────┐  ┌─────────────────┐ │
//! │  │ 对象存储       │  │ 云存储         │  │ 剪切板同步      │ │
//! │  │ (原样存储)     │  │ (分布式)       │  │ (差异同步)      │ │
//! │  └───────────────┘  └───────────────┘  └─────────────────┘ │
//! │  ┌───────────────────────────────────────────────────────┐ │
//! │  │ 消息系统 (群聊/私信)                                   │ │
//! │  └───────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//!         ↓                    ↓                    ↓
//! ┌──────────────────────────────────────────────────────────────┐
//! │              BEY 网络层 (bey-net)                             │
//! │  - 自动接收和路由消息                                          │
//! │  - 支持大文件传输                                             │
//! │  - 设备发现和管理                                             │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! use bey_storage::{UnifiedStorageManager, MessageType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 创建统一存储管理器
//! let storage = UnifiedStorageManager::new(
//!     "my_device_id".to_string(),
//!     std::path::PathBuf::from("./storage")
//! ).await?;
//!
//! // 使用对象存储
//! storage.object_storage.store("file1", b"data").await?;
//! let data = storage.object_storage.retrieve("file1").await?;
//!
//! // 使用云存储
//! let file_hash = storage.cloud_storage.upload_file("test.txt", b"content").await?;
//! let downloaded = storage.cloud_storage.download_file(&file_hash).await?;
//!
//! // 使用剪切板
//! let clip_id = storage.clipboard.add_entry(b"clipboard text".to_vec(), "text".to_string()).await?;
//!
//! // 使用消息系统
//! let msg_id = storage.message.send_message(
//!     MessageType::Private,
//!     "other_device".to_string(),
//!     b"Hello!".to_vec(),
//!     "text".to_string(),
//! ).await?;
//! # Ok(())
//! # }
//! ```

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};

/// 存储结果类型
pub type StorageResult<T> = std::result::Result<T, ErrorInfo>;

// 导出子模块
pub mod object_storage;
pub mod cloud_storage;
pub mod clipboard;
pub mod message;
pub mod compression;
pub mod key_management;

// 重新导出主要类型
pub use object_storage::{ObjectStorage, ObjectStorageConfig};
pub use cloud_storage::{CloudStorage, CloudStorageConfig, FileMetadata as CloudFileMetadata};
pub use clipboard::{ClipboardManager, ClipboardEntry, ClipboardEvent, SyncMode};
pub use message::{MessageManager, Message, MessageType, MessageEvent};
pub use compression::{SmartCompressor, CompressionStrategy, CompressionAlgorithm};
pub use key_management::SecureKeyManager;

/// 统一存储管理器
///
/// 整合所有存储功能的统一接口
pub struct UnifiedStorageManager {
    /// 对象存储
    pub object_storage: ObjectStorage,
    /// 云存储
    pub cloud_storage: CloudStorage,
    /// 剪切板管理器
    pub clipboard: ClipboardManager,
    /// 消息管理器
    pub message: MessageManager,
}

impl UnifiedStorageManager {
    /// 创建新的统一存储管理器
    ///
    /// # 参数
    ///
    /// * `device_id` - 本地设备ID
    /// * `storage_root` - 存储根目录
    ///
    /// # 返回值
    ///
    /// 返回管理器实例或错误
    pub async fn new(device_id: String, storage_root: std::path::PathBuf) -> StorageResult<Self> {
        // 创建存储根目录
        tokio::fs::create_dir_all(&storage_root).await
            .map_err(|e| ErrorInfo::new(6001, format!("创建存储根目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        // 初始化对象存储
        let object_config = ObjectStorageConfig {
            storage_root: storage_root.join("objects"),
            enable_checksum: true,
        };
        let object_storage = ObjectStorage::new(object_config).await?;

        // 初始化云存储
        let cloud_config = CloudStorageConfig {
            storage_root: storage_root.join("cloud"),
            db_path: storage_root.join("cloud_metadata.db"),
            ..Default::default()
        };
        let cloud_storage = CloudStorage::new(cloud_config).await?;

        // 初始化剪切板管理器
        let clipboard_path = storage_root.join("clipboard.db");
        let clipboard = ClipboardManager::new(device_id.clone(), clipboard_path).await
            .map_err(|e| ErrorInfo::new(6002, format!("创建剪切板管理器失败: {}", e))
                .with_category(ErrorCategory::System))?;

        // 初始化消息管理器
        let message_path = storage_root.join("messages.db");
        let message = MessageManager::new(device_id, message_path).await
            .map_err(|e| ErrorInfo::new(6003, format!("创建消息管理器失败: {}", e))
                .with_category(ErrorCategory::System))?;

        Ok(Self {
            object_storage,
            cloud_storage,
            clipboard,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_unified_storage_manager() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let storage_root = temp_dir.path().to_path_buf();
        
        let manager = UnifiedStorageManager::new("test_device".to_string(), storage_root).await
            .expect("创建管理器失败");

        // 测试对象存储
        manager.object_storage.store("test", b"data").await.expect("对象存储失败");
        let data = manager.object_storage.retrieve("test").await.expect("对象检索失败");
        assert_eq!(data, b"data");

        // 测试剪切板
        let clip_id = manager.clipboard.add_entry(b"clipboard".to_vec(), "text".to_string()).await
            .expect("剪切板添加失败");
        let entry = manager.clipboard.get_entry(&clip_id).await.expect("剪切板获取失败");
        assert_eq!(entry.content, b"clipboard");

        // 测试消息
        let msg_id = manager.message.send_message(
            MessageType::Private,
            "other_device".to_string(),
            b"hello".to_vec(),
            "text".to_string(),
        ).await.expect("消息发送失败");
        let msg = manager.message.get_message(&msg_id).await.expect("消息获取失败");
        assert_eq!(msg.content, b"hello");
    }

    #[tokio::test]
    async fn test_object_storage() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config = ObjectStorageConfig {
            storage_root: temp_dir.path().to_path_buf(),
            enable_checksum: true,
        };

        let storage = ObjectStorage::new(config).await.expect("创建存储失败");
        
        // 存储和检索
        storage.store("test_file", b"test data").await.expect("存储失败");
        let data = storage.retrieve("test_file").await.expect("检索失败");
        assert_eq!(data, b"test data");
        
        // 删除
        storage.delete("test_file").await.expect("删除失败");
        assert!(!storage.exists("test_file").await);
    }

    #[tokio::test]
    async fn test_cloud_storage() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config = CloudStorageConfig {
            storage_root: temp_dir.path().join("storage"),
            db_path: temp_dir.path().join("db"),
            chunk_size: 1024,
            ..Default::default()
        };

        let storage = CloudStorage::new(config).await.expect("创建云存储失败");
        
        let test_data = b"Hello, Cloud Storage!".repeat(100);
        
        // 上传和下载
        let file_hash = storage.upload_file("test.txt", &test_data).await.expect("上传失败");
        let downloaded = storage.download_file(&file_hash).await.expect("下载失败");
        assert_eq!(test_data, downloaded.as_slice());
    }
}
