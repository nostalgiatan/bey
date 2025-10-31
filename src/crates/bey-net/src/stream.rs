//! # BEY 流式传输
//!
//! 支持大文件的流式传输，使用分块传输和流水线技术。
//!
//! ## 核心功能
//!
//! - **分块传输**: 将大文件分割成多个块进行传输
//! - **流式标志**: 标识流的开始、数据和结束
//! - **流水线**: 多个块并行传输提高吞吐量
//! - **断点续传**: 支持传输中断后继续

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::{NetResult, token::{Token, TokenMeta}};

/// 流标志
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFlag {
    /// 流开始
    Start,
    /// 数据块
    Data,
    /// 流结束
    End,
    /// 流中止
    Abort,
}

/// 流元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMeta {
    /// 流ID
    pub stream_id: String,
    /// 总大小（字节）
    pub total_size: u64,
    /// 块大小（字节）
    pub chunk_size: usize,
    /// 总块数
    pub total_chunks: usize,
    /// 流类型
    pub stream_type: String,
    /// 自定义元数据
    pub metadata: HashMap<String, String>,
}

/// 流块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// 流ID
    pub stream_id: String,
    /// 块序号（从0开始）
    pub chunk_index: usize,
    /// 块数据
    pub data: Vec<u8>,
    /// 流标志
    pub flag: StreamFlag,
    /// 流元数据（仅在Start时存在）
    pub meta: Option<StreamMeta>,
}

impl StreamChunk {
    /// 创建流开始块
    pub fn start(stream_id: String, meta: StreamMeta) -> Self {
        Self {
            stream_id,
            chunk_index: 0,
            data: Vec::new(),
            flag: StreamFlag::Start,
            meta: Some(meta),
        }
    }

    /// 创建数据块
    pub fn data(stream_id: String, chunk_index: usize, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            chunk_index,
            data,
            flag: StreamFlag::Data,
            meta: None,
        }
    }

    /// 创建流结束块
    pub fn end(stream_id: String, chunk_index: usize) -> Self {
        Self {
            stream_id,
            chunk_index,
            data: Vec::new(),
            flag: StreamFlag::End,
            meta: None,
        }
    }

    /// 转换为令牌
    pub fn to_token(&self, sender_id: String) -> Token {
        let mut meta = TokenMeta::new("stream_chunk".to_string(), sender_id);
        meta.attributes.insert("stream_id".to_string(), self.stream_id.clone());
        meta.attributes.insert("chunk_index".to_string(), self.chunk_index.to_string());
        meta.attributes.insert("flag".to_string(), format!("{:?}", self.flag));

        let data = serde_json::to_vec(self).unwrap_or_default();
        Token::new(meta, data)
    }

    /// 从令牌解析
    pub fn from_token(token: &Token) -> NetResult<Self> {
        serde_json::from_slice(&token.payload).map_err(|e| {
            ErrorInfo::new(4401, format!("解析流块失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
        })
    }
}

/// 流状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamState {
    /// 初始化
    Initialized,
    /// 传输中
    Transferring,
    /// 已完成
    Completed,
    /// 已中止
    Aborted,
    /// 错误
    Error,
}

/// 流会话
pub struct StreamSession {
    /// 流ID
    stream_id: String,
    /// 流元数据
    meta: StreamMeta,
    /// 当前状态
    state: StreamState,
    /// 已接收的块
    received_chunks: HashMap<usize, Vec<u8>>,
    /// 接收的块数
    received_count: usize,
    /// 创建时间
    created_at: std::time::SystemTime,
    /// 最后活跃时间
    last_active: std::time::SystemTime,
}

impl StreamSession {
    /// 创建新会话
    pub fn new(stream_id: String, meta: StreamMeta) -> Self {
        Self {
            stream_id,
            meta,
            state: StreamState::Initialized,
            received_chunks: HashMap::new(),
            received_count: 0,
            created_at: std::time::SystemTime::now(),
            last_active: std::time::SystemTime::now(),
        }
    }

    /// 添加块
    pub fn add_chunk(&mut self, chunk: StreamChunk) -> NetResult<()> {
        match chunk.flag {
            StreamFlag::Start => {
                self.state = StreamState::Transferring;
            }
            StreamFlag::Data => {
                if self.state != StreamState::Transferring {
                    return Err(ErrorInfo::new(4402, "流未处于传输状态".to_string())
                        .with_category(ErrorCategory::System)
                        .with_severity(ErrorSeverity::Warning));
                }
                self.received_chunks.insert(chunk.chunk_index, chunk.data);
                self.received_count += 1;
            }
            StreamFlag::End => {
                self.state = StreamState::Completed;
            }
            StreamFlag::Abort => {
                self.state = StreamState::Aborted;
            }
        }

        self.last_active = std::time::SystemTime::now();
        Ok(())
    }

    /// 检查是否完成
    pub fn is_complete(&self) -> bool {
        self.state == StreamState::Completed && 
        self.received_count >= self.meta.total_chunks
    }

    /// 获取完整数据
    pub fn get_data(&self) -> NetResult<Vec<u8>> {
        if !self.is_complete() {
            return Err(ErrorInfo::new(4403, "流未完成".to_string())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Warning));
        }

        let mut data = Vec::with_capacity(self.meta.total_size as usize);
        for i in 0..self.meta.total_chunks {
            if let Some(chunk_data) = self.received_chunks.get(&i) {
                data.extend_from_slice(chunk_data);
            } else {
                return Err(ErrorInfo::new(4404, format!("缺少块 {}", i))
                    .with_category(ErrorCategory::System)
                    .with_severity(ErrorSeverity::Error));
            }
        }

        Ok(data)
    }
}

/// 流管理器
pub struct StreamManager {
    /// 活跃的流会话
    sessions: Arc<RwLock<HashMap<String, StreamSession>>>,
    /// 默认块大小
    default_chunk_size: usize,
}

impl StreamManager {
    /// 创建流管理器
    pub fn new(default_chunk_size: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            default_chunk_size,
        }
    }

    /// 创建发送流
    pub async fn create_send_stream(
        &self,
        stream_id: String,
        data: Vec<u8>,
        stream_type: String,
    ) -> NetResult<Vec<StreamChunk>> {
        let total_size = data.len() as u64;
        let chunk_size = self.default_chunk_size;
        let total_chunks = (total_size as usize + chunk_size - 1) / chunk_size;

        let meta = StreamMeta {
            stream_id: stream_id.clone(),
            total_size,
            chunk_size,
            total_chunks,
            stream_type,
            metadata: HashMap::new(),
        };

        let mut chunks = Vec::with_capacity(total_chunks + 2);
        
        // 开始块
        chunks.push(StreamChunk::start(stream_id.clone(), meta));

        // 数据块
        for (i, chunk_data) in data.chunks(chunk_size).enumerate() {
            chunks.push(StreamChunk::data(stream_id.clone(), i, chunk_data.to_vec()));
        }

        // 结束块
        chunks.push(StreamChunk::end(stream_id.clone(), total_chunks));

        info!("创建发送流: {} ({}字节, {}块)", stream_id, total_size, total_chunks);
        Ok(chunks)
    }

    /// 处理接收块
    pub async fn handle_chunk(&self, chunk: StreamChunk) -> NetResult<Option<Vec<u8>>> {
        let stream_id = chunk.stream_id.clone();
        
        let mut sessions = self.sessions.write().await;
        
        match chunk.flag {
            StreamFlag::Start => {
                if let Some(meta) = &chunk.meta {
                    let session = StreamSession::new(stream_id.clone(), meta.clone());
                    sessions.insert(stream_id.clone(), session);
                    debug!("创建接收流会话: {}", stream_id);
                }
            }
            StreamFlag::Data | StreamFlag::End | StreamFlag::Abort => {
                if let Some(session) = sessions.get_mut(&stream_id) {
                    session.add_chunk(chunk)?;
                    
                    if session.is_complete() {
                        let data = session.get_data()?;
                        sessions.remove(&stream_id);
                        info!("流完成: {} ({}字节)", stream_id, data.len());
                        return Ok(Some(data));
                    }
                }
            }
        }

        Ok(None)
    }

    /// 清理超时会话
    pub async fn cleanup_timeout_sessions(&self, timeout_secs: u64) -> usize {
        let mut sessions = self.sessions.write().await;
        let now = std::time::SystemTime::now();
        
        let initial_count = sessions.len();
        sessions.retain(|id, session| {
            if let Ok(elapsed) = now.duration_since(session.last_active) {
                if elapsed.as_secs() > timeout_secs {
                    warn!("清理超时流会话: {}", id);
                    return false;
                }
            }
            true
        });

        initial_count - sessions.len()
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new(65536) // 64KB 默认块大小
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_chunk_creation() {
        let meta = StreamMeta {
            stream_id: "test".to_string(),
            total_size: 1000,
            chunk_size: 100,
            total_chunks: 10,
            stream_type: "file".to_string(),
            metadata: HashMap::new(),
        };

        let start_chunk = StreamChunk::start("test".to_string(), meta);
        assert_eq!(start_chunk.flag, StreamFlag::Start);
        assert!(start_chunk.meta.is_some());
    }

    #[tokio::test]
    async fn test_stream_manager() {
        let manager = StreamManager::new(100);
        let data = vec![1u8; 250];
        
        let chunks = manager.create_send_stream(
            "test-stream".to_string(),
            data.clone(),
            "test".to_string()
        ).await.unwrap();

        // 应该有1个开始块 + 3个数据块 + 1个结束块
        assert_eq!(chunks.len(), 5);
        assert_eq!(chunks[0].flag, StreamFlag::Start);
        assert_eq!(chunks[chunks.len() - 1].flag, StreamFlag::End);
    }
}
