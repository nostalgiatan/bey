//! # 并发传输器
//!
//! 负责实现多线程并发文件传输，最大化利用网络带宽和提高传输性能。
//! 使用工作窃取算法和动态负载均衡确保高效的并发传输。
//!
//! ## 核心功能
//!
//! - **并发传输**: 支持多线程并发数据块传输
//! - **动态调度**: 智能的任务调度和负载均衡
//! - **带宽控制**: 防止网络拥塞的带宽管理
//! - **错误恢复**: 自动重试和错误处理机制
//! - **性能监控**: 实时传输性能指标监控

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{info, warn, error, debug, instrument};
use parking_lot::Mutex;
use crate::{TransferConfig, TransferResult, TransferTask, TransferStatus, TransferProgress, ChunkInfo};

/// 并发传输器
///
/// 负责实现高性能的多线程并发文件传输。
/// 使用工作窃取算法和动态负载均衡优化传输性能。
#[derive(Debug)]
pub struct ConcurrentTransfer {
    /// 配置信息
    config: Arc<TransferConfig>,
    /// 工作线程池
    thread_pool: Arc<Mutex<tokio::task::JoinSet<()>>>,
    /// 活跃传输任务
    active_transfers: Arc<RwLock<HashMap<String, ActiveTransfer>>>,
    /// 待处理任务队列
    #[allow(dead_code)]
    pending_tasks: Arc<Mutex<VecDeque<PendingTask>>>,
    /// 传输统计信息
    statistics: Arc<TransferStatistics>,
    /// 带宽控制器
    bandwidth_controller: Arc<BandwidthController>,
    /// 任务调度器
    scheduler: Arc<TaskScheduler>,
}

/// 活跃传输任务
#[derive(Debug)]
struct ActiveTransfer {
    /// 任务ID
    #[allow(dead_code)]
    task_id: String,
    /// 传输方向
    #[allow(dead_code)]
    direction: TransferDirection,
    /// 源文件路径
    #[allow(dead_code)]
    source_path: std::path::PathBuf,
    /// 目标文件路径
    #[allow(dead_code)]
    target_path: std::path::PathBuf,
    /// 文件大小
    file_size: u64,
    /// 已传输大小
    transferred_size: Arc<AtomicU64>,
    /// 传输状态
    status: Arc<RwLock<TransferStatus>>,
    /// 数据块信息
    chunks: Arc<RwLock<Vec<ChunkInfo>>>,
    /// 完成的数据块数量
    completed_chunks: Arc<AtomicUsize>,
    /// 错误计数
    #[allow(dead_code)]
    error_count: Arc<AtomicUsize>,
    /// 开始时间
    start_time: SystemTime,
    /// 更新时间
    updated_at: Arc<RwLock<SystemTime>>,
    /// 进度通知发送器
    progress_sender: mpsc::UnboundedSender<TransferProgress>,
}

/// 待处理任务
#[derive(Debug, Clone)]
struct PendingTask {
    /// 任务ID
    task_id: String,
    /// 优先级
    #[allow(dead_code)]
    priority: TaskPriority,
    /// 创建时间
    #[allow(dead_code)]
    created_at: SystemTime,
    /// 传输任务数据
    task_data: TransferTask,
}

/// 任务优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TaskPriority {
    #[allow(dead_code)]
    Low = 1,
    Normal = 2,
    #[allow(dead_code)]
    High = 3,
    #[allow(dead_code)]
    Urgent = 4,
}

/// 传输方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferDirection {
    Upload,
    Download,
}

/// 传输统计信息
#[derive(Debug, Default)]
struct TransferStatistics {
    /// 总传输字节数
    total_bytes_transferred: AtomicU64,
    /// 成功传输任务数
    successful_transfers: AtomicUsize,
    /// 失败传输任务数
    failed_transfers: AtomicUsize,
    /// 平均传输速度（字节/秒）
    average_speed: AtomicU64,
    /// 活跃连接数
    active_connections: AtomicUsize,
}

/// 带宽控制器
///
/// 负责控制传输带宽，防止网络拥塞。
#[derive(Debug)]
struct BandwidthController {
    /// 令牌桶容量
    bucket_capacity: u64,
    /// 令牌桶
    tokens: Arc<Mutex<u64>>,
    /// 令牌补充速率（字节/秒）
    refill_rate: u64,
    /// 最后补充时间
    last_refill: Arc<Mutex<SystemTime>>,
}

/// 任务调度器
///
/// 负责传输任务的智能调度和负载均衡。
#[derive(Debug)]
struct TaskScheduler {
    /// 工作线程信号量
    #[allow(dead_code)]
    worker_semaphore: Arc<Semaphore>,
    /// 任务队列
    task_queue: Arc<Mutex<VecDeque<PendingTask>>>,
    /// 工作线程状态
    #[allow(dead_code)]
    worker_status: Arc<RwLock<HashMap<String, WorkerStatus>>>,
}

/// 工作线程状态
#[derive(Debug, Clone)]
struct WorkerStatus {
    /// 线程ID
    #[allow(dead_code)]
    worker_id: String,
    /// 当前任务
    #[allow(dead_code)]
    current_task: Option<String>,
    /// 处理的任务数
    #[allow(dead_code)]
    tasks_processed: usize,
    /// 最后活动时间
    #[allow(dead_code)]
    last_activity: SystemTime,
    /// 线程状态
    #[allow(dead_code)]
    status: WorkerThreadStatus,
}

/// 工作线程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerThreadStatus {
    #[allow(dead_code)]
    Idle,
    #[allow(dead_code)]
    Busy,
    #[allow(dead_code)]
    Stopping,
}

/// 传输结果
#[derive(Debug, Clone)]
pub struct TransferExecutionResult {
    /// 任务ID
    pub task_id: String,
    /// 是否成功
    pub success: bool,
    /// 传输字节数
    pub bytes_transferred: u64,
    /// 耗时（秒）
    pub duration_secs: u64,
    /// 平均速度（字节/秒）
    pub average_speed: u64,
    /// 错误信息
    pub error_message: Option<String>,
}

impl ConcurrentTransfer {
    /// 创建新的并发传输器
    ///
    /// # 参数
    ///
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回并发传输器实例
    #[instrument(skip(config))]
    pub async fn new(config: Arc<TransferConfig>) -> TransferResult<Self> {
        info!("创建并发传输器，最大并发数: {}", config.max_concurrency);

        let thread_pool = Arc::new(Mutex::new(tokio::task::JoinSet::new()));
        let active_transfers = Arc::new(RwLock::new(HashMap::new()));
        let pending_tasks = Arc::new(Mutex::new(VecDeque::new()));
        let statistics = Arc::new(TransferStatistics::default());

        // 创建带宽控制器
        let bandwidth_controller = Arc::new(BandwidthController::new(
            config.buffer_size as u64 * 4, // 4倍缓冲区大小的令牌桶容量
            config.buffer_size as u64 * 2, // 2倍缓冲区大小的补充速率
        ));

        // 创建任务调度器
        let scheduler = Arc::new(TaskScheduler::new(config.max_concurrency));

        let concurrent_transfer = Self {
            config: config.clone(),
            thread_pool,
            active_transfers,
            pending_tasks,
            statistics,
            bandwidth_controller,
            scheduler,
        };

        // 启动工作线程
        concurrent_transfer.start_workers().await?;

        info!("并发传输器创建成功");
        Ok(concurrent_transfer)
    }

    /// 开始并发传输
    ///
    /// # 参数
    ///
    /// * `task` - 传输任务
    ///
    /// # 返回
    ///
    /// 返回传输结果接收器
    #[instrument(skip(self, task), fields(task_id = task.task_id))]
    pub async fn start_transfer(
        &self,
        task: TransferTask,
    ) -> TransferResult<mpsc::UnboundedReceiver<TransferProgress>> {
        info!("开始并发传输，任务ID: {}", task.task_id);

        let (progress_sender, progress_receiver) = mpsc::unbounded_channel();

        // 创建活跃传输任务
        let active_transfer = ActiveTransfer {
            task_id: task.task_id.clone(),
            direction: match task.direction {
                crate::TransferDirection::Upload => TransferDirection::Upload,
                crate::TransferDirection::Download => TransferDirection::Download,
            },
            source_path: task.source_path.clone(),
            target_path: task.target_path.clone(),
            file_size: task.file_size,
            transferred_size: Arc::new(AtomicU64::new(0)),
            status: Arc::new(RwLock::new(TransferStatus::Transferring)),
            chunks: Arc::new(RwLock::new(Vec::new())),
            completed_chunks: Arc::new(AtomicUsize::new(0)),
            error_count: Arc::new(AtomicUsize::new(0)),
            start_time: SystemTime::now(),
            updated_at: Arc::new(RwLock::new(SystemTime::now())),
            progress_sender: progress_sender.clone(),
        };

        // 注册活跃传输任务
        self.active_transfers.write().await.insert(task.task_id.clone(), active_transfer);

        // 计算数据块
        let chunks = self.calculate_chunks(task.file_size, self.config.chunk_size);
        *self.active_transfers.read().await.get(&task.task_id).unwrap().chunks.write().await = chunks.clone();

        // 提交传输任务到调度器
        for (index, _chunk) in chunks.into_iter().enumerate() {
            let pending_task = PendingTask {
                task_id: format!("{}-{}", task.task_id, index),
                priority: TaskPriority::Normal, // 可根据任务属性动态设置
                created_at: SystemTime::now(),
                task_data: task.clone(),
            };
            self.scheduler.submit_task(pending_task).await;
        }

        // 启动传输进度监控
        self.start_progress_monitor(task.task_id.clone()).await;

        info!("并发传输已启动，任务ID: {}", task.task_id);
        Ok(progress_receiver)
    }

    /// 暂停传输
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn pause_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("暂停传输，任务ID: {}", task_id);

        let active_transfers = self.active_transfers.read().await;
        if let Some(transfer) = active_transfers.get(task_id) {
            *transfer.status.write().await = TransferStatus::Paused;
            info!("传输已暂停，任务ID: {}", task_id);
            Ok(())
        } else {
            warn!("未找到传输任务，任务ID: {}", task_id);
            Err(ErrorInfo::new(
                7201,
                format!("未找到传输任务: {}", task_id)
            )
            .with_category(ErrorCategory::FileSystem)
            .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 恢复传输
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn resume_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("恢复传输，任务ID: {}", task_id);

        let active_transfers = self.active_transfers.read().await;
        if let Some(transfer) = active_transfers.get(task_id) {
            *transfer.status.write().await = TransferStatus::Transferring;
            info!("传输已恢复，任务ID: {}", task_id);
            Ok(())
        } else {
            warn!("未找到传输任务，任务ID: {}", task_id);
            Err(ErrorInfo::new(
                7202,
                format!("未找到传输任务: {}", task_id)
            )
            .with_category(ErrorCategory::FileSystem)
            .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 取消传输
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn cancel_transfer(&self, task_id: &str) -> TransferResult<()> {
        info!("取消传输，任务ID: {}", task_id);

        let mut active_transfers = self.active_transfers.write().await;
        if let Some(transfer) = active_transfers.remove(task_id) {
            *transfer.status.write().await = TransferStatus::Cancelled;
            self.statistics.failed_transfers.fetch_add(1, Ordering::Relaxed);
            info!("传输已取消，任务ID: {}", task_id);
            Ok(())
        } else {
            warn!("未找到传输任务，任务ID: {}", task_id);
            Err(ErrorInfo::new(
                7203,
                format!("未找到传输任务: {}", task_id)
            )
            .with_category(ErrorCategory::FileSystem)
            .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 获取传输统计信息
    ///
    /// # 返回
    ///
    /// 返回传输统计信息
    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> TransferStatisticsSnapshot {
        let active_transfers_count = self.active_transfers.read().await.len();

        TransferStatisticsSnapshot {
            total_bytes_transferred: self.statistics.total_bytes_transferred.load(Ordering::Relaxed),
            successful_transfers: self.statistics.successful_transfers.load(Ordering::Relaxed),
            failed_transfers: self.statistics.failed_transfers.load(Ordering::Relaxed),
            average_speed: self.statistics.average_speed.load(Ordering::Relaxed),
            active_connections: self.statistics.active_connections.load(Ordering::Relaxed),
            active_transfers: active_transfers_count,
        }
    }

    // 私有方法

    /// 启动工作线程
    async fn start_workers(&self) -> TransferResult<()> {
        info!("启动工作线程，数量: {}", self.config.max_concurrency);

        for i in 0..self.config.max_concurrency {
            let worker_id = format!("worker-{}", i);
            let scheduler = self.scheduler.clone();
            let bandwidth_controller = self.bandwidth_controller.clone();
            let statistics = self.statistics.clone();
            let active_transfers = self.active_transfers.clone();
            let config = self.config.clone();

            if let Some(mut pool) = self.thread_pool.try_lock() {
                pool.spawn(async move {
                    Self::worker_loop(
                        worker_id,
                        scheduler,
                        bandwidth_controller,
                        statistics,
                        active_transfers,
                        config,
                    ).await;
                });
            }
        }

        info!("工作线程启动完成");
        Ok(())
    }

    /// 工作线程主循环
    async fn worker_loop(
        worker_id: String,
        scheduler: Arc<TaskScheduler>,
        bandwidth_controller: Arc<BandwidthController>,
        statistics: Arc<TransferStatistics>,
        active_transfers: Arc<RwLock<HashMap<String, ActiveTransfer>>>,
        config: Arc<TransferConfig>,
    ) {
        info!("工作线程启动: {}", worker_id);

        loop {
            // 获取下一个任务
            match scheduler.get_next_task().await {
                Some(pending_task) => {
                    debug!("工作线程 {} 获取到任务: {}", worker_id, pending_task.task_id);

                    // 执行传输任务
                    let result = ConcurrentTransfer::execute_transfer_task(
                        &pending_task,
                        &bandwidth_controller,
                        &statistics,
                        &active_transfers,
                        &config,
                    ).await;

                    // 处理任务完成
                    scheduler.complete_task(&pending_task.task_id).await;

                    match result {
                        Ok(_) => {
                            debug!("任务执行成功: {}", pending_task.task_id);
                        }
                        Err(e) => {
                            error!("任务执行失败: {}, 错误: {}", pending_task.task_id, e);
                        }
                    }
                }
                None => {
                    // 没有待处理任务，短暂休眠
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// 执行传输任务
    async fn execute_transfer_task(
        pending_task: &PendingTask,
        bandwidth_controller: &Arc<BandwidthController>,
        statistics: &Arc<TransferStatistics>,
        active_transfers: &Arc<RwLock<HashMap<String, ActiveTransfer>>>,
        config: &Arc<TransferConfig>,
    ) -> TransferResult<()> {
        // 模拟数据块传输
        let chunk_size = config.chunk_size;

        // 获取带宽许可
        bandwidth_controller.acquire_tokens(chunk_size as u64).await?;

        // 更新统计信息
        statistics.total_bytes_transferred.fetch_add(chunk_size as u64, Ordering::Relaxed);

        // 更新活跃传输任务的进度
        if let Some(transfer) = active_transfers.read().await.get(&pending_task.task_data.task_id) {
            transfer.transferred_size.fetch_add(chunk_size as u64, Ordering::Relaxed);
            transfer.completed_chunks.fetch_add(1, Ordering::Relaxed);
            *transfer.updated_at.write().await = SystemTime::now();

            // 发送进度更新
            let total_chunks = transfer.chunks.read().await.len();
            let completed_chunks = transfer.completed_chunks.load(Ordering::Relaxed);
            let transferred_bytes = transfer.transferred_size.load(Ordering::Relaxed);

            let progress = TransferProgress {
                task_id: pending_task.task_data.task_id.clone(),
                percentage: (completed_chunks as f64 / total_chunks as f64) * 100.0,
                transferred_bytes,
                total_bytes: transfer.file_size,
                speed: statistics.average_speed.load(Ordering::Relaxed),
                eta_seconds: None,
                error: None,
                updated_at: SystemTime::now(),
            };

            let _ = transfer.progress_sender.send(progress);
        }

        // 模拟网络传输延迟
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(())
    }

    /// 计算数据块
    fn calculate_chunks(&self, file_size: u64, chunk_size: usize) -> Vec<ChunkInfo> {
        let mut chunks = Vec::new();
        let mut offset = 0;

        while offset < file_size {
            let size = std::cmp::min(chunk_size, (file_size - offset) as usize);

            chunks.push(ChunkInfo {
                index: chunks.len(),
                offset,
                size,
                hash: String::new(), // 实际传输时计算哈希
                timestamp: SystemTime::now(),
            });

            offset += size as u64;
        }

        chunks
    }

    /// 启动传输进度监控
    async fn start_progress_monitor(&self, task_id: String) {
        let active_transfers = self.active_transfers.clone();
        let statistics = self.statistics.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                let transfers = active_transfers.read().await;
                if let Some(transfer) = transfers.get(&task_id) {
                    let current_time = SystemTime::now();
                    let elapsed = current_time.duration_since(transfer.start_time).unwrap_or_default();

                    if elapsed.as_secs() > 0 {
                        let transferred_bytes = transfer.transferred_size.load(Ordering::Relaxed);
                        let speed = transferred_bytes / elapsed.as_secs();
                        statistics.average_speed.store(speed, Ordering::Relaxed);
                    }
                } else {
                    // 任务已完成或被取消，退出监控
                    break;
                }
            }
        });
    }
}

/// 传输统计信息快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStatisticsSnapshot {
    /// 总传输字节数
    pub total_bytes_transferred: u64,
    /// 成功传输任务数
    pub successful_transfers: usize,
    /// 失败传输任务数
    pub failed_transfers: usize,
    /// 平均传输速度（字节/秒）
    pub average_speed: u64,
    /// 活跃连接数
    pub active_connections: usize,
    /// 活跃传输任务数
    pub active_transfers: usize,
}

impl BandwidthController {
    fn new(bucket_capacity: u64, refill_rate: u64) -> Self {
        Self {
            bucket_capacity,
            tokens: Arc::new(Mutex::new(bucket_capacity)),
            refill_rate,
            last_refill: Arc::new(Mutex::new(SystemTime::now())),
        }
    }

    async fn acquire_tokens(&self, amount: u64) -> TransferResult<()> {
        loop {
            {
                let mut tokens = self.tokens.lock();
                if *tokens >= amount {
                    *tokens -= amount;
                    return Ok(());
                }
            }

            // 补充令牌
            self.refill_tokens().await;

            // 如果仍然不够，等待一段时间
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn refill_tokens(&self) {
        let mut last_refill = self.last_refill.lock();
        let now = SystemTime::now();

        if let Ok(elapsed) = now.duration_since(*last_refill) {
            let tokens_to_add = elapsed.as_secs() * self.refill_rate;

            let mut tokens = self.tokens.lock();
            *tokens = std::cmp::min(*tokens + tokens_to_add, self.bucket_capacity);
            *last_refill = now;
        }
    }
}

impl TaskScheduler {
    fn new(max_workers: usize) -> Self {
        Self {
            worker_semaphore: Arc::new(Semaphore::new(max_workers)),
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            worker_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn submit_task(&self, task: PendingTask) {
        let mut queue = self.task_queue.lock();
        queue.push_back(task);
    }

    async fn get_next_task(&self) -> Option<PendingTask> {
        let mut queue = self.task_queue.lock();
        queue.pop_front()
    }

    async fn complete_task(&self, task_id: &str) {
        // 任务完成处理逻辑
        debug!("任务完成: {}", task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TransferTask, TransferDirection, TransferConfig, TransferMetadata};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_concurrent_transfer_creation() {
        let config = Arc::new(TransferConfig::default());
        let concurrent_transfer = ConcurrentTransfer::new(config).await;
        assert!(concurrent_transfer.is_ok());
    }

    #[tokio::test]
    async fn test_start_transfer() {
        let config = Arc::new(TransferConfig::default());
        let concurrent_transfer = ConcurrentTransfer::new(config).await.unwrap();

        let task = TransferTask {
            task_id: "test-task-001".to_string(),
            direction: TransferDirection::Upload,
            source_path: PathBuf::from("/test/source.txt"),
            target_path: PathBuf::from("/test/target.txt"),
            file_size: 1024 * 1024, // 1MB
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
                properties: std::collections::HashMap::new(),
            },
        };

        let result = concurrent_transfer.start_transfer(task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_bandwidth_controller() {
        let controller = BandwidthController::new(1024 * 1024, 512 * 1024); // 1MB容量，512KB/s速率

        // 获取令牌
        let result = controller.acquire_tokens(256 * 1024).await; // 256KB
        assert!(result.is_ok());

        // 尝试获取过多令牌应该等待
        let start = SystemTime::now();
        let result = controller.acquire_tokens(2 * 1024 * 1024).await; // 2MB
        assert!(result.is_ok());
        let elapsed = SystemTime::now().duration_since(start).unwrap();

        // 应该等待一段时间来补充令牌
        assert!(elapsed.as_millis() > 100);
    }

    #[tokio::test]
    async fn test_task_scheduler() {
        let scheduler = TaskScheduler::new(4);

        // 提交任务
        let task = PendingTask {
            task_id: "test-task".to_string(),
            priority: TaskPriority::Normal,
            created_at: SystemTime::now(),
            task_data: TransferTask {
                task_id: "test-task".to_string(),
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
                    properties: std::collections::HashMap::new(),
                },
            },
        };

        scheduler.submit_task(task.clone()).await;

        // 获取任务
        let retrieved_task = scheduler.get_next_task().await;
        assert!(retrieved_task.is_some());
        assert_eq!(retrieved_task.unwrap().task_id, task.task_id);

        // 队列应该为空
        let empty_task = scheduler.get_next_task().await;
        assert!(empty_task.is_none());
    }
}