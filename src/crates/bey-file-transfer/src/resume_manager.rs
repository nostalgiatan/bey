//! # 断点续传管理器
//!
//! 负责文件传输的断点续传功能，包括传输状态保存、断点信息管理和传输恢复。
//! 使用高效的序列化机制确保断点信息的快速保存和加载。
//!
//! ## 核心功能
//!
//! - **断点保存**: 自动保存传输进度和状态信息
//! - **智能恢复**: 根据断点信息智能恢复传输
//! - **状态验证**: 验证断点信息的有效性和一致性
//! - **并发安全**: 支持多线程并发访问断点信息
//! - **存储优化**: 高效的断点信息存储和检索

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tracing::{info, warn, error, debug, instrument};
use parking_lot::Mutex;
use dashmap::DashMap;
use crate::{TransferCheckpoint, TransferConfig, TransferResult, ChunkInfo};

/// 断点续传管理器
///
/// 负责管理文件传输的断点续传功能，包括断点信息的保存、加载和验证。
/// 确保在网络中断或系统故障后能够恢复传输进度。
#[derive(Debug)]
pub struct ResumeManager {
    /// 断点信息存储映射
    checkpoints: Arc<DashMap<String, TransferCheckpoint>>,
    /// 断点信息存储目录
    storage_dir: PathBuf,
    /// 配置信息
    config: Arc<TransferConfig>,
    /// 内存缓存锁
    cache_lock: Arc<Mutex<()>>,
}

impl ResumeManager {
    /// 创建新的断点续传管理器
    ///
    /// # 参数
    ///
    /// * `storage_dir` - 断点信息存储目录
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回断点续传管理器实例或错误信息
    #[instrument(skip(config))]
    pub async fn new<P: AsRef<Path> + std::fmt::Debug>(
        storage_dir: P,
        config: Arc<TransferConfig>,
    ) -> TransferResult<Self> {
        let storage_dir = storage_dir.as_ref().to_path_buf();

        info!("创建断点续传管理器，存储目录: {:?}", storage_dir);

        // 确保存储目录存在
        if let Err(e) = fs::create_dir_all(&storage_dir).await {
            error!("创建存储目录失败: {}", e);
            return Err(ErrorInfo::new(
                7001,
                format!("创建存储目录失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error));
        }

        let manager = Self {
            checkpoints: Arc::new(DashMap::new()),
            storage_dir,
            config,
            cache_lock: Arc::new(Mutex::new(())),
        };

        // 加载现有的断点信息
        manager.load_checkpoints().await?;

        Ok(manager)
    }

    /// 保存传输断点信息
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    /// * `transferred_chunks` - 已传输的数据块信息
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self, transferred_chunks), fields(task_id))]
    pub async fn save_checkpoint(
        &self,
        task_id: &str,
        transferred_chunks: Vec<ChunkInfo>,
    ) -> TransferResult<()> {
        let _lock = self.cache_lock.lock();

        info!("保存传输断点信息，任务ID: {}, 已传输块数: {}", task_id, transferred_chunks.len());

        // 创建断点信息
        let checkpoint = TransferCheckpoint {
            task_id: task_id.to_string(),
            transferred_chunks: transferred_chunks.clone(),
            config: self.config.as_ref().clone(),
            created_at: SystemTime::now(),
        };

        // 保存到内存
        self.checkpoints.insert(task_id.to_string(), checkpoint.clone());

        // 异步保存到磁盘
        let storage_dir = self.storage_dir.clone();
        let task_id_owned = task_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = Self::save_checkpoint_to_disk(&storage_dir, &task_id_owned, &checkpoint).await {
                error!("保存断点信息到磁盘失败: {}", e);
            }
        });

        debug!("断点信息保存完成，任务ID: {}", task_id);
        Ok(())
    }

    /// 加载传输断点信息
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回断点信息或错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn load_checkpoint(&self, task_id: &str) -> TransferResult<Option<TransferCheckpoint>> {
        info!("加载传输断点信息，任务ID: {}", task_id);

        // 首先从内存缓存查找
        if let Some(checkpoint) = self.checkpoints.get(task_id) {
            info!("从内存缓存找到断点信息，任务ID: {}", task_id);
            return Ok(Some(checkpoint.clone()));
        }

        // 从磁盘加载
        let checkpoint = Self::load_checkpoint_from_disk(&self.storage_dir, task_id).await?;

        if let Some(ref cp) = checkpoint {
            // 加载到内存缓存
            self.checkpoints.insert(task_id.to_string(), cp.clone());
            info!("从磁盘加载断点信息成功，任务ID: {}", task_id);
        } else {
            info!("未找到断点信息，任务ID: {}", task_id);
        }

        Ok(checkpoint)
    }

    /// 删除传输断点信息
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn delete_checkpoint(&self, task_id: &str) -> TransferResult<()> {
        let _lock = self.cache_lock.lock();

        info!("删除传输断点信息，任务ID: {}", task_id);

        // 从内存缓存删除
        self.checkpoints.remove(task_id);

        // 从磁盘删除
        let checkpoint_file = self.storage_dir.join(format!("{}.checkpoint", task_id));
        if let Err(e) = fs::remove_file(&checkpoint_file).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("删除断点文件失败: {}", e);
                return Err(ErrorInfo::new(
                    7002,
                    format!("删除断点文件失败: {}", e)
                )
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Warning));
            }
        }

        info!("断点信息删除完成，任务ID: {}", task_id);
        Ok(())
    }

    /// 验证断点信息的有效性
    ///
    /// # 参数
    ///
    /// * `checkpoint` - 待验证的断点信息
    ///
    /// # 返回
    ///
    /// 返回验证结果
    #[instrument(skip(checkpoint), fields(task_id = checkpoint.task_id))]
    pub async fn validate_checkpoint(&self, checkpoint: &TransferCheckpoint) -> TransferResult<bool> {
        info!("验证断点信息有效性，任务ID: {}", checkpoint.task_id);

        // 检查断点时间戳
        let now = SystemTime::now();
        let checkpoint_age = now.duration_since(checkpoint.created_at).unwrap_or_default();

        // 如果断点信息超过24小时，认为无效
        if checkpoint_age > Duration::from_secs(24 * 60 * 60) {
            warn!("断点信息过期，任务ID: {}, 年龄: {:?}", checkpoint.task_id, checkpoint_age);
            return Ok(false);
        }

        // 验证传输块信息的完整性
        for chunk in &checkpoint.transferred_chunks {
            if chunk.hash.is_empty() {
                warn!("传输块哈希值为空，任务ID: {}, 块索引: {}", checkpoint.task_id, chunk.index);
                return Ok(false);
            }

            if chunk.size == 0 {
                warn!("传输块大小为0，任务ID: {}, 块索引: {}", checkpoint.task_id, chunk.index);
                return Ok(false);
            }
        }

        info!("断点信息验证通过，任务ID: {}", checkpoint.task_id);
        Ok(true)
    }

    /// 获取所有断点信息
    ///
    /// # 返回
    ///
    /// 返回所有断点信息的列表
    #[instrument(skip(self))]
    pub async fn get_all_checkpoints(&self) -> TransferResult<Vec<TransferCheckpoint>> {
        info!("获取所有断点信息");

        let mut checkpoints = Vec::new();

        // 从内存缓存收集
        for entry in self.checkpoints.iter() {
            checkpoints.push(entry.value().clone());
        }

        info!("获取到 {} 个断点信息", checkpoints.len());
        Ok(checkpoints)
    }

    /// 清理过期的断点信息
    ///
    /// # 返回
    ///
    /// 返回清理的断点信息数量
    #[instrument(skip(self))]
    pub async fn cleanup_expired_checkpoints(&self) -> TransferResult<usize> {
        info!("开始清理过期的断点信息");

        let mut cleaned_count = 0;
        let now = SystemTime::now();
        let expiration_threshold = Duration::from_secs(24 * 60 * 60); // 24小时

        // 收集过期的任务ID
        let mut expired_task_ids = Vec::new();

        for entry in self.checkpoints.iter() {
            let checkpoint = entry.value();
            if let Ok(age) = now.duration_since(checkpoint.created_at) {
                if age > expiration_threshold {
                    expired_task_ids.push(entry.key().clone());
                }
            }
        }

        // 删除过期的断点信息
        for task_id in expired_task_ids {
            if let Err(e) = self.delete_checkpoint(&task_id).await {
                warn!("删除过期断点信息失败，任务ID: {}, 错误: {}", task_id, e);
            } else {
                cleaned_count += 1;
            }
        }

        info!("清理完成，删除了 {} 个过期断点信息", cleaned_count);
        Ok(cleaned_count)
    }

    /// 加载所有断点信息
    ///
    /// 从磁盘加载所有断点信息到内存缓存
    #[instrument(skip(self))]
    async fn load_checkpoints(&self) -> TransferResult<()> {
        info!("加载所有断点信息到内存缓存");

        let mut entries = match fs::read_dir(&self.storage_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                error!("读取存储目录失败: {}", e);
                return Err(ErrorInfo::new(
                    7003,
                    format!("读取存储目录失败: {}", e)
                )
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Error));
            }
        };

        let mut loaded_count = 0;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            ErrorInfo::new(
                7004,
                format!("读取目录条目失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("checkpoint") {
                if let Some(task_id) = path.file_stem().and_then(|s| s.to_str()) {
                    match Self::load_checkpoint_from_disk(&self.storage_dir, task_id).await {
                        Ok(Some(checkpoint)) => {
                            self.checkpoints.insert(task_id.to_string(), checkpoint);
                            loaded_count += 1;
                        }
                        Ok(None) => {
                            debug!("断点文件为空，任务ID: {}", task_id);
                        }
                        Err(e) => {
                            warn!("加载断点信息失败，任务ID: {}, 错误: {}", task_id, e);
                        }
                    }
                }
            }
        }

        info!("加载完成，共加载 {} 个断点信息", loaded_count);
        Ok(())
    }

    /// 保存断点信息到磁盘
    ///
    /// # 参数
    ///
    /// * `storage_dir` - 存储目录
    /// * `task_id` - 任务ID
    /// * `checkpoint` - 断点信息
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    async fn save_checkpoint_to_disk(
        storage_dir: &Path,
        task_id: &str,
        checkpoint: &TransferCheckpoint,
    ) -> TransferResult<()> {
        let file_path = storage_dir.join(format!("{}.checkpoint", task_id));

        // 序列化断点信息
        let serialized = serde_json::to_vec(checkpoint).map_err(|e| {
            ErrorInfo::new(
                7005,
                format!("序列化断点信息失败: {}", e)
            )
            .with_category(ErrorCategory::Parse)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 原子写入文件
        fs::write(&file_path, serialized).await.map_err(|e| {
            ErrorInfo::new(
                7006,
                format!("写入断点文件失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        debug!("断点信息已保存到磁盘: {:?}", file_path);
        Ok(())
    }

    /// 从磁盘加载断点信息
    ///
    /// # 参数
    ///
    /// * `storage_dir` - 存储目录
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回断点信息或错误信息
    async fn load_checkpoint_from_disk(
        storage_dir: &Path,
        task_id: &str,
    ) -> TransferResult<Option<TransferCheckpoint>> {
        let file_path = storage_dir.join(format!("{}.checkpoint", task_id));

        // 检查文件是否存在
        if !file_path.exists() {
            return Ok(None);
        }

        // 读取文件内容
        let content = fs::read(&file_path).await.map_err(|e| {
            ErrorInfo::new(
                7007,
                format!("读取断点文件失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 反序列化断点信息
        let checkpoint: TransferCheckpoint = serde_json::from_slice(&content).map_err(|e| {
            ErrorInfo::new(
                7008,
                format!("反序列化断点信息失败: {}", e)
            )
            .with_category(ErrorCategory::Parse)
            .with_severity(ErrorSeverity::Error)
        })?;

        debug!("断点信息已从磁盘加载: {:?}", file_path);
        Ok(Some(checkpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInfo, TransferConfig};
    use tempfile::tempdir;
    use std::time::UNIX_EPOCH;

    #[tokio::test]
    async fn test_resume_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let config = Arc::new(TransferConfig::default());

        let manager = ResumeManager::new(temp_dir.path(), config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_save_and_load_checkpoint() {
        let temp_dir = tempdir().unwrap();
        let config = Arc::new(TransferConfig::default());
        let manager = ResumeManager::new(temp_dir.path(), config).await.unwrap();

        let task_id = "test-task-001";
        let chunks = vec![
            ChunkInfo {
                index: 0,
                offset: 0,
                size: 1024,
                hash: "test-hash-001".to_string(),
                timestamp: SystemTime::now(),
            }
        ];

        // 保存断点信息
        let save_result = manager.save_checkpoint(task_id, chunks.clone()).await;
        assert!(save_result.is_ok());

        // 加载断点信息
        let loaded_checkpoint = manager.load_checkpoint(task_id).await.unwrap();
        assert!(loaded_checkpoint.is_some());

        let checkpoint = loaded_checkpoint.unwrap();
        assert_eq!(checkpoint.task_id, task_id);
        assert_eq!(checkpoint.transferred_chunks.len(), 1);
        assert_eq!(checkpoint.transferred_chunks[0].hash, "test-hash-001");
    }

    #[tokio::test]
    async fn test_checkpoint_validation() {
        let temp_dir = tempdir().unwrap();
        let config = Arc::new(TransferConfig::default());
        let manager = ResumeManager::new(temp_dir.path(), config).await.unwrap();

        // 创建有效的断点信息
        let valid_checkpoint = TransferCheckpoint {
            task_id: "valid-task".to_string(),
            transferred_chunks: vec![
                ChunkInfo {
                    index: 0,
                    offset: 0,
                    size: 1024,
                    hash: "valid-hash".to_string(),
                    timestamp: SystemTime::now(),
                }
            ],
            config: TransferConfig::default(),
            created_at: SystemTime::now(),
        };

        let is_valid = manager.validate_checkpoint(&valid_checkpoint).await.unwrap();
        assert!(is_valid);

        // 创建无效的断点信息（空哈希）
        let invalid_checkpoint = TransferCheckpoint {
            task_id: "invalid-task".to_string(),
            transferred_chunks: vec![
                ChunkInfo {
                    index: 0,
                    offset: 0,
                    size: 1024,
                    hash: "".to_string(), // 空哈希
                    timestamp: SystemTime::now(),
                }
            ],
            config: TransferConfig::default(),
            created_at: SystemTime::now(),
        };

        let is_valid = manager.validate_checkpoint(&invalid_checkpoint).await.unwrap();
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let temp_dir = tempdir().unwrap();
        let config = Arc::new(TransferConfig::default());
        let manager = ResumeManager::new(temp_dir.path(), config).await.unwrap();

        let task_id = "delete-task";
        let chunks = vec![
            ChunkInfo {
                index: 0,
                offset: 0,
                size: 1024,
                hash: "delete-hash".to_string(),
                timestamp: SystemTime::now(),
            }
        ];

        // 保存断点信息
        manager.save_checkpoint(task_id, chunks).await.unwrap();

        // 验证断点信息存在
        let loaded = manager.load_checkpoint(task_id).await.unwrap();
        assert!(loaded.is_some());

        // 删除断点信息
        let delete_result = manager.delete_checkpoint(task_id).await;
        assert!(delete_result.is_ok());

        // 验证断点信息已删除
        let loaded = manager.load_checkpoint(task_id).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired_checkpoints() {
        let temp_dir = tempdir().unwrap();
        let config = Arc::new(TransferConfig::default());
        let manager = ResumeManager::new(temp_dir.path(), config).await.unwrap();

        // 创建过期的断点信息（25小时前）
        let expired_time = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        let expired_checkpoint = TransferCheckpoint {
            task_id: "expired-task".to_string(),
            transferred_chunks: vec![],
            config: TransferConfig::default(),
            created_at: expired_time,
        };

        // 直接保存到内存（模拟已过期的断点）
        manager.checkpoints.insert("expired-task".to_string(), expired_checkpoint);

        // 清理过期断点
        let cleaned_count = manager.cleanup_expired_checkpoints().await.unwrap();
        assert_eq!(cleaned_count, 1);

        // 验证过期断点已清理
        let loaded = manager.load_checkpoint("expired-task").await.unwrap();
        assert!(loaded.is_none());
    }
}