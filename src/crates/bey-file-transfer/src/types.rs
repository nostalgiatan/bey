//! # 公共类型定义
//!
//! 定义文件传输系统中使用的公共类型，避免循环导入问题。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;
use bytes::Bytes;
use error::{ErrorInfo};

/// 文件传输结果类型
pub type TransferResult<T> = std::result::Result<T, ErrorInfo>;

/// 传输方向枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransferDirection {
    /// 上传传输 (本地 -> 远程)
    Upload,
    /// 下载传输 (远程 -> 本地)
    Download,
}

/// 传输状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransferStatus {
    /// 等待中
    Pending,
    /// 准备中
    Preparing,
    /// 传输中
    Transferring,
    /// 暂停
    Paused,
    /// 已完成
    Completed,
    /// 已取消
    Cancelled,
    /// 传输失败
    Failed,
    /// 恢复中
    Resuming,
}

impl std::fmt::Display for TransferStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            TransferStatus::Pending => "等待中",
            TransferStatus::Preparing => "准备中",
            TransferStatus::Transferring => "传输中",
            TransferStatus::Paused => "暂停",
            TransferStatus::Completed => "已完成",
            TransferStatus::Cancelled => "已取消",
            TransferStatus::Failed => "传输失败",
            TransferStatus::Resuming => "恢复中",
        };
        write!(f, "{}", status_str)
    }
}

/// 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    /// 启用加密传输
    pub enable_encryption: bool,
    /// 最大并发数
    pub max_concurrency: usize,
    /// 数据块大小（字节）
    pub chunk_size: usize,
    /// 重试次数
    pub max_retries: usize,
    /// 超时时间（秒）
    pub timeout_seconds: u64,
    /// 心跳间隔（秒）
    pub heartbeat_interval_seconds: u64,
    /// 缓冲区大小
    pub buffer_size: usize,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            enable_encryption: true,
            max_concurrency: 4,
            chunk_size: 1024 * 1024, // 1MB
            max_retries: 3,
            timeout_seconds: 300,
            heartbeat_interval_seconds: 5,
            buffer_size: 64 * 1024, // 64KB
        }
    }
}

/// 传输元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferMetadata {
    /// MIME类型
    pub mime_type: String,
    /// 文件扩展名
    pub file_extension: String,
    /// 创建时间
    pub created_at: SystemTime,
    /// 修改时间
    pub modified_at: SystemTime,
    /// 自定义属性
    pub properties: std::collections::HashMap<String, String>,
}

/// 传输任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    /// 传输任务ID
    pub task_id: String,
    /// 传输方向
    pub direction: TransferDirection,
    /// 源文件路径
    pub source_path: PathBuf,
    /// 目标文件路径
    pub target_path: PathBuf,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 已传输大小（字节）
    pub transferred_size: u64,
    /// 传输状态
    pub status: TransferStatus,
    /// 创建时间
    pub created_at: SystemTime,
    /// 更新时间
    pub updated_at: SystemTime,
    /// 完成时间（可选）
    pub completed_at: Option<SystemTime>,
    /// 文件哈希值（用于完整性校验）
    pub file_hash: Option<String>,
    /// 传输配置
    pub config: TransferConfig,
    /// 传输元数据
    pub metadata: TransferMetadata,
    /// 传输选项
    pub options: TransferOptions,
}

/// 传输进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    /// 传输任务ID
    pub task_id: String,
    /// 传输进度百分比 (0.0 - 100.0)
    pub percentage: f64,
    /// 已传输字节数
    pub transferred_bytes: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 传输速度 (字节/秒)
    pub speed: u64,
    /// 预估剩余时间（秒）
    pub eta_seconds: Option<u64>,
    /// 错误信息
    pub error: Option<String>,
    /// 更新时间
    pub updated_at: SystemTime,
}

/// 传输优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransferPriority {
    /// 低优先级
    Low = 1,
    /// 普通优先级
    Normal = 2,
    /// 高优先级
    High = 3,
    /// 紧急优先级
    Urgent = 4,
}

/// 传输选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferOptions {
    /// 传输优先级
    pub priority: TransferPriority,
    /// 用户ID
    pub user_id: String,
    /// 权限令牌
    pub permission_token: String,
    /// 自定义标签
    pub tags: Vec<String>,
    /// 自定义属性
    pub attributes: std::collections::HashMap<String, String>,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            priority: TransferPriority::Normal,
            user_id: String::new(),
            permission_token: String::new(),
            tags: Vec::new(),
            attributes: std::collections::HashMap::new(),
        }
    }
}

/// 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// 文件路径
    pub path: PathBuf,
    /// 文件大小
    pub size: u64,
    /// 修改时间
    pub modified: SystemTime,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件权限
    pub permissions: Option<u32>,
    /// 文件哈希值
    pub hash: Option<String>,
}

/// 目录条目信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    /// 条目名称
    pub name: String,
    /// 完整路径
    pub path: PathBuf,
    /// 是否为目录
    pub is_directory: bool,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间
    pub modified: SystemTime,
    /// 文件权限
    pub permissions: Option<u32>,
    /// 是否为符号链接
    pub is_symlink: bool,
    /// 符号链接目标（如果是符号链接）
    pub symlink_target: Option<PathBuf>,
}

/// 文件系统操作选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemOptions {
    /// 是否递归操作
    pub recursive: bool,
    /// 是否包含隐藏文件
    pub include_hidden: bool,
    /// 文件过滤模式
    pub filter_pattern: Option<String>,
    /// 排序方式
    pub sort_by: DirectorySortOrder,
}

/// 目录排序方式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirectorySortOrder {
    /// 按名称排序
    Name,
    /// 按大小排序
    Size,
    /// 按修改时间排序
    ModifiedTime,
    /// 按类型排序
    Type,
}

/// 数据块信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// 块索引
    pub index: usize,
    /// 块偏移量
    pub offset: u64,
    /// 块大小
    pub size: usize,
    /// 块哈希值
    pub hash: String,
    /// 传输时间戳
    pub timestamp: SystemTime,
}

/// 传输断点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferCheckpoint {
    /// 传输任务ID
    pub task_id: String,
    /// 已传输块信息
    pub transferred_chunks: Vec<ChunkInfo>,
    /// 传输配置
    pub config: TransferConfig,
    /// 创建时间
    pub created_at: SystemTime,
}

/// 存储接口
#[async_trait::async_trait]
pub trait StorageInterface: Send + Sync {
    /// 读取文件块
    async fn read_chunk(&self, path: &std::path::Path, offset: u64, size: usize) -> TransferResult<Bytes>;

    /// 写入文件块
    async fn write_chunk(&self, path: &std::path::Path, offset: u64, data: Bytes) -> TransferResult<()>;

    /// 获取文件信息
    async fn get_file_info(&self, path: &std::path::Path) -> TransferResult<FileInfo>;

    /// 创建目录
    async fn create_dir(&self, path: &std::path::Path) -> TransferResult<()>;

    /// 删除文件
    async fn delete_file(&self, path: &std::path::Path) -> TransferResult<()>;

    /// 文件是否存在
    async fn exists(&self, path: &std::path::Path) -> TransferResult<bool>;

    /// 列出目录内容
    async fn list_directory(&self, path: &std::path::Path) -> TransferResult<Vec<DirectoryEntry>>;

    /// 删除目录
    async fn remove_dir(&self, path: &std::path::Path) -> TransferResult<()>;

    /// 获取目录大小
    async fn get_directory_size(&self, path: &std::path::Path) -> TransferResult<u64>;
}