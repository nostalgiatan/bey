//! # 对象存储模块
//!
//! 提供基本的对象存储功能，文件原样存储和传输，不进行分片或冗余处理。
//! 用于直接的文件传输场景。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, debug};

/// 对象存储结果类型
pub type ObjectStorageResult<T> = std::result::Result<T, ErrorInfo>;

/// 对象存储配置
#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    /// 存储根目录
    pub storage_root: PathBuf,
    /// 是否启用校验
    pub enable_checksum: bool,
}

impl Default for ObjectStorageConfig {
    fn default() -> Self {
        Self {
            storage_root: PathBuf::from("./object_storage"),
            enable_checksum: true,
        }
    }
}

/// 对象存储管理器
///
/// 负责文件的直接存储和检索，不进行分片或冗余
pub struct ObjectStorage {
    config: ObjectStorageConfig,
}

impl ObjectStorage {
    /// 创建新的对象存储实例
    ///
    /// # 参数
    ///
    /// * `config` - 存储配置
    ///
    /// # 返回值
    ///
    /// 返回对象存储实例或错误
    pub async fn new(config: ObjectStorageConfig) -> ObjectStorageResult<Self> {
        // 创建存储目录
        fs::create_dir_all(&config.storage_root).await
            .map_err(|e| ErrorInfo::new(6001, format!("创建存储目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;
        
        info!("对象存储初始化成功: {:?}", config.storage_root);
        Ok(Self { config })
    }

    /// 存储对象
    ///
    /// # 参数
    ///
    /// * `object_id` - 对象唯一标识符
    /// * `data` - 对象数据
    ///
    /// # 返回值
    ///
    /// 返回存储路径或错误
    pub async fn store(&self, object_id: &str, data: &[u8]) -> ObjectStorageResult<PathBuf> {
        let path = self.config.storage_root.join(object_id);
        
        // 创建父目录（如果需要）
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| ErrorInfo::new(6002, format!("创建父目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))?;
        }

        // 写入文件
        let mut file = fs::File::create(&path).await
            .map_err(|e| ErrorInfo::new(6003, format!("创建文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;
        
        file.write_all(data).await
            .map_err(|e| ErrorInfo::new(6004, format!("写入文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;
        
        file.sync_all().await
            .map_err(|e| ErrorInfo::new(6005, format!("同步文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        debug!("对象存储成功: {} ({} 字节)", object_id, data.len());
        Ok(path)
    }

    /// 检索对象
    ///
    /// # 参数
    ///
    /// * `object_id` - 对象唯一标识符
    ///
    /// # 返回值
    ///
    /// 返回对象数据或错误
    pub async fn retrieve(&self, object_id: &str) -> ObjectStorageResult<Vec<u8>> {
        let path = self.config.storage_root.join(object_id);
        
        if !path.exists() {
            return Err(ErrorInfo::new(6006, format!("对象不存在: {}", object_id))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Warning));
        }

        let mut file = fs::File::open(&path).await
            .map_err(|e| ErrorInfo::new(6007, format!("打开文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;
        
        let mut data = Vec::new();
        file.read_to_end(&mut data).await
            .map_err(|e| ErrorInfo::new(6008, format!("读取文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        debug!("对象检索成功: {} ({} 字节)", object_id, data.len());
        Ok(data)
    }

    /// 删除对象
    ///
    /// # 参数
    ///
    /// * `object_id` - 对象唯一标识符
    ///
    /// # 返回值
    ///
    /// 返回删除结果
    pub async fn delete(&self, object_id: &str) -> ObjectStorageResult<()> {
        let path = self.config.storage_root.join(object_id);
        
        if !path.exists() {
            return Err(ErrorInfo::new(6009, format!("对象不存在: {}", object_id))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Warning));
        }

        fs::remove_file(&path).await
            .map_err(|e| ErrorInfo::new(6010, format!("删除文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        debug!("对象删除成功: {}", object_id);
        Ok(())
    }

    /// 检查对象是否存在
    ///
    /// # 参数
    ///
    /// * `object_id` - 对象唯一标识符
    ///
    /// # 返回值
    ///
    /// 返回对象是否存在
    pub async fn exists(&self, object_id: &str) -> bool {
        self.config.storage_root.join(object_id).exists()
    }

    /// 列出所有对象
    ///
    /// # 返回值
    ///
    /// 返回对象ID列表
    pub async fn list(&self) -> ObjectStorageResult<Vec<String>> {
        let mut entries = fs::read_dir(&self.config.storage_root).await
            .map_err(|e| ErrorInfo::new(6011, format!("读取目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        let mut objects = Vec::new();
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| ErrorInfo::new(6012, format!("读取目录项失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))? {
            
            if let Ok(file_name) = entry.file_name().into_string() {
                objects.push(file_name);
            }
        }

        Ok(objects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_object_storage_store_and_retrieve() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config = ObjectStorageConfig {
            storage_root: temp_dir.path().to_path_buf(),
            enable_checksum: true,
        };

        let storage = ObjectStorage::new(config).await.expect("创建存储失败");
        let data = b"test data";
        
        // 存储
        storage.store("test_object", data).await.expect("存储失败");
        
        // 检索
        let retrieved = storage.retrieve("test_object").await.expect("检索失败");
        assert_eq!(data, retrieved.as_slice());
        
        // 检查存在
        assert!(storage.exists("test_object").await);
        
        // 删除
        storage.delete("test_object").await.expect("删除失败");
        assert!(!storage.exists("test_object").await);
    }

    #[tokio::test]
    async fn test_object_storage_list() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config = ObjectStorageConfig {
            storage_root: temp_dir.path().to_path_buf(),
            enable_checksum: true,
        };

        let storage = ObjectStorage::new(config).await.expect("创建存储失败");
        
        // 存储多个对象
        storage.store("obj1", b"data1").await.expect("存储失败");
        storage.store("obj2", b"data2").await.expect("存储失败");
        storage.store("obj3", b"data3").await.expect("存储失败");
        
        // 列出
        let objects = storage.list().await.expect("列出失败");
        assert_eq!(objects.len(), 3);
        assert!(objects.contains(&"obj1".to_string()));
        assert!(objects.contains(&"obj2".to_string()));
        assert!(objects.contains(&"obj3".to_string()));
    }
}
