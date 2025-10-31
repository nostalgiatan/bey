//! # BEY 优先级队列
//!
//! 基于令牌优先级的队列实现，确保高优先级令牌优先处理。
//!
//! ## 核心功能
//!
//! - **优先级排序**: 自动按优先级排序令牌
//! - **确认机制**: 支持令牌确认和重传
//! - **超时管理**: 自动处理超时的令牌

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, Duration};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, info, warn};

use crate::{NetResult, token::Token};

/// 优先级队列条目
#[derive(Debug, Clone)]
struct PriorityQueueEntry {
    /// 令牌
    token: Token,
    /// 入队时间
    enqueued_at: SystemTime,
    /// 需要确认
    requires_ack: bool,
    /// 重试次数
    retry_count: u32,
}

impl PartialEq for PriorityQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.token.meta.priority == other.token.meta.priority
    }
}

impl Eq for PriorityQueueEntry {}

impl PartialOrd for PriorityQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // 优先级高的排在前面，时间早的排在前面
        match self.token.meta.priority.cmp(&other.token.meta.priority) {
            Ordering::Equal => other.enqueued_at.cmp(&self.enqueued_at),
            other_ord => other_ord,
        }
    }
}

/// 令牌确认状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckStatus {
    /// 等待确认
    Pending,
    /// 已确认
    Acknowledged,
    /// 超时
    Timeout,
}

/// 待确认的令牌
struct PendingAck {
    /// 令牌
    token: Token,
    /// 发送时间
    sent_at: SystemTime,
    /// 超时时间
    timeout: Duration,
    /// 重试次数
    retry_count: u32,
    /// 最大重试次数
    max_retries: u32,
}

/// 优先级队列
pub struct PriorityQueue {
    /// 内部堆
    heap: Arc<RwLock<BinaryHeap<PriorityQueueEntry>>>,
    /// 待确认的令牌
    pending_acks: Arc<RwLock<HashMap<String, PendingAck>>>,
    /// 确认通知通道
    ack_notify: mpsc::UnboundedSender<String>,
    /// 确认通知接收
    _ack_receiver: Arc<RwLock<mpsc::UnboundedReceiver<String>>>,
    /// 默认确认超时
    default_ack_timeout: Duration,
    /// 最大重试次数
    max_retries: u32,
}

impl PriorityQueue {
    /// 创建优先级队列
    pub fn new(default_ack_timeout: Duration, max_retries: u32) -> Self {
        let (ack_notify, ack_receiver) = mpsc::unbounded_channel();
        
        Self {
            heap: Arc::new(RwLock::new(BinaryHeap::new())),
            pending_acks: Arc::new(RwLock::new(HashMap::new())),
            ack_notify,
            _ack_receiver: Arc::new(RwLock::new(ack_receiver)),
            default_ack_timeout,
            max_retries,
        }
    }

    /// 入队令牌
    pub async fn enqueue(&self, token: Token) -> NetResult<()> {
        let requires_ack = token.meta.requires_ack;
        
        let entry = PriorityQueueEntry {
            token: token.clone(),
            enqueued_at: SystemTime::now(),
            requires_ack,
            retry_count: 0,
        };

        {
            let mut heap = self.heap.write().await;
            heap.push(entry);
        }

        debug!("令牌入队: {} (优先级: {:?})", token.meta.id, token.meta.priority);
        Ok(())
    }

    /// 出队令牌（获取最高优先级）
    pub async fn dequeue(&self) -> NetResult<Option<Token>> {
        let mut heap = self.heap.write().await;
        
        if let Some(entry) = heap.pop() {
            let token = entry.token.clone();
            
            // 如果需要确认，添加到待确认列表
            if entry.requires_ack {
                let pending = PendingAck {
                    token: token.clone(),
                    sent_at: SystemTime::now(),
                    timeout: self.default_ack_timeout,
                    retry_count: entry.retry_count,
                    max_retries: self.max_retries,
                };
                
                let mut pending_acks = self.pending_acks.write().await;
                pending_acks.insert(token.meta.id.clone(), pending);
            }

            debug!("令牌出队: {} (优先级: {:?})", token.meta.id, token.meta.priority);
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }

    /// 确认令牌
    pub async fn acknowledge(&self, token_id: &str) -> NetResult<()> {
        let mut pending_acks = self.pending_acks.write().await;
        
        if pending_acks.remove(token_id).is_some() {
            info!("令牌已确认: {}", token_id);
            let _ = self.ack_notify.send(token_id.to_string());
            Ok(())
        } else {
            Err(ErrorInfo::new(4501, format!("未找到待确认令牌: {}", token_id))
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Warning))
        }
    }

    /// 检查并处理超时的令牌
    pub async fn check_timeouts(&self) -> usize {
        let now = SystemTime::now();
        let mut pending_acks = self.pending_acks.write().await;
        let mut heap = self.heap.write().await;
        
        let mut timed_out = Vec::new();
        let mut to_retry = Vec::new();

        for (token_id, pending) in pending_acks.iter() {
            if let Ok(elapsed) = now.duration_since(pending.sent_at) {
                if elapsed > pending.timeout {
                    if pending.retry_count < pending.max_retries {
                        // 可以重试
                        to_retry.push(token_id.clone());
                    } else {
                        // 已达最大重试次数
                        timed_out.push(token_id.clone());
                    }
                }
            }
        }

        // 移除超时的令牌
        for token_id in &timed_out {
            if pending_acks.remove(token_id).is_some() {
                warn!("令牌超时（已达最大重试次数）: {}", token_id);
            }
        }

        // 重新入队需要重试的令牌
        for token_id in &to_retry {
            if let Some(mut pending) = pending_acks.remove(token_id) {
                pending.retry_count += 1;
                pending.sent_at = now;
                
                let entry = PriorityQueueEntry {
                    token: pending.token.clone(),
                    enqueued_at: now,
                    requires_ack: true,
                    retry_count: pending.retry_count,
                };
                
                heap.push(entry);
                info!("令牌重试: {} (第{}次)", token_id, pending.retry_count);
            }
        }

        timed_out.len() + to_retry.len()
    }

    /// 获取队列大小
    pub async fn size(&self) -> usize {
        let heap = self.heap.read().await;
        heap.len()
    }

    /// 获取待确认数量
    pub async fn pending_acks_count(&self) -> usize {
        let pending_acks = self.pending_acks.read().await;
        pending_acks.len()
    }

    /// 清空队列
    pub async fn clear(&self) {
        let mut heap = self.heap.write().await;
        let mut pending_acks = self.pending_acks.write().await;
        
        heap.clear();
        pending_acks.clear();
        
        info!("优先级队列已清空");
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new(Duration::from_secs(5), 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenMeta;

    #[tokio::test]
    async fn test_priority_queue() {
        let queue = PriorityQueue::default();

        // 添加不同优先级的令牌
        let mut token_low = Token::new(
            TokenMeta::new("test".to_string(), "sender".to_string())
                .with_priority(TokenPriority::Low),
            vec![1, 2, 3]
        );

        let mut token_high = Token::new(
            TokenMeta::new("test".to_string(), "sender".to_string())
                .with_priority(TokenPriority::High),
            vec![4, 5, 6]
        );

        queue.enqueue(token_low).await.unwrap();
        queue.enqueue(token_high).await.unwrap();

        // 高优先级应该先出队
        let first = queue.dequeue().await.unwrap().unwrap();
        assert_eq!(first.meta.priority, TokenPriority::High);

        let second = queue.dequeue().await.unwrap().unwrap();
        assert_eq!(second.meta.priority, TokenPriority::Low);
    }

    #[tokio::test]
    async fn test_acknowledge() {
        let queue = PriorityQueue::default();

        let token = Token::new(
            TokenMeta::new("test".to_string(), "sender".to_string())
                .with_ack(true),
            vec![1, 2, 3]
        );

        let token_id = token.meta.id.clone();
        queue.enqueue(token).await.unwrap();
        
        // 出队后应该在待确认列表中
        let dequeued = queue.dequeue().await.unwrap().unwrap();
        assert_eq!(queue.pending_acks_count().await, 1);

        // 确认后应该从列表中移除
        queue.acknowledge(&token_id).await.unwrap();
        assert_eq!(queue.pending_acks_count().await, 0);
    }
}
