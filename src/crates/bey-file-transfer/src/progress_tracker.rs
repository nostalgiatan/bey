//! # 进度跟踪器
//!
//! 负责实时跟踪文件传输进度并提供详细的性能指标监控。
//! 支持多任务并发进度跟踪和实时进度通知机制。
//!
//! ## 核心功能
//!
//! - **实时监控**: 实时跟踪传输进度和性能指标
//! - **进度计算**: 精确的传输进度百分比和速度计算
//! - **性能监控**: 传输速度、剩余时间等关键指标监控
//! - **事件通知**: 进度变化事件的实时广播机制
//! - **历史记录**: 完整的进度历史数据记录和分析

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, debug, instrument};
use parking_lot::Mutex;
use dashmap::DashMap;
use crate::{TransferProgress, TransferResult, TransferStatus};

/// 进度跟踪器
///
/// 负责实时跟踪文件传输进度并提供详细的性能指标。
/// 支持多任务并发跟踪和实时进度广播。
#[derive(Debug)]
pub struct ProgressTracker {
    /// 进度数据存储
    progress_data: Arc<DashMap<String, ProgressState>>,
    /// 进度通知发送器映射
    progress_senders: Arc<RwLock<HashMap<String, broadcast::Sender<TransferProgress>>>>,
    /// 性能统计器
    performance_tracker: Arc<PerformanceTracker>,
    /// 配置参数
    config: ProgressTrackerConfig,
}

/// 进度状态
#[derive(Debug, Clone)]
struct ProgressState {
    /// 任务ID
    #[allow(dead_code)]
    task_id: String,
    /// 总字节数
    total_bytes: u64,
    /// 已传输字节数
    transferred_bytes: Arc<std::sync::atomic::AtomicU64>,
    /// 传输状态
    status: Arc<RwLock<TransferStatus>>,
    /// 开始时间
    start_time: SystemTime,
    /// 最后更新时间
    last_update: Arc<RwLock<SystemTime>>,
    /// 速度历史记录
    speed_history: Arc<Mutex<VecDeque<SpeedRecord>>>,
    /// 进度历史记录
    progress_history: Arc<Mutex<VecDeque<ProgressSnapshot>>>,
    /// 错误信息
    error_info: Arc<RwLock<Option<String>>>,
}

/// 速度记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpeedRecord {
    /// 时间戳
    timestamp: SystemTime,
    /// 瞬时速度（字节/秒）
    instant_speed: u64,
    /// 平滑速度（字节/秒）
    smooth_speed: u64,
}

/// 进度快照
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgressSnapshot {
    /// 时间戳
    timestamp: SystemTime,
    /// 进度百分比
    percentage: f64,
    /// 已传输字节数
    transferred_bytes: u64,
    /// 传输速度
    speed: u64,
}

/// 性能跟踪器
///
/// 负责计算和维护传输性能指标。
#[derive(Debug)]
struct PerformanceTracker {
    /// 全局统计信息
    global_stats: Arc<RwLock<GlobalPerformanceStats>>,
    /// 活跃任务计数
    active_tasks: Arc<std::sync::atomic::AtomicUsize>,
    /// 窗口大小（用于速度计算）
    #[allow(dead_code)]
    window_size: Duration,
}

/// 全局性能统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPerformanceStats {
    /// 总传输字节数
    pub total_bytes_transferred: u64,
    /// 总传输任务数
    pub total_tasks: usize,
    /// 成功任务数
    pub successful_tasks: usize,
    /// 失败任务数
    pub failed_tasks: usize,
    /// 平均传输速度（字节/秒）
    pub average_speed: u64,
    /// 峰值传输速度（字节/秒）
    pub peak_speed: u64,
    /// 总传输时间（秒）
    pub total_transfer_time: u64,
    /// 最后更新时间
    pub last_updated: SystemTime,
}

impl Default for GlobalPerformanceStats {
    fn default() -> Self {
        Self {
            total_bytes_transferred: 0,
            total_tasks: 0,
            successful_tasks: 0,
            failed_tasks: 0,
            average_speed: 0,
            peak_speed: 0,
            total_transfer_time: 0,
            last_updated: SystemTime::UNIX_EPOCH,
        }
    }
}

/// 进度跟踪器配置
#[derive(Debug, Clone)]
struct ProgressTrackerConfig {
    /// 更新间隔（毫秒）
    #[allow(dead_code)]
    update_interval_ms: u64,
    /// 速度历史窗口大小
    speed_history_size: usize,
    /// 进度历史窗口大小
    progress_history_size: usize,
    /// 速度平滑因子（0.0-1.0）
    speed_smoothing_factor: f64,
    /// 窗口大小
    window_size: Duration,
}

impl Default for ProgressTrackerConfig {
    fn default() -> Self {
        Self {
            update_interval_ms: 1000, // 1秒更新间隔
            speed_history_size: 60,   // 保留60个速度记录
            progress_history_size: 100, // 保留100个进度记录
            speed_smoothing_factor: 0.3, // 30%平滑因子
            window_size: Duration::from_secs(60), // 60秒窗口
        }
    }
}

impl ProgressTracker {
    /// 创建新的进度跟踪器
    ///
    /// # 返回
    ///
    /// 返回进度跟踪器实例
    #[instrument]
    pub fn new() -> Self {
        info!("创建进度跟踪器");

        let config = ProgressTrackerConfig::default();
        let performance_tracker = Arc::new(PerformanceTracker::new(config.window_size));

        Self {
            progress_data: Arc::new(DashMap::new()),
            progress_senders: Arc::new(RwLock::new(HashMap::new())),
            performance_tracker,
            config,
        }
    }

    /// 注册传输任务
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    /// * `total_bytes` - 总字节数
    ///
    /// # 返回
    ///
    /// 返回进度更新接收器
    #[instrument(skip(self), fields(task_id, total_bytes))]
    pub async fn register_task(&self, task_id: String, total_bytes: u64) -> TransferResult<broadcast::Receiver<TransferProgress>> {
        info!("注册传输任务，任务ID: {}, 总大小: {} 字节", task_id, total_bytes);

        // 创建进度状态
        let progress_state = ProgressState {
            task_id: task_id.clone(),
            total_bytes,
            transferred_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            status: Arc::new(RwLock::new(TransferStatus::Pending)),
            start_time: SystemTime::now(),
            last_update: Arc::new(RwLock::new(SystemTime::now())),
            speed_history: Arc::new(Mutex::new(VecDeque::with_capacity(self.config.speed_history_size))),
            progress_history: Arc::new(Mutex::new(VecDeque::with_capacity(self.config.progress_history_size))),
            error_info: Arc::new(RwLock::new(None)),
        };

        // 存储进度状态
        self.progress_data.insert(task_id.clone(), progress_state);

        // 创建进度通知发送器
        let (sender, receiver) = broadcast::channel(100);
        self.progress_senders.write().await.insert(task_id.clone(), sender);

        // 更新性能统计
        self.performance_tracker.register_task().await;

        info!("任务注册成功，任务ID: {}", task_id);
        Ok(receiver)
    }

    /// 更新传输进度
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    /// * `transferred_bytes` - 已传输字节数
    /// * `status` - 传输状态
    ///
    /// # 返回
    ///
    /// 返回成功或错误信息
    #[instrument(skip(self), fields(task_id, transferred_bytes))]
    pub async fn update_progress(
        &self,
        task_id: &str,
        transferred_bytes: u64,
        status: TransferStatus,
    ) -> TransferResult<()> {
        debug!("更新传输进度，任务ID: {}, 已传输: {} 字节", task_id, transferred_bytes);

        // 获取进度状态
        let progress_state = match self.progress_data.get(task_id) {
            Some(state) => state,
            None => {
                warn!("未找到任务进度状态，任务ID: {}", task_id);
                return Err(ErrorInfo::new(
                    7301,
                    format!("未找到任务进度状态: {}", task_id)
                )
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Warning));
            }
        };

        // 更新进度数据
        progress_state.transferred_bytes.store(transferred_bytes, std::sync::atomic::Ordering::Relaxed);
        *progress_state.status.write().await = status;
        *progress_state.last_update.write().await = SystemTime::now();

        // 计算当前进度
        let percentage = if progress_state.total_bytes > 0 {
            (transferred_bytes as f64 / progress_state.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        // 计算传输速度
        let current_time = SystemTime::now();
        let elapsed = current_time.duration_since(progress_state.start_time).unwrap_or_default();
        let speed = if elapsed.as_secs() > 0 {
            transferred_bytes / elapsed.as_secs()
        } else {
            0
        };

        // 更新速度历史
        self.update_speed_history(&progress_state, speed, current_time).await;

        // 更新进度历史
        self.update_progress_history(&progress_state, percentage, transferred_bytes, speed, current_time).await;

        // 创建进度通知
        let progress = TransferProgress {
            task_id: task_id.to_string(),
            percentage,
            transferred_bytes,
            total_bytes: progress_state.total_bytes,
            speed,
            eta_seconds: self.calculate_eta(&progress_state).await,
            error: progress_state.error_info.read().await.clone(),
            updated_at: current_time,
        };

        // 广播进度更新
        self.broadcast_progress_update(task_id, progress).await?;

        // 更新全局性能统计
        self.performance_tracker.update_global_stats(transferred_bytes, speed).await;

        debug!("进度更新完成，任务ID: {}, 进度: {:.1}%, 速度: {} 字节/秒",
               task_id, percentage, speed);

        Ok(())
    }

    /// 设置错误信息
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    /// * `error_message` - 错误信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn set_error(&self, task_id: &str, error_message: String) {
        warn!("设置传输错误，任务ID: {}, 错误: {}", task_id, error_message);

        if let Some(progress_state) = self.progress_data.get(task_id) {
            *progress_state.error_info.write().await = Some(error_message.clone());
            *progress_state.status.write().await = TransferStatus::Failed;

            // 广播错误信息
            let progress = TransferProgress {
                task_id: task_id.to_string(),
                percentage: 0.0,
                transferred_bytes: progress_state.transferred_bytes.load(std::sync::atomic::Ordering::Relaxed),
                total_bytes: progress_state.total_bytes,
                speed: 0,
                eta_seconds: None,
                error: Some(error_message),
                updated_at: SystemTime::now(),
            };

            let _ = self.broadcast_progress_update(task_id, progress).await;
        }
    }

    /// 获取任务进度
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    ///
    /// # 返回
    ///
    /// 返回当前进度信息
    #[instrument(skip(self), fields(task_id))]
    pub async fn get_progress(&self, task_id: &str) -> TransferResult<Option<TransferProgress>> {
        debug!("获取任务进度，任务ID: {}", task_id);

        if let Some(progress_state) = self.progress_data.get(task_id) {
            let transferred_bytes = progress_state.transferred_bytes.load(std::sync::atomic::Ordering::Relaxed);
            let _status = progress_state.status.read().await.clone();

            let percentage = if progress_state.total_bytes > 0 {
                (transferred_bytes as f64 / progress_state.total_bytes as f64) * 100.0
            } else {
                0.0
            };

            let elapsed = SystemTime::now().duration_since(progress_state.start_time).unwrap_or_default();
            let speed = if elapsed.as_secs() > 0 {
                transferred_bytes / elapsed.as_secs()
            } else {
                0
            };

            let progress = TransferProgress {
                task_id: task_id.to_string(),
                percentage,
                transferred_bytes,
                total_bytes: progress_state.total_bytes,
                speed,
                eta_seconds: self.calculate_eta(&progress_state).await,
                error: progress_state.error_info.read().await.clone(),
                updated_at: *progress_state.last_update.read().await,
            };

            Ok(Some(progress))
        } else {
            Ok(None)
        }
    }

    /// 获取所有活跃任务的进度
    ///
    /// # 返回
    ///
    /// 返回所有活跃任务的进度信息
    #[instrument(skip(self))]
    pub async fn get_all_progress(&self) -> Vec<TransferProgress> {
        debug!("获取所有活跃任务进度");

        let mut all_progress = Vec::new();

        for entry in self.progress_data.iter() {
            let task_id = entry.key();
            if let Ok(Some(progress)) = self.get_progress(task_id).await {
                all_progress.push(progress);
            }
        }

        all_progress.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        debug!("获取到 {} 个任务的进度信息", all_progress.len());
        all_progress
    }

    /// 获取性能统计信息
    ///
    /// # 返回
    ///
    /// 返回全局性能统计信息
    #[instrument(skip(self))]
    pub async fn get_performance_stats(&self) -> GlobalPerformanceStats {
        self.performance_tracker.get_global_stats().await
    }

    /// 取消任务跟踪
    ///
    /// # 参数
    ///
    /// * `task_id` - 任务ID
    #[instrument(skip(self), fields(task_id))]
    pub async fn unregister_task(&self, task_id: &str) {
        info!("取消任务跟踪，任务ID: {}", task_id);

        // 移除进度数据
        self.progress_data.remove(task_id);

        // 移除进度发送器
        self.progress_senders.write().await.remove(task_id);

        // 更新性能统计
        self.performance_tracker.unregister_task().await;

        info!("任务跟踪已取消，任务ID: {}", task_id);
    }

    /// 清理过期的进度数据
    ///
    /// # 返回
    ///
    /// 返回清理的任务数量
    #[instrument(skip(self))]
    pub async fn cleanup_expired_data(&self) -> usize {
        info!("开始清理过期的进度数据");

        let mut expired_tasks = Vec::new();
        let current_time = SystemTime::now();
        let expiration_threshold = Duration::from_secs(3600); // 1小时过期

        for entry in self.progress_data.iter() {
            let progress_state = entry.value();
            let last_update = *progress_state.last_update.read().await;

            if let Ok(elapsed) = current_time.duration_since(last_update) {
                if elapsed > expiration_threshold {
                    expired_tasks.push(entry.key().clone());
                }
            }
        }

        for task_id in &expired_tasks {
            self.unregister_task(&task_id).await;
        }

        let cleaned_count = expired_tasks.len();
        info!("清理完成，删除了 {} 个过期任务的进度数据", cleaned_count);
        cleaned_count
    }

    // 私有方法

    /// 更新速度历史记录
    async fn update_speed_history(&self, progress_state: &ProgressState, instant_speed: u64, timestamp: SystemTime) {
        let mut speed_history = progress_state.speed_history.lock();

        // 计算平滑速度
        let smooth_speed = if let Some(last_record) = speed_history.back() {
            let smoothing_factor = self.config.speed_smoothing_factor;
            (instant_speed as f64 * (1.0 - smoothing_factor) + last_record.smooth_speed as f64 * smoothing_factor) as u64
        } else {
            instant_speed
        };

        let record = SpeedRecord {
            timestamp,
            instant_speed,
            smooth_speed,
        };

        speed_history.push_back(record);

        // 保持历史记录大小
        if speed_history.len() > self.config.speed_history_size {
            speed_history.pop_front();
        }
    }

    /// 更新进度历史记录
    async fn update_progress_history(
        &self,
        progress_state: &ProgressState,
        percentage: f64,
        transferred_bytes: u64,
        speed: u64,
        timestamp: SystemTime,
    ) {
        let mut progress_history = progress_state.progress_history.lock();

        let snapshot = ProgressSnapshot {
            timestamp,
            percentage,
            transferred_bytes,
            speed,
        };

        progress_history.push_back(snapshot);

        // 保持历史记录大小
        if progress_history.len() > self.config.progress_history_size {
            progress_history.pop_front();
        }
    }

    /// 计算预估剩余时间
    async fn calculate_eta(&self, progress_state: &ProgressState) -> Option<u64> {
        let transferred_bytes = progress_state.transferred_bytes.load(std::sync::atomic::Ordering::Relaxed);

        if transferred_bytes == 0 || progress_state.total_bytes == 0 {
            return None;
        }

        // 使用最近的平均速度计算ETA
        let speed_history = progress_state.speed_history.lock();
        if let Some(avg_speed) = speed_history.iter()
            .rev()
            .take(5) // 使用最近5个记录
            .map(|r| r.smooth_speed)
            .sum::<u64>()
            .checked_div(speed_history.len().min(5) as u64)
        {
            if avg_speed > 0 {
                let remaining_bytes = progress_state.total_bytes.saturating_sub(transferred_bytes);
                let eta_seconds = remaining_bytes / avg_speed;
                return Some(eta_seconds);
            }
        }

        None
    }

    /// 广播进度更新
    async fn broadcast_progress_update(&self, task_id: &str, progress: TransferProgress) -> TransferResult<()> {
        let senders = self.progress_senders.read().await;

        if let Some(sender) = senders.get(task_id) {
            match sender.send(progress.clone()) {
                Ok(receivers_count) => {
                    debug!("进度更新广播成功，任务ID: {}, 接收者数量: {}", task_id, receivers_count);
                }
                Err(_) => {
                    debug!("没有活跃的进度接收器，任务ID: {}", task_id);
                }
            }
        }

        Ok(())
    }
}

impl PerformanceTracker {
    fn new(window_size: Duration) -> Self {
        Self {
            global_stats: Arc::new(RwLock::new(GlobalPerformanceStats::default())),
            active_tasks: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            window_size,
        }
    }

    async fn register_task(&self) {
        self.active_tasks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut stats = self.global_stats.write().await;
        stats.total_tasks += 1;
        stats.last_updated = SystemTime::now();
    }

    async fn unregister_task(&self) {
        self.active_tasks.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    async fn update_global_stats(&self, transferred_bytes: u64, speed: u64) {
        let mut stats = self.global_stats.write().await;
        stats.total_bytes_transferred += transferred_bytes;

        // 更新平均速度
        if stats.total_transfer_time > 0 {
            stats.average_speed = stats.total_bytes_transferred / stats.total_transfer_time;
        }

        // 更新峰值速度
        if speed > stats.peak_speed {
            stats.peak_speed = speed;
        }

        stats.last_updated = SystemTime::now();
    }

    async fn get_global_stats(&self) -> GlobalPerformanceStats {
        self.global_stats.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransferStatus;

    #[tokio::test]
    async fn test_progress_tracker_creation() {
        let tracker = ProgressTracker::new();
        assert_eq!(tracker.progress_data.len(), 0);
    }

    #[tokio::test]
    async fn test_register_task() {
        let tracker = ProgressTracker::new();
        let task_id = "test-task-001".to_string();
        let total_bytes = 1024 * 1024; // 1MB

        let mut receiver = tracker.register_task(task_id.clone(), total_bytes).await.unwrap();

        // 验证任务已注册
        assert_eq!(tracker.progress_data.len(), 1);
        assert!(tracker.progress_data.contains_key(&task_id));

        // 验证接收器工作
        drop(receiver);
    }

    #[tokio::test]
    async fn test_update_progress() {
        let tracker = ProgressTracker::new();
        let task_id = "test-task-002".to_string();
        let total_bytes = 1000;

        let _receiver = tracker.register_task(task_id.clone(), total_bytes).await.unwrap();

        // 更新进度
        let result = tracker.update_progress(&task_id, 500, TransferStatus::Transferring).await;
        assert!(result.is_ok());

        // 获取进度
        let progress = tracker.get_progress(&task_id).await.unwrap().unwrap();
        assert_eq!(progress.transferred_bytes, 500);
        assert_eq!(progress.total_bytes, total_bytes);
        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.status, TransferStatus::Transferring);
    }

    #[tokio::test]
    async fn test_set_error() {
        let tracker = ProgressTracker::new();
        let task_id = "test-task-003".to_string();
        let total_bytes = 1000;

        let mut receiver = tracker.register_task(task_id.clone(), total_bytes).await.unwrap();

        // 设置错误
        let error_message = "传输失败".to_string();
        tracker.set_error(&task_id, error_message.clone()).await;

        // 验证错误信息
        let progress = tracker.get_progress(&task_id).await.unwrap().unwrap();
        assert_eq!(progress.error, Some(error_message));

        // 验证接收到错误通知
        let notification = receiver.recv().await.unwrap();
        assert_eq!(notification.error, Some(error_message));
    }

    #[tokio::test]
    async fn test_get_all_progress() {
        let tracker = ProgressTracker::new();

        // 注册多个任务
        let task1 = "task-001".to_string();
        let task2 = "task-002".to_string();

        tracker.register_task(task1.clone(), 1000).await.unwrap();
        tracker.register_task(task2.clone(), 2000).await.unwrap();

        // 更新进度
        tracker.update_progress(&task1, 500, TransferStatus::Transferring).await.unwrap();
        tracker.update_progress(&task2, 1000, TransferStatus::Transferring).await.unwrap();

        // 获取所有进度
        let all_progress = tracker.get_all_progress().await;
        assert_eq!(all_progress.len(), 2);

        // 验证任务按ID排序
        assert_eq!(all_progress[0].task_id, task1);
        assert_eq!(all_progress[1].task_id, task2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_data() {
        let tracker = ProgressTracker::new();
        let task_id = "expired-task".to_string();

        tracker.register_task(task_id.clone(), 1000).await.unwrap();

        // 手动创建过期数据（通过修改内部状态）
        if let Some(progress_state) = tracker.progress_data.get(&task_id) {
            *progress_state.last_update.write().await = SystemTime::now() - Duration::from_secs(7200); // 2小时前
        }

        // 清理过期数据
        let cleaned_count = tracker.cleanup_expired_data().await;
        assert_eq!(cleaned_count, 1);

        // 验证数据已清理
        assert_eq!(tracker.progress_data.len(), 0);
    }

    #[tokio::test]
    async fn test_performance_stats() {
        let tracker = ProgressTracker::new();

        // 注册任务并更新进度
        let task_id = "perf-test".to_string();
        tracker.register_task(task_id.clone(), 1000).await.unwrap();
        tracker.update_progress(&task_id, 500, TransferStatus::Transferring).await.unwrap();

        // 获取性能统计
        let stats = tracker.get_performance_stats().await;
        assert_eq!(stats.total_tasks, 1);
        assert!(stats.total_bytes_transferred > 0);
    }
}