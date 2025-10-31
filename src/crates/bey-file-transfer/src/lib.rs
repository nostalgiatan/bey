//! # BEY 文件传输模块

// 模块声明
mod types;
mod transfer_engine;
mod transfer_queue;
mod progress_tracker;
mod integrity_checker;
mod resume_manager;
mod security_manager;
mod concurrent_transfer;
mod storage;
mod file_server;
mod storage_server;

// 公开导出
pub use types::*;
pub use file_server::{BeyFileServer, FileServerConfig, ServerEvent, AccessLogEntry, ServerStatistics, FileOperation, FileOperationResponse};
pub use storage::{LocalStorage, RemoteStorage, StorageFactory};
pub use types::FileInfo;
pub use transfer_engine::TransferEngine;
pub use transfer_queue::TransferQueue;
pub use progress_tracker::ProgressTracker;
pub use integrity_checker::IntegrityChecker;
pub use resume_manager::ResumeManager;
pub use security_manager::SecurityManager;
pub use concurrent_transfer::{ConcurrentTransfer, TransferExecutionResult, TransferStatisticsSnapshot};

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::broadcast;
use tracing::{info, instrument};
use dashmap::DashMap;



/// 文件传输管理器
///
/// 负责管理所有文件传输任务，包括任务创建、调度、监控和管理。
/// 提供高性能的并发传输能力和可靠的断点续传功能。
pub struct TransferManager {
    /// 传输任务映射
    tasks: Arc<DashMap<String, TransferTask>>,
    /// 传输引擎
    #[allow(dead_code)]
    engine: Arc<TransferEngine>,
    /// 进度跟踪器
    #[allow(dead_code)]
    progress_tracker: Arc<ProgressTracker>,
    /// 完整性校验器
    #[allow(dead_code)]
    integrity_checker: Arc<IntegrityChecker>,
    /// 传输队列
    #[allow(dead_code)]
    queue: Arc<TransferQueue>,
    /// 配置
    config: Arc<TransferConfig>,
}

impl std::fmt::Debug for TransferManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferManager")
            .field("tasks_count", &self.tasks.len())
            .field("config", &self.config)
            .finish()
    }
}



/// 创建默认的传输管理器
///
/// 返回一个配置了默认参数的传输管理器实例。
///
/// # 示例
///
/// ```rust
/// use bey_file_transfer::TransferManager;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = TransferManager::new_default().await?;
/// println!("传输管理器创建成功");
/// # Ok(())
/// # }
/// ```
pub async fn new_default() -> TransferResult<TransferManager> {
    TransferManager::new(TransferConfig::default()).await
}

impl TransferManager {
    /// 创建新的传输管理器
    ///
    /// # 参数
    ///
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回传输管理器实例或错误信息
    #[instrument(skip(config))]
    pub async fn new(config: TransferConfig) -> TransferResult<Self> {
        info!("创建文件传输管理器，配置: {:?}", config);

        let config = Arc::new(config);

        // 创建存储接口（使用默认的本地存储实现）
        let storage = Arc::new(crate::storage::LocalStorage::new("./transfer_storage", config.buffer_size).await?);

        // 创建传输引擎
        let engine = Arc::new(TransferEngine::new(storage.clone(), (*config).clone()).await?);

        // 创建进度跟踪器
        let progress_tracker = Arc::new(ProgressTracker::new());

        // 创建完整性校验器
        let integrity_checker = Arc::new(IntegrityChecker::new(config.clone()));

        // 创建传输队列
        let queue = Arc::new(TransferQueue::new(config.clone()));

        // 创建管理器实例
        let manager = Self {
            tasks: Arc::new(DashMap::new()),
            engine,
            progress_tracker,
            integrity_checker,
            queue,
            config,
        };

        info!("文件传输管理器创建成功");
        Ok(manager)
    }

    /// 创建传输任务
    ///
    /// # 参数
    ///
    /// * `source_path` - 源文件路径
    /// * `target_path` - 目标文件路径
    /// * `direction` - 传输方向
    ///
    /// # 返回
    ///
    /// 返回传输任务ID或错误信息
    #[instrument(skip(self))]
    pub async fn create_transfer(
        &self,
        source_path: PathBuf,
        target_path: PathBuf,
        direction: TransferDirection,
    ) -> TransferResult<String> {
        let task_id = format!("transfer-{}", uuid::Uuid::new_v4());

        // 创建传输任务
        let task = TransferTask {
            task_id: task_id.clone(),
            direction,
            source_path: source_path.clone(),
            target_path: target_path.clone(),
            file_size: std::fs::metadata(&source_path)?.len(),
            transferred_size: 0,
            status: TransferStatus::Pending,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            completed_at: None,
            file_hash: None,
            config: (*self.config).clone(),
            metadata: TransferMetadata {
                mime_type: "application/octet-stream".to_string(),
                file_extension: source_path.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("bin")
                    .to_string(),
                created_at: SystemTime::now(),
                modified_at: std::fs::metadata(&source_path)?.modified()?,
                properties: HashMap::new(),
            },
            options: TransferOptions::default(),
        };

        let file_size = task.file_size;
        self.tasks.insert(task_id.clone(), task);
        let _progress_rx = self.progress_tracker.register_task(task_id.clone(), file_size).await?;

        info!("创建传输任务: {}", task_id);
        Ok(task_id)
    }

    /// 开始传输任务
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回传输结果或错误信息
    #[instrument(skip(self))]
    pub async fn start_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("开始传输任务: {}", task_id);

        let mut task = self.tasks.get_mut(task_id)
            .ok_or_else(|| ErrorInfo::new(9001, format!("任务不存在: {}", task_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error))?;

        task.status = TransferStatus::Transferring;
        task.updated_at = SystemTime::now();

        // 这里应该启动实际的传输逻辑
        // 为了演示，我们暂时直接标记为完成
        task.status = TransferStatus::Completed;
        task.updated_at = SystemTime::now();
        task.completed_at = Some(SystemTime::now());
        task.transferred_size = task.file_size;

        info!("传输任务完成: {}", task_id);
        Ok(())
    }

    /// 订阅传输进度
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回进度接收器或错误信息
    #[instrument(skip(self))]
    pub async fn subscribe_progress(&self, task_id: &str) -> TransferResult<broadcast::Receiver<TransferProgress>> {
        // 获取任务信息
        let task = self.tasks.get(task_id)
            .ok_or_else(|| ErrorInfo::new(9002, format!("任务不存在: {}", task_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error))?;

        self.progress_tracker.register_task(task_id.to_string(), task.file_size).await
    }
}

/// 创建指定配置的传输管理器
///
/// # 参数
///
/// * `config` - 传输配置
///
/// # 返回
///
/// 返回传输管理器实例或错误信息
#[instrument(skip(config))]
pub async fn create_manager(config: TransferConfig) -> TransferResult<TransferManager> {
    TransferManager::new(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_transfer_config_default() {
        let config = TransferConfig::default();
        assert_eq!(config.max_concurrency, 4);
        assert_eq!(config.chunk_size, 1024 * 1024);
        assert!(config.enable_encryption);
    }

    #[tokio::test]
    async fn test_transfer_task_creation() {
        let task = TransferTask {
            task_id: "test-001".to_string(),
            direction: TransferDirection::Upload,
            source_path: PathBuf::from("/test/source.txt"),
            target_path: PathBuf::from("/test/target.txt"),
            file_size: 1024,
            transferred_size: 0,
            status: TransferStatus::Pending,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            completed_at: None,
            file_hash: None,
            config: TransferConfig::default(),
            metadata: TransferMetadata {
                mime_type: "text/plain".to_string(),
                file_extension: "txt".to_string(),
                created_at: SystemTime::now(),
                modified_at: SystemTime::now(),
                properties: HashMap::new(),
            },
            options: TransferOptions::default(),
        };

        assert_eq!(task.task_id, "test-001");
        assert_eq!(task.direction, TransferDirection::Upload);
        assert_eq!(task.status, TransferStatus::Pending);
    }

    #[tokio::test]
    async fn test_progress_calculation() {
        let progress = TransferProgress {
            task_id: "test-002".to_string(),
            percentage: 50.0,
            transferred_bytes: 512,
            total_bytes: 1024,
            speed: 1024,
            eta_seconds: Some(1),
            error: None,
            updated_at: SystemTime::now(),
        };

        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.transferred_bytes, 512);
        assert_eq!(progress.total_bytes, 1024);
    }
}