//! # BEY 流量控制和拥塞控制
//!
//! 实现TCP友好的流量控制和拥塞控制机制。
//!
//! ## 核心功能
//!
//! - **滑动窗口**: 控制发送速率
//! - **拥塞控制**: 检测和避免网络拥塞
//! - **速率限制**: 限制发送速率
//! - **反压机制**: 接收端反压信号

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::NetResult;

/// 拥塞状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionState {
    /// 慢启动
    SlowStart,
    /// 拥塞避免
    CongestionAvoidance,
    /// 快速恢复
    FastRecovery,
}

/// 流量控制器
pub struct FlowController {
    /// 发送窗口大小（字节）
    send_window: Arc<RwLock<usize>>,
    /// 接收窗口大小（字节）
    recv_window: Arc<RwLock<usize>>,
    /// 拥塞窗口大小（字节）
    congestion_window: Arc<RwLock<usize>>,
    /// 慢启动阈值
    ssthresh: Arc<RwLock<usize>>,
    /// 拥塞状态
    congestion_state: Arc<RwLock<CongestionState>>,
    /// 最大窗口大小
    max_window: usize,
    /// 最小窗口大小
    min_window: usize,
    /// 已发送未确认字节数
    bytes_in_flight: Arc<RwLock<usize>>,
    /// RTT估计值（毫秒）
    rtt_ms: Arc<RwLock<u64>>,
    /// RTT变化
    rtt_var_ms: Arc<RwLock<u64>>,
}

impl FlowController {
    /// 创建流量控制器
    pub fn new(initial_window: usize, max_window: usize) -> Self {
        Self {
            send_window: Arc::new(RwLock::new(initial_window)),
            recv_window: Arc::new(RwLock::new(initial_window)),
            congestion_window: Arc::new(RwLock::new(initial_window)),
            ssthresh: Arc::new(RwLock::new(max_window / 2)),
            congestion_state: Arc::new(RwLock::new(CongestionState::SlowStart)),
            max_window,
            min_window: 1024, // 1KB
            bytes_in_flight: Arc::new(RwLock::new(0)),
            rtt_ms: Arc::new(RwLock::new(100)), // 初始RTT 100ms
            rtt_var_ms: Arc::new(RwLock::new(50)),
        }
    }

    /// 检查是否可以发送
    pub async fn can_send(&self, size: usize) -> bool {
        let send_window = *self.send_window.read().await;
        let recv_window = *self.recv_window.read().await;
        let congestion_window = *self.congestion_window.read().await;
        let bytes_in_flight = *self.bytes_in_flight.read().await;

        let effective_window = send_window.min(recv_window).min(congestion_window);
        bytes_in_flight + size <= effective_window
    }

    /// 记录发送
    pub async fn on_send(&self, size: usize) -> NetResult<()> {
        let mut bytes_in_flight = self.bytes_in_flight.write().await;
        *bytes_in_flight += size;
        debug!("发送 {} 字节，飞行中: {}", size, *bytes_in_flight);
        Ok(())
    }

    /// 记录确认
    pub async fn on_ack(&self, size: usize, rtt: Duration) -> NetResult<()> {
        // 更新飞行字节数
        {
            let mut bytes_in_flight = self.bytes_in_flight.write().await;
            *bytes_in_flight = bytes_in_flight.saturating_sub(size);
        }

        // 更新RTT
        self.update_rtt(rtt).await;

        // 更新拥塞窗口
        let state = *self.congestion_state.read().await;
        match state {
            CongestionState::SlowStart => {
                let mut cwnd = self.congestion_window.write().await;
                *cwnd += size; // 每个ACK增加一个MSS
                
                let ssthresh = *self.ssthresh.read().await;
                if *cwnd >= ssthresh {
                    *self.congestion_state.write().await = CongestionState::CongestionAvoidance;
                    info!("进入拥塞避免阶段，cwnd: {}", *cwnd);
                }
            }
            CongestionState::CongestionAvoidance => {
                let mut cwnd = self.congestion_window.write().await;
                // 每个RTT增加一个MSS
                *cwnd += size * size / *cwnd;
            }
            CongestionState::FastRecovery => {
                // 快速恢复期间保持窗口不变
            }
        }

        // 限制窗口大小
        let mut cwnd = self.congestion_window.write().await;
        *cwnd = (*cwnd).min(self.max_window).max(self.min_window);

        Ok(())
    }

    /// 处理丢包
    pub async fn on_loss(&self) -> NetResult<()> {
        warn!("检测到丢包，触发拥塞控制");

        let mut cwnd = self.congestion_window.write().await;
        let mut ssthresh = self.ssthresh.write().await;
        
        // 慢启动阈值设置为当前窗口的一半
        *ssthresh = (*cwnd / 2).max(self.min_window);
        
        // 窗口减半
        *cwnd = *ssthresh;
        
        *self.congestion_state.write().await = CongestionState::FastRecovery;
        
        info!("拥塞控制：cwnd: {}, ssthresh: {}", *cwnd, *ssthresh);
        Ok(())
    }

    /// 更新RTT
    async fn update_rtt(&self, rtt: Duration) {
        let rtt_ms = rtt.as_millis() as u64;
        let mut estimated_rtt = self.rtt_ms.write().await;
        let mut rtt_var = self.rtt_var_ms.write().await;

        // 使用指数加权移动平均
        let alpha = 0.125;
        let beta = 0.25;

        let diff = (*estimated_rtt as i64 - rtt_ms as i64).abs() as u64;
        *rtt_var = ((1.0 - beta) * *rtt_var as f64 + beta * diff as f64) as u64;
        *estimated_rtt = ((1.0 - alpha) * *estimated_rtt as f64 + alpha * rtt_ms as f64) as u64;

        debug!("RTT更新: {}ms (var: {}ms)", *estimated_rtt, *rtt_var);
    }

    /// 获取超时时间
    pub async fn get_timeout(&self) -> Duration {
        let rtt = *self.rtt_ms.read().await;
        let rtt_var = *self.rtt_var_ms.read().await;
        
        // RTO = RTT + 4 * RTTVAR
        let timeout_ms = rtt + 4 * rtt_var;
        Duration::from_millis(timeout_ms.max(1000)) // 最少1秒
    }

    /// 更新接收窗口
    pub async fn update_recv_window(&self, size: usize) {
        let mut recv_window = self.recv_window.write().await;
        *recv_window = size;
        debug!("接收窗口更新: {}", size);
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> FlowControlStats {
        FlowControlStats {
            send_window: *self.send_window.read().await,
            recv_window: *self.recv_window.read().await,
            congestion_window: *self.congestion_window.read().await,
            ssthresh: *self.ssthresh.read().await,
            bytes_in_flight: *self.bytes_in_flight.read().await,
            rtt_ms: *self.rtt_ms.read().await,
            congestion_state: *self.congestion_state.read().await,
        }
    }
}

/// 流量控制统计
#[derive(Debug, Clone)]
pub struct FlowControlStats {
    /// 发送窗口
    pub send_window: usize,
    /// 接收窗口
    pub recv_window: usize,
    /// 拥塞窗口
    pub congestion_window: usize,
    /// 慢启动阈值
    pub ssthresh: usize,
    /// 飞行中字节数
    pub bytes_in_flight: usize,
    /// RTT（毫秒）
    pub rtt_ms: u64,
    /// 拥塞状态
    pub congestion_state: CongestionState,
}

impl Default for FlowController {
    fn default() -> Self {
        Self::new(65536, 1048576) // 64KB初始，1MB最大
    }
}

/// 速率限制器
pub struct RateLimiter {
    /// 最大速率（字节/秒）
    max_rate: Arc<RwLock<u64>>,
    /// 令牌桶容量
    bucket_capacity: Arc<RwLock<u64>>,
    /// 当前令牌数
    tokens: Arc<RwLock<u64>>,
    /// 最后补充时间
    last_refill: Arc<RwLock<SystemTime>>,
}

impl RateLimiter {
    /// 创建速率限制器
    pub fn new(max_rate: u64) -> Self {
        Self {
            max_rate: Arc::new(RwLock::new(max_rate)),
            bucket_capacity: Arc::new(RwLock::new(max_rate)),
            tokens: Arc::new(RwLock::new(max_rate)),
            last_refill: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// 尝试获取令牌
    pub async fn acquire(&self, size: u64) -> NetResult<()> {
        // 补充令牌
        self.refill_tokens().await;

        let mut tokens = self.tokens.write().await;
        if *tokens >= size {
            *tokens -= size;
            Ok(())
        } else {
            Err(ErrorInfo::new(4601, "速率限制：令牌不足".to_string())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 补充令牌
    async fn refill_tokens(&self) {
        let now = SystemTime::now();
        let mut last_refill = self.last_refill.write().await;
        
        if let Ok(elapsed) = now.duration_since(*last_refill) {
            let max_rate = *self.max_rate.read().await;
            let refill_amount = (max_rate as f64 * elapsed.as_secs_f64()) as u64;
            
            if refill_amount > 0 {
                let mut tokens = self.tokens.write().await;
                let bucket_capacity = *self.bucket_capacity.read().await;
                *tokens = (*tokens + refill_amount).min(bucket_capacity);
                *last_refill = now;
            }
        }
    }

    /// 更新速率
    pub async fn set_rate(&self, new_rate: u64) {
        let mut max_rate = self.max_rate.write().await;
        let mut bucket_capacity = self.bucket_capacity.write().await;
        *max_rate = new_rate;
        *bucket_capacity = new_rate;
        info!("速率限制更新: {} 字节/秒", new_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_flow_controller() {
        let fc = FlowController::new(1000, 10000);
        
        assert!(fc.can_send(500).await);
        fc.on_send(500).await.unwrap();
        
        assert!(fc.can_send(500).await);
        fc.on_send(500).await.unwrap();
        
        // 超过窗口
        assert!(!fc.can_send(100).await);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(1000); // 1000字节/秒
        
        // 第一次应该成功
        assert!(limiter.acquire(500).await.is_ok());
        
        // 第二次应该成功
        assert!(limiter.acquire(500).await.is_ok());
        
        // 第三次应该失败（超过容量）
        assert!(limiter.acquire(100).await.is_err());
    }
}
