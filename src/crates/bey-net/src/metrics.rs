//! # BEY 性能监控和指标收集
//!
//! 提供全面的性能监控和指标收集功能。
//!
//! ## 核心功能
//!
//! - **吞吐量统计**: 发送/接收速率
//! - **延迟监控**: RTT和处理延迟
//! - **错误统计**: 各类错误计数
//! - **资源使用**: 内存和连接数

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::info;

/// 性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// 总发送字节数
    pub bytes_sent: u64,
    /// 总接收字节数
    pub bytes_received: u64,
    /// 发送令牌数
    pub tokens_sent: u64,
    /// 接收令牌数
    pub tokens_received: u64,
    /// 发送速率（字节/秒）
    pub send_rate: f64,
    /// 接收速率（字节/秒）
    pub receive_rate: f64,
    /// 平均RTT（毫秒）
    pub avg_rtt_ms: f64,
    /// 最小RTT（毫秒）
    pub min_rtt_ms: u64,
    /// 最大RTT（毫秒）
    pub max_rtt_ms: u64,
    /// 错误计数
    pub error_count: u64,
    /// 重传次数
    pub retransmit_count: u64,
    /// 超时次数
    pub timeout_count: u64,
    /// 活跃连接数
    pub active_connections: usize,
    /// 活跃流数
    pub active_streams: usize,
    /// 队列大小
    pub queue_size: usize,
    /// 开始时间
    pub start_time: SystemTime,
    /// 运行时间（秒）
    pub uptime_secs: u64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            bytes_sent: 0,
            bytes_received: 0,
            tokens_sent: 0,
            tokens_received: 0,
            send_rate: 0.0,
            receive_rate: 0.0,
            avg_rtt_ms: 0.0,
            min_rtt_ms: u64::MAX,
            max_rtt_ms: 0,
            error_count: 0,
            retransmit_count: 0,
            timeout_count: 0,
            active_connections: 0,
            active_streams: 0,
            queue_size: 0,
            start_time: SystemTime::now(),
            uptime_secs: 0,
        }
    }
}

/// 延迟直方图
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    /// 延迟桶（毫秒）: <1, <10, <50, <100, <500, <1000, >=1000
    buckets: [u64; 7],
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            buckets: [0; 7],
        }
    }

    pub fn record(&mut self, latency_ms: u64) {
        let bucket_index = match latency_ms {
            0..=1 => 0,
            2..=10 => 1,
            11..=50 => 2,
            51..=100 => 3,
            101..=500 => 4,
            501..=1000 => 5,
            _ => 6,
        };
        self.buckets[bucket_index] += 1;
    }

    pub fn get_percentile(&self, percentile: f64) -> u64 {
        let total: u64 = self.buckets.iter().sum();
        if total == 0 {
            return 0;
        }

        let target = (total as f64 * percentile) as u64;
        let mut cumulative = 0u64;

        for (i, &count) in self.buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return match i {
                    0 => 1,
                    1 => 10,
                    2 => 50,
                    3 => 100,
                    4 => 500,
                    5 => 1000,
                    _ => 2000,
                };
            }
        }

        2000
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// 错误统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorStats {
    /// 按错误码统计
    pub by_code: HashMap<u32, u64>,
    /// 按错误分类统计
    pub by_category: HashMap<String, u64>,
}

/// 指标收集器
pub struct MetricsCollector {
    /// 当前指标
    metrics: Arc<RwLock<Metrics>>,
    /// 延迟直方图
    latency_histogram: Arc<RwLock<LatencyHistogram>>,
    /// 错误统计
    error_stats: Arc<RwLock<ErrorStats>>,
    /// 最后更新时间
    last_update: Arc<RwLock<SystemTime>>,
}

impl MetricsCollector {
    /// 创建指标收集器
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Metrics::default())),
            latency_histogram: Arc::new(RwLock::new(LatencyHistogram::new())),
            error_stats: Arc::new(RwLock::new(ErrorStats::default())),
            last_update: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// 记录发送
    pub async fn record_send(&self, bytes: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.bytes_sent += bytes as u64;
        metrics.tokens_sent += 1;
    }

    /// 记录接收
    pub async fn record_receive(&self, bytes: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.bytes_received += bytes as u64;
        metrics.tokens_received += 1;
    }

    /// 记录RTT
    pub async fn record_rtt(&self, rtt: Duration) {
        let rtt_ms = rtt.as_millis() as u64;
        let mut metrics = self.metrics.write().await;
        
        // 更新平均RTT（使用指数移动平均）
        if metrics.avg_rtt_ms == 0.0 {
            metrics.avg_rtt_ms = rtt_ms as f64;
        } else {
            metrics.avg_rtt_ms = 0.9 * metrics.avg_rtt_ms + 0.1 * rtt_ms as f64;
        }

        metrics.min_rtt_ms = metrics.min_rtt_ms.min(rtt_ms);
        metrics.max_rtt_ms = metrics.max_rtt_ms.max(rtt_ms);

        // 记录到直方图
        let mut histogram = self.latency_histogram.write().await;
        histogram.record(rtt_ms);
    }

    /// 记录错误
    pub async fn record_error(&self, error_code: u32, category: String) {
        let mut metrics = self.metrics.write().await;
        metrics.error_count += 1;

        let mut error_stats = self.error_stats.write().await;
        *error_stats.by_code.entry(error_code).or_insert(0) += 1;
        *error_stats.by_category.entry(category).or_insert(0) += 1;
    }

    /// 记录重传
    pub async fn record_retransmit(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.retransmit_count += 1;
    }

    /// 记录超时
    pub async fn record_timeout(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.timeout_count += 1;
    }

    /// 更新连接数
    pub async fn update_connections(&self, count: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.active_connections = count;
    }

    /// 更新流数
    pub async fn update_streams(&self, count: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.active_streams = count;
    }

    /// 更新队列大小
    pub async fn update_queue_size(&self, size: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.queue_size = size;
    }

    /// 更新速率
    pub async fn update_rates(&self) {
        let now = SystemTime::now();
        let mut last_update = self.last_update.write().await;
        
        if let Ok(elapsed) = now.duration_since(*last_update) {
            if elapsed.as_secs() > 0 {
                let mut metrics = self.metrics.write().await;
                let elapsed_secs = elapsed.as_secs_f64();
                
                metrics.send_rate = metrics.bytes_sent as f64 / elapsed_secs;
                metrics.receive_rate = metrics.bytes_received as f64 / elapsed_secs;
                
                if let Ok(uptime) = now.duration_since(metrics.start_time) {
                    metrics.uptime_secs = uptime.as_secs();
                }
                
                *last_update = now;
            }
        }
    }

    /// 获取当前指标
    pub async fn get_metrics(&self) -> Metrics {
        self.update_rates().await;
        self.metrics.read().await.clone()
    }

    /// 获取错误统计
    pub async fn get_error_stats(&self) -> ErrorStats {
        self.error_stats.read().await.clone()
    }

    /// 获取延迟百分位
    pub async fn get_latency_percentiles(&self) -> HashMap<String, u64> {
        let histogram = self.latency_histogram.read().await;
        let mut percentiles = HashMap::new();
        
        percentiles.insert("p50".to_string(), histogram.get_percentile(0.50));
        percentiles.insert("p90".to_string(), histogram.get_percentile(0.90));
        percentiles.insert("p95".to_string(), histogram.get_percentile(0.95));
        percentiles.insert("p99".to_string(), histogram.get_percentile(0.99));
        
        percentiles
    }

    /// 重置指标
    pub async fn reset(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = Metrics::default();
        
        let mut histogram = self.latency_histogram.write().await;
        *histogram = LatencyHistogram::new();
        
        let mut error_stats = self.error_stats.write().await;
        *error_stats = ErrorStats::default();
        
        info!("指标已重置");
    }

    /// 打印摘要
    pub async fn print_summary(&self) {
        let metrics = self.get_metrics().await;
        let percentiles = self.get_latency_percentiles().await;
        
        info!("=== 性能指标摘要 ===");
        info!("运行时间: {}秒", metrics.uptime_secs);
        info!("发送: {} 字节 ({} 令牌), 速率: {:.2} MB/s", 
            metrics.bytes_sent, metrics.tokens_sent, 
            metrics.send_rate / 1_048_576.0);
        info!("接收: {} 字节 ({} 令牌), 速率: {:.2} MB/s", 
            metrics.bytes_received, metrics.tokens_received, 
            metrics.receive_rate / 1_048_576.0);
        info!("RTT: 平均 {:.2}ms, 最小 {}ms, 最大 {}ms", 
            metrics.avg_rtt_ms, metrics.min_rtt_ms, metrics.max_rtt_ms);
        info!("延迟百分位: p50={}ms, p90={}ms, p95={}ms, p99={}ms",
            percentiles.get("p50").unwrap_or(&0),
            percentiles.get("p90").unwrap_or(&0),
            percentiles.get("p95").unwrap_or(&0),
            percentiles.get("p99").unwrap_or(&0));
        info!("错误: {}, 重传: {}, 超时: {}", 
            metrics.error_count, metrics.retransmit_count, metrics.timeout_count);
        info!("活跃连接: {}, 活跃流: {}, 队列: {}", 
            metrics.active_connections, metrics.active_streams, metrics.queue_size);
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        
        collector.record_send(1000).await;
        collector.record_receive(500).await;
        collector.record_rtt(Duration::from_millis(50)).await;
        
        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.bytes_sent, 1000);
        assert_eq!(metrics.bytes_received, 500);
        assert_eq!(metrics.tokens_sent, 1);
        assert_eq!(metrics.tokens_received, 1);
    }

    #[test]
    fn test_latency_histogram() {
        let mut histogram = LatencyHistogram::new();
        
        histogram.record(5);
        histogram.record(50);
        histogram.record(150);
        histogram.record(2000);
        
        let p50 = histogram.get_percentile(0.50);
        assert!(p50 > 0);
    }
}
