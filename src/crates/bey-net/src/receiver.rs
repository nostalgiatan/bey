//! # BEY 元接收器系统
//!
//! 提供灵活的消息接收和处理机制，支持多种接收模式和过滤策略。
//! 元接收器是网络层和应用层之间的桥梁。
//!
//! ## 核心概念
//!
//! - **元接收器(MetaReceiver)**: 抽象的消息接收器，定义接收行为
//! - **接收器过滤器(ReceiverFilter)**: 过滤接收的令牌
//! - **接收器缓冲区(ReceiverBuffer)**: 缓存接收的令牌
//! - **接收器策略(ReceiverStrategy)**: 定义接收和处理策略

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, info};

use crate::{NetResult, token::{Token, TokenType, TokenPriority}};

/// 接收器模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverMode {
    /// 阻塞模式：等待直到有令牌可用
    Blocking,
    /// 非阻塞模式：立即返回，如果没有令牌则返回None
    NonBlocking,
    /// 超时模式：等待指定时间，超时返回None
    Timeout(std::time::Duration),
}

/// 接收器过滤器特征
///
/// 实现此特征以过滤接收的令牌
#[async_trait]
pub trait ReceiverFilter: Send + Sync {
    /// 检查令牌是否应该被接收
    ///
    /// # 参数
    ///
    /// * `token` - 要检查的令牌
    ///
    /// # 返回值
    ///
    /// 如果令牌应该被接收返回true，否则返回false
    async fn should_receive(&self, token: &Token) -> bool;
}

/// 类型过滤器
///
/// 只接收指定类型的令牌
pub struct TypeFilter {
    /// 允许的令牌类型
    allowed_types: Vec<TokenType>,
}

impl TypeFilter {
    /// 创建新的类型过滤器
    ///
    /// # 参数
    ///
    /// * `allowed_types` - 允许的令牌类型列表
    pub fn new(allowed_types: Vec<TokenType>) -> Self {
        Self { allowed_types }
    }
}

#[async_trait]
impl ReceiverFilter for TypeFilter {
    async fn should_receive(&self, token: &Token) -> bool {
        self.allowed_types.contains(&token.meta.token_type)
    }
}

/// 优先级过滤器
///
/// 只接收指定优先级以上的令牌
pub struct PriorityFilter {
    /// 最小优先级
    min_priority: TokenPriority,
}

impl PriorityFilter {
    /// 创建新的优先级过滤器
    ///
    /// # 参数
    ///
    /// * `min_priority` - 最小优先级
    pub fn new(min_priority: TokenPriority) -> Self {
        Self { min_priority }
    }
}

#[async_trait]
impl ReceiverFilter for PriorityFilter {
    async fn should_receive(&self, token: &Token) -> bool {
        token.meta.priority >= self.min_priority
    }
}

/// 元接收器特征
///
/// 定义接收器的基本行为，所有具体的接收器都需要实现此特征
#[async_trait]
pub trait MetaReceiver: Send + Sync {
    /// 接收令牌
    ///
    /// # 参数
    ///
    /// * `mode` - 接收模式
    ///
    /// # 返回值
    ///
    /// 返回接收到的令牌或错误
    async fn receive(&self, mode: ReceiverMode) -> NetResult<Option<Token>>;

    /// 批量接收令牌
    ///
    /// # 参数
    ///
    /// * `max_count` - 最大接收数量
    /// * `mode` - 接收模式
    ///
    /// # 返回值
    ///
    /// 返回接收到的令牌列表
    async fn receive_batch(&self, max_count: usize, mode: ReceiverMode) -> NetResult<Vec<Token>>;

    /// 查看但不移除下一个令牌
    ///
    /// # 返回值
    ///
    /// 返回下一个令牌（如果有）
    async fn peek(&self) -> NetResult<Option<Token>>;

    /// 获取缓冲区中的令牌数量
    ///
    /// # 返回值
    ///
    /// 返回令牌数量
    async fn pending_count(&self) -> usize;

    /// 清空接收器缓冲区
    async fn clear(&self) -> NetResult<()>;
}

/// 缓冲接收器
///
/// 带缓冲区的令牌接收器实现
pub struct BufferedReceiver {
    /// 令牌缓冲区
    buffer: Arc<RwLock<VecDeque<Token>>>,
    /// 缓冲区大小限制
    buffer_size: usize,
    /// 接收通道
    rx: Arc<RwLock<mpsc::UnboundedReceiver<Token>>>,
    /// 过滤器链
    filters: Vec<Arc<dyn ReceiverFilter>>,
}

impl BufferedReceiver {
    /// 创建新的缓冲接收器
    ///
    /// # 参数
    ///
    /// * `buffer_size` - 缓冲区大小
    /// * `rx` - 接收通道
    ///
    /// # 返回值
    ///
    /// 返回新的缓冲接收器
    pub fn new(buffer_size: usize, rx: mpsc::UnboundedReceiver<Token>) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(buffer_size))),
            buffer_size,
            rx: Arc::new(RwLock::new(rx)),
            filters: Vec::new(),
        }
    }

    /// 添加过滤器
    ///
    /// # 参数
    ///
    /// * `filter` - 过滤器
    pub fn add_filter(&mut self, filter: Arc<dyn ReceiverFilter>) {
        self.filters.push(filter);
    }

    /// 从通道填充缓冲区
    async fn fill_buffer(&self) -> NetResult<()> {
        let mut rx = self.rx.write().await;
        let mut buffer = self.buffer.write().await;

        // 尽可能多地从通道读取令牌到缓冲区
        while buffer.len() < self.buffer_size {
            match rx.try_recv() {
                Ok(token) => {
                    // 应用过滤器
                    let should_receive = self.apply_filters(&token).await;
                    if should_receive {
                        buffer.push_back(token);
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(ErrorInfo::new(4201, "接收通道已关闭".to_string())
                        .with_category(ErrorCategory::Network)
                        .with_severity(ErrorSeverity::Error));
                }
            }
        }

        Ok(())
    }

    /// 应用所有过滤器
    async fn apply_filters(&self, token: &Token) -> bool {
        for filter in &self.filters {
            if !filter.should_receive(token).await {
                return false;
            }
        }
        true
    }
}

#[async_trait]
impl MetaReceiver for BufferedReceiver {
    async fn receive(&self, mode: ReceiverMode) -> NetResult<Option<Token>> {
        // 先尝试从缓冲区获取
        {
            let mut buffer = self.buffer.write().await;
            if let Some(token) = buffer.pop_front() {
                debug!("从缓冲区接收令牌: {}", token.meta.id);
                return Ok(Some(token));
            }
        }

        // 缓冲区为空，尝试从通道获取
        match mode {
            ReceiverMode::NonBlocking => {
                self.fill_buffer().await?;
                let mut buffer = self.buffer.write().await;
                Ok(buffer.pop_front())
            }
            ReceiverMode::Blocking => {
                // 阻塞等待
                loop {
                    self.fill_buffer().await?;
                    {
                        let mut buffer = self.buffer.write().await;
                        if let Some(token) = buffer.pop_front() {
                            return Ok(Some(token));
                        }
                    }
                    // 短暂休眠后重试
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
            ReceiverMode::Timeout(duration) => {
                // 带超时的等待
                let deadline = tokio::time::Instant::now() + duration;
                loop {
                    self.fill_buffer().await?;
                    {
                        let mut buffer = self.buffer.write().await;
                        if let Some(token) = buffer.pop_front() {
                            return Ok(Some(token));
                        }
                    }

                    if tokio::time::Instant::now() >= deadline {
                        return Ok(None);
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }
    }

    async fn receive_batch(&self, max_count: usize, mode: ReceiverMode) -> NetResult<Vec<Token>> {
        let mut tokens = Vec::with_capacity(max_count);

        // 先从缓冲区获取
        {
            let mut buffer = self.buffer.write().await;
            while tokens.len() < max_count {
                if let Some(token) = buffer.pop_front() {
                    tokens.push(token);
                } else {
                    break;
                }
            }
        }

        // 如果还需要更多令牌
        if tokens.len() < max_count {
            self.fill_buffer().await?;
            let mut buffer = self.buffer.write().await;
            while tokens.len() < max_count {
                if let Some(token) = buffer.pop_front() {
                    tokens.push(token);
                } else {
                    break;
                }
            }
        }

        debug!("批量接收 {} 个令牌", tokens.len());
        Ok(tokens)
    }

    async fn peek(&self) -> NetResult<Option<Token>> {
        // 确保缓冲区有数据
        self.fill_buffer().await?;

        let buffer = self.buffer.read().await;
        Ok(buffer.front().cloned())
    }

    async fn pending_count(&self) -> usize {
        let buffer = self.buffer.read().await;
        buffer.len()
    }

    async fn clear(&self) -> NetResult<()> {
        let mut buffer = self.buffer.write().await;
        buffer.clear();
        info!("接收器缓冲区已清空");
        Ok(())
    }
}

/// 创建接收器对
///
/// # 参数
///
/// * `buffer_size` - 缓冲区大小
///
/// # 返回值
///
/// 返回发送端和接收端
pub fn create_receiver(buffer_size: usize) -> (mpsc::UnboundedSender<Token>, BufferedReceiver) {
    let (tx, rx) = mpsc::unbounded_channel();
    let receiver = BufferedReceiver::new(buffer_size, rx);
    (tx, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{TokenMeta, Token};

    #[tokio::test]
    async fn test_type_filter() {
        let filter = TypeFilter::new(vec!["type1".to_string(), "type2".to_string()]);
        
        let meta1 = TokenMeta::new("type1".to_string(), "sender".to_string());
        let token1 = Token::new(meta1, vec![]);
        assert!(filter.should_receive(&token1).await);

        let meta2 = TokenMeta::new("type3".to_string(), "sender".to_string());
        let token2 = Token::new(meta2, vec![]);
        assert!(!filter.should_receive(&token2).await);
    }

    #[tokio::test]
    async fn test_priority_filter() {
        let filter = PriorityFilter::new(TokenPriority::High);
        
        let meta1 = TokenMeta::new("type1".to_string(), "sender".to_string())
            .with_priority(TokenPriority::Critical);
        let token1 = Token::new(meta1, vec![]);
        assert!(filter.should_receive(&token1).await);

        let meta2 = TokenMeta::new("type2".to_string(), "sender".to_string())
            .with_priority(TokenPriority::Normal);
        let token2 = Token::new(meta2, vec![]);
        assert!(!filter.should_receive(&token2).await);
    }

    #[tokio::test]
    async fn test_buffered_receiver() {
        let (tx, receiver) = create_receiver(10);

        // 发送令牌
        let meta = TokenMeta::new("test".to_string(), "sender".to_string());
        let token = Token::new(meta, vec![1, 2, 3]);
        tx.send(token.clone()).unwrap();

        // 接收令牌
        let received = receiver.receive(ReceiverMode::NonBlocking).await.unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().meta.token_type, "test");
    }

    #[tokio::test]
    async fn test_batch_receive() {
        let (tx, receiver) = create_receiver(10);

        // 发送多个令牌
        for i in 0..5 {
            let meta = TokenMeta::new("test".to_string(), format!("sender_{}", i));
            let token = Token::new(meta, vec![i as u8]);
            tx.send(token).unwrap();
        }

        // 批量接收
        let tokens = receiver.receive_batch(3, ReceiverMode::NonBlocking).await.unwrap();
        assert_eq!(tokens.len(), 3);
    }

    #[tokio::test]
    async fn test_peek() {
        let (tx, receiver) = create_receiver(10);

        // 发送令牌
        let meta = TokenMeta::new("test".to_string(), "sender".to_string());
        let token = Token::new(meta, vec![1, 2, 3]);
        tx.send(token.clone()).unwrap();

        // Peek不应该移除令牌
        let peeked = receiver.peek().await.unwrap();
        assert!(peeked.is_some());

        // 再次接收应该还能获取到同一个令牌
        let received = receiver.receive(ReceiverMode::NonBlocking).await.unwrap();
        assert!(received.is_some());
    }
}
