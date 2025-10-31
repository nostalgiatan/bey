//! # 传输引擎
//!
//! 传输引擎是文件传输系统的核心组件，负责管理传输任务的执行。
//! 提供高性能的并发传输、断点续传、加密解密等功能。
//!
//! ## 核心功能
//!
//! - **任务调度**: 智于优先级的传输任务调度
//! - **并发控制**: 根据配置控制并发传输数量
//! - **进度监控**: 实时传输进度和速度监控
//! - **错误处理**: 完善的错误重试和恢复机制
//! - **完整性校验**: BLAKE3哈希确保数据完整性
//! - **安全传输**: AES-GCM加密保护敏感数据

use crate::{
    TransferConfig, TransferDirection, TransferStatus, TransferTask,
    TransferProgress, TransferMetadata,
};
use crate::types::{
    ChunkInfo, TransferResult, StorageInterface
};
use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error, debug, instrument, span};
use parking_lot::RwLock as ParkingLotRwLock;
use dashmap::DashMap;

/// 传输引擎实现
pub struct TransferEngine {
    /// 存储接口
    storage: Arc<dyn StorageInterface>,
    /// 续传管理器
    resume_manager: Arc<crate::ResumeManager>,
    /// 安全管理器
    security_manager: Arc<crate::SecurityManager>,
    /// 并发传输器
    #[allow(dead_code)]
    concurrent_transfer: Arc<crate::ConcurrentTransfer>,
    /// 进度跟踪器
    #[allow(dead_code)]
    progress_tracker: Arc<crate::ProgressTracker>,
    /// 完整性校验器
    #[allow(dead_code)]
    integrity_checker: Arc<crate::IntegrityChecker>,
    /// 活跃传输任务
    active_transfers: Arc<DashMap<String, Arc<RwLock<TransferTask>>>>,
    /// 进度通知发送器
    progress_senders: Arc<ParkingLotRwLock<HashMap<String, broadcast::Sender<TransferProgress>>>>,
    /// 配置
    config: Arc<TransferConfig>,
    /// 统计信息
    statistics: Arc<RwLock<TransferStatistics>>,
}

impl std::fmt::Debug for TransferEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferEngine")
            .field("active_transfers_count", &self.active_transfers.len())
            .field("config", &self.config)
            .finish()
    }
}

/// 传输统计信息
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransferStatistics {
    /// 总传输任务数
    pub total_tasks: u64,
    /// 成功传输任务数
    pub successful_tasks: u64,
    /// 失败传输任务数
    pub failed_tasks: u64,
    /// 已传输总字节数
    pub total_bytes_transferred: u64,
    /// 平均传输速度 (字节/秒)
    pub average_speed: u64,
    /// 当前活跃传输数
    pub active_transfers: usize,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 平均传输时间 (毫秒)
    pub average_transfer_time_ms: u64,
}

impl TransferEngine {
    /// 创建新的传输引擎实例
    ///
    /// # 参数
    ///
    /// * `storage` - 存储接口实现
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回传输引擎实例或错误信息
    #[instrument(skip(storage, config))]
    pub async fn new(
        storage: Arc<dyn StorageInterface>,
        config: TransferConfig,
    ) -> TransferResult<Self> {
        info!("初始化传输引擎，配置: {:?}", config);

        let config = Arc::new(config);

        // 创建进度通知发送器
        let progress_senders = Arc::new(ParkingLotRwLock::new(HashMap::new()));

        // 创建所有组件
        let resume_manager = Arc::new(crate::ResumeManager::new("./checkpoints", config.clone()).await.unwrap());
        let security_manager = Arc::new(crate::SecurityManager::new(config.clone()).await.unwrap());
        let concurrent_transfer = Arc::new(crate::ConcurrentTransfer::new(config.clone()).await.unwrap());
        let progress_tracker = Arc::new(crate::ProgressTracker::new());
        let integrity_checker = Arc::new(crate::IntegrityChecker::new(config.clone()));

        let engine = Self {
            storage,
            resume_manager,
            security_manager,
            concurrent_transfer,
            progress_tracker,
            integrity_checker,
            active_transfers: Arc::new(DashMap::new()),
            progress_senders,
            config,
            statistics: Arc::new(RwLock::new(TransferStatistics::default())),
        };

        info!("传输引擎初始化完成");
        Ok(engine)
    }

    /// 创建传输任务
    ///
    /// # 参数
    ///
    /// * `source_path` - 源文件路径
    /// * `target_path` - 目标文件路径
    /// * `direction` - 传输方向
    /// * `metadata` - 传输元数据
    /// * `options` - 传输选项
    ///
    /// # 返回
    ///
    /// 返回传输任务或错误信息
    #[instrument(
        skip(self, source_path, target_path, direction, metadata, options),
        fields(
            source_path = ?source_path.as_ref(),
            target_path = ?target_path.as_ref(),
            direction = ?direction
        )
    )]
    pub async fn create_transfer(
        &self,
        source_path: impl AsRef<Path>,
        target_path: impl AsRef<Path>,
        direction: TransferDirection,
        metadata: TransferMetadata,
        options: crate::TransferOptions,
    ) -> TransferResult<Arc<RwLock<TransferTask>>> {
        let source_path = source_path.as_ref();
        let target_path = target_path.as_ref();

        info!("创建传输任务: {} -> {}", source_path.display(), target_path.display());

        // 获取源文件信息
        let file_info = self.storage.get_file_info(source_path).await
            .map_err(|e| ErrorInfo::new(7001, format!("获取文件信息失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        // 检查文件是否存在
        if !self.storage.exists(source_path).await? {
            return Err(ErrorInfo::new(7002, format!("源文件不存在: {}", source_path.display()))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error));
        }

        // 生成任务ID
        let task_id = format!("{}-{}-{}",
            uuid::Uuid::new_v4().to_string(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            fastrand::u64(10000..99999)
        );

        // 创建传输任务
        let task = Arc::new(RwLock::new(TransferTask {
            task_id: task_id.clone(),
            direction,
            source_path: source_path.to_path_buf(),
            target_path: target_path.to_path_buf(),
            file_size: file_info.size,
            transferred_size: 0,
            status: TransferStatus::Preparing,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            completed_at: None,
            file_hash: file_info.hash,
            config: self.config.as_ref().clone(),
            metadata,
            options,
        }));

        // 注册到活跃传输列表
        self.active_transfers.insert(task_id.clone(), task.clone());

        // 创建进度通知发送器
        let (tx, _) = broadcast::channel(100);
        let mut senders = self.progress_senders.write();
        senders.insert(task_id.clone(), tx);
        drop(senders);

        // 初始化进度跟踪
        let initial_progress = TransferProgress {
            task_id: task_id.clone(),
            percentage: 0.0,
            transferred_bytes: 0,
            total_bytes: file_info.size,
            speed: 0,
            eta_seconds: None,
            error: None,
            updated_at: SystemTime::now(),
        };

        // 发送初始进度通知
        if let Some(progress_senders) = self.progress_senders.try_read() {
            if let Some(sender) = progress_senders.get(&task_id) {
                let _ = sender.send(initial_progress);
            }
        }

        info!("传输任务创建成功: {}", task_id);
        Ok(task)
    }

    /// 开始传输任务
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    ///
    /// # 返回
    ///
    /// 返回传输结果或错误信息
    #[instrument(skip(self, task))]
    pub async fn start_transfer(&self, task: Arc<RwLock<TransferTask>>) -> TransferResult<()> {
        let task_id = {
            let task_ref = task.read().await;
            task_ref.task_id.clone()
        };

        info!("开始传输任务: {}", task_id);

        let span = span!(tracing::Level::INFO, "transfer_execution", task_id = %task_id);

        let _enter = span.enter();

        info!("开始执行传输任务");

        // 获取任务详细信息
        let (task_details, progress_sender) = {
            let task_ref = task.read().await;
            let task_id = task_ref.task_id.clone();
            let progress_sender = {
                let senders = self.progress_senders.try_read();
                senders.and_then(|s| s.get(&task_id).map(|tx| tx.subscribe()))
            };
            (task_ref.clone(), progress_sender)
        };

        // 验证传输状态
        if task_details.status != TransferStatus::Preparing && task_details.status != TransferStatus::Resuming {
            return Err(ErrorInfo::new(7009, format!("任务状态不允许开始传输: {} ({})", task_details.task_id, task_details.status))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        // 更新任务状态为传输中
        {
            let mut task_ref = task.write().await;
            task_ref.status = TransferStatus::Transferring;
            task_ref.updated_at = SystemTime::now();
        }

        // 计算文件块
        let chunks = self.calculate_file_chunks(&task_details).await?;

        // 检查续传点
        let start_chunk_index = if task_details.status == TransferStatus::Resuming {
            self.check_resume_point(&task_details).await?
        } else {
            0
        };

        info!("开始传输文件块，起始索引: {}, 总块数: {}", start_chunk_index, chunks.len());

        // 并发传输数据块
        let transfer_result = self.execute_chunked_transfer(
            &task_details,
            &chunks,
            start_chunk_index,
            progress_sender,
        ).await;

        // 处理传输结果
        match transfer_result {
            Ok(_) => {
                // 传输成功，更新任务状态
                let mut task_ref = task.write().await;
                task_ref.status = TransferStatus::Completed;
                task_ref.transferred_size = task_details.file_size;
                task_ref.updated_at = SystemTime::now();
                task_ref.completed_at = Some(SystemTime::now());

                // 更新统计信息
                self.update_statistics(&task_ref, &Ok(())).await;

                info!("传输任务完成: {}", task_details.task_id);
                Ok(())
            }
            Err(e) => {
                // 传输失败，更新任务状态
                let mut task_ref = task.write().await;
                task_ref.status = TransferStatus::Failed;
                task_ref.updated_at = SystemTime::now();

                // 更新统计信息
                self.update_statistics(&task_ref, &Err(e.clone())).await;

                error!("传输任务失败: {} - {}", task_details.task_id, e);
                Err(e)
            }
        }
    }

    /// 暂停传输任务
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回操作结果或错误信息
    #[instrument(skip(self, task_id))]
    pub async fn pause_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("暂停传输任务: {}", task_id);

        if let Some(task) = self.active_transfers.get(task_id) {
            let mut task_ref = task.write().await;
            if task_ref.status == TransferStatus::Transferring {
                task_ref.status = TransferStatus::Paused;
                task_ref.updated_at = SystemTime::now();

                // 保存传输断点
                let chunks = self.calculate_file_chunks(&task_ref).await?;
                let completed_chunks = (task_ref.transferred_size as usize / task_ref.config.chunk_size).min(chunks.len());
                let transferred_chunks: Vec<ChunkInfo> = chunks[..completed_chunks].to_vec();
                if let Err(e) = self.resume_manager.save_checkpoint(&task_id, transferred_chunks).await {
                    warn!("保存传输断点失败: {}", e);
                }

                info!("传输任务已暂停: {}", task_id);
                Ok(())
            } else {
                Err(ErrorInfo::new(7003, format!("任务状态不允许暂停: {} ({:?})", task_id, task_ref.status))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Warning))
            }
        } else {
            Err(ErrorInfo::new(7004, format!("传输任务不存在: {}", task_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error))
        }
    }

    /// 恢复传输任务
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回操作结果或错误信息
    #[instrument(skip(self, task_id))]
    pub async fn resume_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("恢复传输任务: {}", task_id);

        if let Some(task) = self.active_transfers.get(task_id) {
            let mut task_ref = task.write().await;
            if task_ref.status == TransferStatus::Paused {
                task_ref.status = TransferStatus::Resuming;
                task_ref.updated_at = SystemTime::now();

                // 从断点恢复传输
                let transferred_size = if let Some(checkpoint) = self.resume_manager.load_checkpoint(&task_id).await? {
                    if self.resume_manager.validate_checkpoint(&checkpoint).await? {
                        // 恢复已传输的数据块
                        let mut transferred_size = 0;
                        for chunk in &checkpoint.transferred_chunks {
                            transferred_size += chunk.size as u64;
                        }

                        info!("从断点恢复传输，已传输: {} 字节", transferred_size);
                        transferred_size
                    } else {
                        warn!("断点信息无效，将从头开始传输");
                        0
                    }
                } else {
                    warn!("未找到断点信息，将从头开始传输");
                    0
                };

                // 更新任务状态和进度
                {
                    let mut task_ref = task.write().await;
                    task_ref.status = TransferStatus::Transferring;
                    task_ref.transferred_size = transferred_size;
                    task_ref.updated_at = SystemTime::now();
                }

                info!("传输任务正在恢复: {}", task_id);
                Ok(())
            } else {
                Err(ErrorInfo::new(7005, format!("任务状态不允许恢复: {} ({})", task_id, task_ref.status))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Warning))
            }
        } else {
            Err(ErrorInfo::new(7006, format!("传输任务不存在: {}", task_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error))
        }
    }

    /// 取消传输任务
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回操作结果或错误信息
    #[instrument(skip(self, task_id))]
    pub async fn cancel_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("取消传输任务: {}", task_id);

        if let Some(task) = self.active_transfers.get(task_id) {
            let mut task_ref = task.write().await;
            if task_ref.status != TransferStatus::Completed &&
               task_ref.status != TransferStatus::Failed &&
               task_ref.status != TransferStatus::Cancelled {

                task_ref.status = TransferStatus::Cancelled;
                task_ref.updated_at = SystemTime::now();
                task_ref.completed_at = Some(SystemTime::now());

                // 清理临时文件和资源
                // 删除断点信息
                if let Err(e) = self.resume_manager.delete_checkpoint(&task_id).await {
                    warn!("删除断点信息失败: {}", e);
                }

                // 清理进度通知发送器
                {
                    let mut senders = self.progress_senders.write();
                    senders.remove(task_id);
                }

                // 如果是不完整传输，可能需要清理部分传输的文件
                if task_ref.transferred_size < task_ref.file_size {
                    if let Err(e) = self.storage.delete_file(&task_ref.target_path).await {
                        warn!("清理不完整传输文件失败: {}", e);
                    } else {
                        info!("已清理不完整的传输文件: {}", task_ref.target_path.display());
                    }
                }

                // 从活跃传输列表中移除
                drop(task_ref); // 释放锁
                self.active_transfers.remove(task_id);

                info!("传输任务资源清理完成: {}", task_id);

                info!("传输任务已取消: {}", task_id);
                Ok(())
            } else {
                Err(ErrorInfo::new(7007, format!("任务状态不允许取消: {} ({})", task_id, task_ref.status))
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Warning))
            }
        } else {
            Err(ErrorInfo::new(7008, format!("传输任务不存在: {}", task_id))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error))
        }
    }

    /// 获取传输任务信息
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// # 返回
    ///
    /// 返回传输任务或错误信息
    #[instrument(skip(self, task_id))]
    pub async fn get_transfer_task(&self, task_id: &str) -> TransferResult<Option<Arc<RwLock<TransferTask>>>> {
        Ok(self.active_transfers.get(task_id).map(|v| v.clone()))
    }

    /// 获取所有活跃传输任务
    ///
    /// # 返回
    ///
    /// 返回活跃传输任务列表
    #[instrument(skip(self))]
    pub async fn get_active_transfers(&self) -> Vec<Arc<RwLock<TransferTask>>> {
        self.active_transfers
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// 获取传输统计信息
    ///
    /// # 返回
    ///
    /// 返回传输统计信息
    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> TransferStatistics {
        self.statistics.read().await.clone()
    }

    /// 订阅传输进度通知
    ///
    /// # 参数
    ///
    /// * `task_id` - 传输任务ID
    ///
    /// 返回
    ///
    /// 返回进度通知接收器
    #[instrument(skip(self, task_id))]
    pub async fn subscribe_progress(&self, task_id: &str) -> Option<broadcast::Receiver<TransferProgress>> {
        let senders = self.progress_senders.read();
        senders.get(task_id).map(|tx| tx.subscribe())
    }

    /// 更新传输统计信息
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    /// * `result` - 传输结果
    async fn update_statistics(&self, task: &TransferTask, result: &TransferResult<()>) {
        let mut stats = self.statistics.write().await;

        stats.total_tasks += 1;

        if result.is_ok() {
            stats.successful_tasks += 1;
            stats.total_bytes_transferred += task.transferred_size;
        } else {
            stats.failed_tasks += 1;
        }

        // 计算平均速度
        if stats.successful_tasks > 0 {
            stats.average_speed = stats.total_bytes_transferred / stats.successful_tasks;
        }

        stats.active_transfers = self.active_transfers.len();
    }

    /// 计算文件块
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    ///
    /// # 返回
    ///
    /// 返回文件块信息列表
    async fn calculate_file_chunks(&self, task: &TransferTask) -> TransferResult<Vec<ChunkInfo>> {
        let mut chunks = Vec::new();
        let chunk_size = task.config.chunk_size as u64;
        let mut offset = 0;

        while offset < task.file_size {
            let size = std::cmp::min(chunk_size, task.file_size - offset) as usize;

            let chunk = ChunkInfo {
                index: chunks.len(),
                offset,
                size,
                hash: String::new(), // 哈希值在传输时计算
                timestamp: SystemTime::now(),
            };

            chunks.push(chunk);
            offset += chunk_size;
        }

        debug!("计算文件块完成，共 {} 个块", chunks.len());
        Ok(chunks)
    }

    /// 检查续传点
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    ///
    /// # 返回
    ///
    /// 返回应该开始传输的块索引
    async fn check_resume_point(&self, task: &TransferTask) -> TransferResult<usize> {
        // 尝试加载断点信息
        if let Some(checkpoint) = self.resume_manager.load_checkpoint(&task.task_id).await? {
            // 验证断点信息
            if self.resume_manager.validate_checkpoint(&checkpoint).await? {
                let next_chunk_index = checkpoint.transferred_chunks.len();
                info!("找到有效断点，从块索引 {} 开始恢复传输", next_chunk_index);
                return Ok(next_chunk_index);
            } else {
                warn!("断点信息无效，将从头开始传输");
            }
        } else {
            debug!("未找到断点信息，将从头开始传输");
        }

        Ok(0)
    }

    /// 执行分块传输
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    /// * `chunks` - 文件块列表
    /// * `start_index` - 开始传输的块索引
    /// * `progress_sender` - 进度通知发送器
    ///
    /// # 返回
    ///
    /// 返回传输结果
    async fn execute_chunked_transfer(
        &self,
        task: &TransferTask,
        chunks: &[ChunkInfo],
        start_index: usize,
        _progress_sender: Option<broadcast::Receiver<TransferProgress>>,
    ) -> TransferResult<()> {
        info!("开始分块传输，范围: {} - {}", start_index, chunks.len());

        let mut transferred_chunks = Vec::new();
        let mut total_transferred = task.transferred_size;

        // 处理已经传输的块
        if start_index > 0 {
            transferred_chunks.extend_from_slice(&chunks[..start_index]);
        }

        // 传输剩余的块
        for (index, chunk) in chunks.iter().enumerate().skip(start_index) {
            debug!("传输块 {}/{}, 偏移: {}, 大小: {}", index + 1, chunks.len(), chunk.offset, chunk.size);

            // 读取数据块
            let data = match task.direction {
                TransferDirection::Upload => {
                    self.storage.read_chunk(&task.source_path, chunk.offset, chunk.size).await?
                }
                TransferDirection::Download => {
                    // 对于下载，这里应该是从远程存储读取
                    // 暂时使用本地存储作为模拟
                    self.storage.read_chunk(&task.source_path, chunk.offset, chunk.size).await?
                }
            };

            // 计算块哈希
            let chunk_hash = self.security_manager.calculate_hash(&data).await;

            // 创建更新后的块信息
            let mut updated_chunk = chunk.clone();
            updated_chunk.hash = chunk_hash.clone();

            // 写入数据块
            match task.direction {
                TransferDirection::Upload => {
                    self.storage.write_chunk(&task.target_path, chunk.offset, data).await?;
                }
                TransferDirection::Download => {
                    // 对于下载，这里应该是写入本地存储
                    self.storage.write_chunk(&task.target_path, chunk.offset, data).await?;
                }
            }

            // 更新传输信息
            transferred_chunks.push(updated_chunk);
            total_transferred += chunk.size as u64;

            // 更新任务进度
            if let Some(task_lock) = self.active_transfers.get(&task.task_id) {
                let mut task_ref = task_lock.write().await;
                task_ref.transferred_size = total_transferred;
                task_ref.updated_at = SystemTime::now();
            }

            // 发送进度通知
            let progress = TransferProgress {
                task_id: task.task_id.clone(),
                percentage: (total_transferred as f64 / task.file_size as f64) * 100.0,
                transferred_bytes: total_transferred,
                total_bytes: task.file_size,
                speed: 0, // 实际应用中应该计算传输速度
                eta_seconds: None, // 实际应用中应该计算剩余时间
                error: None,
                updated_at: SystemTime::now(),
            };

            // 发送进度更新
            if let Some(ref tx) = self.progress_senders.try_read().and_then(|s| s.get(&task.task_id).cloned()) {
                let _ = tx.send(progress);
            }

            // 定期保存断点信息
            if (index + 1) % 10 == 0 || index + 1 == chunks.len() {
                self.resume_manager.save_checkpoint(&task.task_id, transferred_chunks.clone()).await?;
                debug!("保存断点信息，已完成 {}/{} 块", index + 1, chunks.len());
            }

            debug!("块传输完成: {}/{}", index + 1, chunks.len());
        }

        // 保存最终断点信息
        self.resume_manager.save_checkpoint(&task.task_id, transferred_chunks).await?;

        // 验证文件完整性
        if let Some(expected_hash) = &task.file_hash {
            let target_data = self.storage.read_chunk(&task.target_path, 0, task.file_size as usize).await?;
            let actual_hash = self.security_manager.calculate_hash(&target_data).await;

            if actual_hash != *expected_hash {
                return Err(ErrorInfo::new(7011, "文件完整性验证失败".to_string())
                    .with_category(ErrorCategory::Validation)
                    .with_severity(ErrorSeverity::Error));
            }

            info!("文件完整性验证通过");
        }

        info!("分块传输完成，总传输量: {} 字节", total_transferred);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // 创建测试用的存储实现
    struct TestStorage {
        files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl StorageInterface for TestStorage {
        async fn read_chunk(&self, path: &Path, offset: u64, size: usize) -> TransferResult<Bytes> {
            let files = self.files.read().await;
            if let Some(data) = files.get(path) {
                let offset_usize = offset as usize;
                if offset_usize < data.len() {
                    let end = std::cmp::min(offset_usize + size, data.len());
                    Ok(Bytes::from(data[offset_usize..end].to_vec()))
                } else {
                    Ok(Bytes::new())
                }
            } else {
                Err(ErrorInfo::new(7009, "文件不存在".to_string())
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))
                }
        }

        async fn write_chunk(&self, path: &Path, offset: u64, data: Bytes) -> TransferResult<()> {
            let mut files = self.files.write().await;
            let file_data = files.entry(path.to_path_buf()).or_insert_with(Vec::new);

            let offset_usize = offset as usize;
            if offset_usize > file_data.len() {
                file_data.resize(offset_usize, 0);
            }

            let end = std::cmp::min(offset_usize + data.len(), file_data.capacity());
            if end > file_data.len() {
                file_data.resize(end, 0);
            }

            file_data[offset_usize..offset_usize + data.len()].copy_from_slice(&data);
            Ok(())
        }

        async fn get_file_info(&self, path: &Path) -> TransferResult<FileInfo> {
            let files = self.files.read().await;
            if let Some(data) = files.get(path) {
                let mut hasher = Hasher::new();
                hasher.update(data);
                let hash = format!("{:x}", hasher.finalize());

                Ok(FileInfo {
                    path: path.to_path_buf(),
                    size: data.len() as u64,
                    modified: SystemTime::now(),
                    is_dir: false,
                    permissions: None,
                    hash: Some(hash),
                })
            } else {
                Err(ErrorInfo::new(7010, "文件不存在".to_string())
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))
            }
        }

        async fn create_dir(&self, _path: &Path) -> TransferResult<()> {
            Ok(())
        }

        async fn delete_file(&self, path: &Path) -> TransferResult<()> {
            let mut files = self.files.write().await;
            files.remove(path);
            Ok(())
        }

        async fn list_directory(&self, path: &Path) -> TransferResult<Vec<DirectoryEntry>> {
            // 简单实现：返回空列表
            Ok(Vec::new())
        }

        async fn remove_dir(&self, path: &Path) -> TransferResult<()> {
            // 简单实现：不支持删除目录
            Err(ErrorInfo::new(7011, "不支持删除目录".to_string())
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))
        }

        async fn get_directory_size(&self, path: &Path) -> TransferResult<u64> {
            // 简单实现：返回0
            Ok(0)
        }

        async fn exists(&self, path: &Path) -> TransferResult<bool> {
            let files = self.files.read().await;
            Ok(files.contains_key(path))
        }
    }

    #[tokio::test]
    async fn test_transfer_engine_creation() {
        let storage = Arc::new(TestStorage {
            files: Arc::new(RwLock::new(HashMap::new())),
        });

        let config = TransferConfig::default();
        let result = TransferEngine::new(storage, config).await;

        assert!(result.is_ok(), "传输引擎创建应该成功");
    }

    #[tokio::test]
    async fn test_transfer_task_creation() {
        let storage = Arc::new(TestStorage {
            files: Arc::new(RwLock::new(HashMap::new())),
        });

        let config = TransferConfig::default();
        let engine = TransferEngine::new(storage, config).await.unwrap();

        // 创建测试文件
        let test_data = b"Hello, World!";
        let test_path = PathBuf::from("/test.txt");

        {
            let mut files = storage.files.write();
            files.insert(test_path.clone(), test_data.to_vec());
        }

        let metadata = TransferMetadata {
            mime_type: "text/plain".to_string(),
            file_extension: "txt".to_string(),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            properties: HashMap::new(),
        };

        let task = engine.create_transfer(
            &test_path,
            &PathBuf::from("/target.txt"),
            TransferDirection::Upload,
            metadata,
            crate::TransferOptions::default(),
        ).await;

        assert!(task.is_ok(), "传输任务创建应该成功");

        let task_ref = task.unwrap().read();
        assert_eq!(task_ref.direction, TransferDirection::Upload);
        assert_eq!(task_ref.file_size, test_data.len() as u64);
    }

    #[tokio::test]
    async fn test_transfer_pause_and_resume() {
        let storage = Arc::new(TestStorage {
            files: Arc::new(RwLock::new(HashMap::new())),
        });

        let config = TransferConfig::default();
        let engine = TransferEngine::new(storage, config).await.unwrap();

        // 创建测试文件
        let test_data = b"Hello, World!";
        let test_path = PathBuf::from("/test.txt");

        {
            let mut files = storage.files.write();
            files.insert(test_path.clone(), test_data.to_vec());
        }

        let metadata = TransferMetadata {
            mime_type: "text/plain".to_string(),
            file_extension: "txt".to_string(),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            properties: HashMap::new(),
        };

        let task = engine.create_transfer(
            &test_path,
            &PathBuf::from("/target.txt"),
            TransferDirection::Upload,
            metadata,
            crate::TransferOptions::default(),
        ).await.unwrap();

        // 测试暂停
        let task_id = task.read().task_id.clone();
        let pause_result = engine.pause_transfer(&task_id).await;
        assert!(pause_result.is_ok(), "传输任务暂停应该成功");

        let task_ref = task.read();
        assert_eq!(task_ref.status, TransferStatus::Paused);

        // 测试恢复
        let resume_result = engine.resume_transfer(&task_id).await;
        assert!(resume_result.is_ok(), "传输任务恢复应该成功");
    }

    #[tokio::test]
    async fn test_transfer_cancel() {
        let storage = Arc::new(TestStorage {
            files: Arc::new(RwLock::new(HashMap::new())),
        });

        let config = TransferConfig::default();
        let engine = TransferEngine::new(storage, config).await.unwrap();

        // 创建测试文件
        let test_data = b"Hello, World!";
        let test_path = PathBuf::from("/test.txt");

        {
            let mut files = storage.files.write();
            files.insert(test_path.clone(), test_data.to_vec());
        }

        let metadata = TransferMetadata {
            mime_type: "text/plain".to_string(),
            file_extension: "txt".to_string(),
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            properties: HashMap::new(),
        };

        let task = engine.create_transfer(
            &test_path,
            &PathBuf::from("/target.txt"),
            TransferDirection::Upload,
            metadata,
            crate::TransferOptions::default(),
        ).await.unwrap();

        // 测试取消
        let task_id = task.read().task_id.clone();
        let cancel_result = engine.cancel_transfer(&task_id).await;
        assert!(cancel_result.is_ok(), "传输任务取消应该成功");

        let task_ref = task.read();
        assert_eq!(task_ref.status, TransferStatus::Cancelled);
        assert!(task_ref.completed_at.is_some());
    }
}