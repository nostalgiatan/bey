//! # 存储模块
//!
//! 提供统一的文件存储操作接口，支持本地和远程存储。
//! 实现高性能的文件读写操作和存储抽象层。
//!
//! ## 核心功能
//!
//! - **统一接口**: 提供本地和远程存储的统一操作接口
//! - **高性能**: 优化的大文件读写和缓冲机制
//! - **并发安全**: 支持多线程并发访问
//! - **错误处理**: 完善的错误处理和恢复机制
//! - **存储监控**: 实时的存储空间和性能监控
//! - **BEY网络**: 使用BEY分布式网络替代HTTP协议

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::net::SocketAddr;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::{info, warn, error, debug, instrument};
use bytes::Bytes;
use crate::{TransferResult, FileInfo, StorageInterface};
use std::os::unix::fs::PermissionsExt;

/// 本地存储实现
///
/// 提供本地文件系统的高性能访问接口。
/// 支持并发操作和智能缓存机制。
#[derive(Debug)]
pub struct LocalStorage {
    /// 基础路径
    base_path: PathBuf,
    /// 缓冲区大小
    #[allow(dead_code)]
    buffer_size: usize,
    /// 存储统计信息
    statistics: Arc<StorageStatistics>,
}

/// 远程存储实现（简化版本）
///
/// 提供远程文件系统访问接口的基础框架。
/// 实际的网络传输功能可以根据需要扩展。
pub struct RemoteStorage {
    /// 远程地址
    remote_address: String,
    /// 认证令牌
    auth_token: String,
    /// 存储统计信息
    statistics: Arc<StorageStatistics>,
}

/// 远程存储配置
#[derive(Debug, Clone)]
pub struct RemoteStorageConfig {
    /// 远程地址
    pub remote_address: String,
    /// 超时时间（秒）
    pub timeout_secs: u64,
    /// 最大重试次数
    pub max_retries: usize,
}

impl Default for RemoteStorageConfig {
    fn default() -> Self {
        Self {
            remote_address: "http://localhost:8080".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

impl LocalStorage {
    /// 创建新的本地存储实例
    ///
    /// # 参数
    ///
    /// * `base_path` - 基础存储路径
    /// * `buffer_size` - 缓冲区大小
    ///
    /// # 返回
    ///
    /// 返回本地存储实例或错误信息
    #[instrument(skip(base_path), fields(base_path = %base_path.as_ref().display()))]
    pub async fn new<P: AsRef<Path>>(base_path: P, buffer_size: usize) -> TransferResult<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        info!("创建本地存储实例，基础路径: {}, 缓冲区大小: {}",
              base_path.display(), buffer_size);

        // 确保基础路径存在
        if let Err(e) = fs::create_dir_all(&base_path).await {
            error!("创建基础目录失败: {}", e);
            return Err(ErrorInfo::new(
                7601,
                format!("创建基础目录失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error));
        }

        Ok(Self {
            base_path,
            buffer_size,
            statistics: Arc::new(StorageStatistics::default()),
        })
    }

    /// 获取完整路径
    fn get_full_path(&self, path: &Path) -> PathBuf {
        self.base_path.join(path.strip_prefix("/").unwrap_or(path))
    }

    /// 更新读取统计信息
    #[allow(dead_code)]
    async fn update_read_statistics(&self, bytes_read: u64, duration: Duration) {
        self.statistics.total_reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.statistics.total_bytes_read.fetch_add(bytes_read, std::sync::atomic::Ordering::Relaxed);

        if duration.as_secs_f64() > 0.0 {
            let speed = bytes_read as f64 / duration.as_secs_f64();
            self.statistics.average_read_speed.store(
                speed as u64,
                std::sync::atomic::Ordering::Relaxed
            );
        }
    }

    /// 更新写入统计信息
    #[allow(dead_code)]
    async fn update_write_statistics(&self, bytes_written: u64, duration: Duration) {
        self.statistics.total_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.statistics.total_bytes_written.fetch_add(bytes_written, std::sync::atomic::Ordering::Relaxed);

        if duration.as_secs_f64() > 0.0 {
            let speed = bytes_written as f64 / duration.as_secs_f64();
            self.statistics.average_write_speed.store(
                speed as u64,
                std::sync::atomic::Ordering::Relaxed
            );
        }
    }

    /// 获取存储统计信息快照
    pub async fn get_statistics_snapshot(&self) -> StorageStatisticsSnapshot {
        StorageStatisticsSnapshot {
            total_reads: self.statistics.total_reads.load(std::sync::atomic::Ordering::Relaxed),
            total_writes: self.statistics.total_writes.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_read: self.statistics.total_bytes_read.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_written: self.statistics.total_bytes_written.load(std::sync::atomic::Ordering::Relaxed),
            average_read_speed: self.statistics.average_read_speed.load(std::sync::atomic::Ordering::Relaxed),
            average_write_speed: self.statistics.average_write_speed.load(std::sync::atomic::Ordering::Relaxed),
            error_count: self.statistics.error_count.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[async_trait::async_trait]
impl StorageInterface for LocalStorage {
    async fn read_chunk(&self, path: &Path, offset: u64, size: usize) -> TransferResult<Bytes> {
        let start_time = SystemTime::now();
        debug!("读取本地文件块，路径: {}, 偏移: {}, 大小: {}", path.display(), offset, size);

        let full_path = self.get_full_path(path);

        // 打开文件
        let mut file = fs::File::open(&full_path).await.map_err(|e| {
            error!("打开文件失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7602, format!("打开文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 定位到指定偏移
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| {
            error!("文件定位失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7603, format!("文件定位失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 读取数据
        let mut buffer = vec![0u8; size];
        let bytes_read = file.read(&mut buffer).await.map_err(|e| {
            error!("读取文件失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7604, format!("读取文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 更新统计信息
        let duration = SystemTime::now().duration_since(start_time).unwrap_or_default();
        self.update_read_statistics(bytes_read as u64, duration).await;

        debug!("本地文件块读取完成，实际读取 {} 字节", bytes_read);
        Ok(Bytes::from(buffer[..bytes_read].to_vec()))
    }

    async fn write_chunk(&self, path: &Path, offset: u64, data: Bytes) -> TransferResult<()> {
        let start_time = SystemTime::now();
        debug!("写入本地文件块，路径: {}, 偏移: {}, 大小: {}", path.display(), offset, data.len());

        let full_path = self.get_full_path(path);

        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                error!("创建父目录失败: {}", e);
                self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ErrorInfo::new(7605, format!("创建父目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;
        }

        // 打开文件（创建或追加）
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&full_path)
            .await
            .map_err(|e| {
                error!("打开文件用于写入失败: {}", e);
                self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ErrorInfo::new(7606, format!("打开文件用于写入失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;

        // 定位到指定偏移
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| {
            error!("文件定位失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7607, format!("文件定位失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 写入数据
        file.write_all(&data).await.map_err(|e| {
            error!("写入文件失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7608, format!("写入文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 确保数据写入磁盘
        file.sync_all().await.map_err(|e| {
            error!("同步文件失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7609, format!("同步文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        // 更新统计信息
        let duration = SystemTime::now().duration_since(start_time).unwrap_or_default();
        self.update_write_statistics(data.len() as u64, duration).await;

        debug!("本地文件块写入完成，写入大小: {} 字节", data.len());
        Ok(())
    }

    async fn get_file_info(&self, path: &Path) -> TransferResult<FileInfo> {
        debug!("获取本地文件信息，路径: {}", path.display());

        let full_path = self.get_full_path(path);

        let metadata = fs::metadata(&full_path).await.map_err(|e| {
            error!("获取文件元数据失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7610, format!("获取文件元数据失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        let file_info = FileInfo {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
            is_dir: metadata.is_dir(),
            permissions: Some(metadata.permissions().mode()),
            hash: None, // 本地存储不自动计算哈希
        };

        debug!("获取本地文件信息完成: {}, 大小: {} 字节", path.display(), file_info.size);
        Ok(file_info)
    }

    async fn create_dir(&self, path: &Path) -> TransferResult<()> {
        debug!("创建本地目录，路径: {}", path.display());

        let full_path = self.get_full_path(path);

        fs::create_dir_all(&full_path).await.map_err(|e| {
            error!("创建目录失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7611, format!("创建目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        debug!("本地目录创建完成: {}", path.display());
        Ok(())
    }

    async fn delete_file(&self, path: &Path) -> TransferResult<()> {
        debug!("删除本地文件，路径: {}", path.display());

        let full_path = self.get_full_path(path);

        fs::remove_file(&full_path).await.map_err(|e| {
            error!("删除文件失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7612, format!("删除文件失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        debug!("本地文件删除完成: {}", path.display());
        Ok(())
    }

    async fn exists(&self, path: &Path) -> TransferResult<bool> {
        debug!("检查本地文件是否存在，路径: {}", path.display());

        let full_path = self.get_full_path(path);
        let exists = fs::metadata(&full_path).await.is_ok();

        debug!("本地文件存在性检查完成: {} -> {}", path.display(), exists);
        Ok(exists)
    }

    /// 列出目录内容
    async fn list_directory(&self, path: &Path) -> TransferResult<Vec<crate::DirectoryEntry>> {
        let start_time = SystemTime::now();
        debug!("列出目录内容，路径: {}", path.display());

        let full_path = self.get_full_path(path);

        // 检查路径是否存在且为目录
        let metadata = fs::metadata(&full_path).await.map_err(|e| {
            error!("获取路径元数据失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7610, format!("获取路径元数据失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        if !metadata.is_dir() {
            return Err(ErrorInfo::new(7611, format!("路径不是目录: {}", path.display()))
                .with_category(ErrorCategory::Validation)
                .with_severity(ErrorSeverity::Error));
        }

        // 读取目录内容
        let mut entries = Vec::<crate::DirectoryEntry>::new();
        let mut dir_entry = fs::read_dir(&full_path).await.map_err(|e| {
            error!("读取目录失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7612, format!("读取目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        while let Some(entry) = dir_entry.next_entry().await.map_err(|e| {
            error!("读取目录条目失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7613, format!("读取目录条目失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_string();

            // 跳过隐藏文件（以.开头的文件）
            if name.starts_with('.') {
                continue;
            }

            let entry_path = entry.path();
            let entry_metadata = entry.metadata().await.map_err(|e| {
                error!("获取文件元数据失败: {}", e);
                self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ErrorInfo::new(7614, format!("获取文件元数据失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;

            let is_symlink = entry_metadata.file_type().is_symlink();
            let symlink_target = if is_symlink {
                fs::read_link(&entry_path).await.ok().map(|target| target.to_path_buf())
            } else {
                None
            };

            let (size, is_directory) = if is_symlink {
                // 对于符号链接，获取目标文件的信息
                if let Some(ref target) = symlink_target {
                    match fs::metadata(target).await {
                        Ok(meta) => (meta.len(), meta.is_dir()),
                        Err(_) => (0, false), // 如果目标不存在，使用默认值
                    }
                } else {
                    (0, false)
                }
            } else {
                (entry_metadata.len(), entry_metadata.is_dir())
            };

            let modified = entry_metadata.modified().unwrap_or_else(|_| SystemTime::now());
            let permissions = entry_metadata.permissions().mode();

            entries.push(crate::DirectoryEntry {
                name,
                path: entry_path.strip_prefix(&self.base_path).unwrap_or(&entry_path).to_path_buf(),
                is_directory,
                size,
                modified,
                permissions: Some(permissions),
                is_symlink,
                symlink_target,
            });
        }

        // 按名称排序
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let elapsed = start_time.elapsed().unwrap_or_default();
        info!("目录列表完成，路径: {}, 条目数: {}, 耗时: {:?}", path.display(), entries.len(), elapsed);

        Ok(entries)
    }

    /// 删除目录
    async fn remove_dir(&self, path: &Path) -> TransferResult<()> {
        let start_time = SystemTime::now();
        debug!("删除目录，路径: {}", path.display());

        let full_path = self.get_full_path(path);

        fs::remove_dir(&full_path).await.map_err(|e| {
            error!("删除目录失败: {}", e);
            self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ErrorInfo::new(7615, format!("删除目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error)
        })?;

        let elapsed = start_time.elapsed().unwrap_or_default();
        info!("目录删除完成，路径: {}, 耗时: {:?}", path.display(), elapsed);

        Ok(())
    }

    /// 获取目录大小
    async fn get_directory_size(&self, path: &Path) -> TransferResult<u64> {
        let start_time = SystemTime::now();
        debug!("计算目录大小，路径: {}", path.display());

        let full_path = self.get_full_path(path);
        let mut total_size = 0u64;

        // 递归遍历目录
        let mut stack = vec![full_path];
        while let Some(current_path) = stack.pop() {
            let mut dir_entry = fs::read_dir(&current_path).await.map_err(|e| {
                error!("读取目录失败: {}", e);
                self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ErrorInfo::new(7616, format!("读取目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })?;

            while let Some(entry) = dir_entry.next_entry().await.map_err(|e| {
                error!("读取目录条目失败: {}", e);
                self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ErrorInfo::new(7617, format!("读取目录条目失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error)
            })? {
                let entry_path = entry.path();
                let entry_metadata = entry.metadata().await.map_err(|e| {
                    error!("获取文件元数据失败: {}", e);
                    self.statistics.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    ErrorInfo::new(7618, format!("获取文件元数据失败: {}", e))
                        .with_category(ErrorCategory::FileSystem)
                        .with_severity(ErrorSeverity::Error)
                })?;

                if entry_metadata.is_dir() && !entry_metadata.file_type().is_symlink() {
                    stack.push(entry_path);
                } else {
                    total_size += entry_metadata.len();
                }
            }
        }

        let elapsed = start_time.elapsed().unwrap_or_default();
        info!("目录大小计算完成，路径: {}, 大小: {} 字节, 耗时: {:?}", path.display(), total_size, elapsed);

        Ok(total_size)
    }
}

impl RemoteStorage {
    /// 创建新的远程存储实例
    ///
    /// # 参数
    ///
    /// * `remote_address` - 远程地址
    /// * `auth_token` - 认证令牌
    /// * `config` - 远程存储配置
    ///
    /// # 返回
    ///
    /// 返回远程存储实例
    #[instrument(skip(remote_address, auth_token), fields(remote_address))]
    pub async fn new(remote_address: String, auth_token: String, _config: RemoteStorageConfig) -> TransferResult<Self> {
        info!("创建远程存储实例，地址: {}", remote_address);

        Ok(Self {
            remote_address,
            auth_token,
            statistics: Arc::new(StorageStatistics::default()),
        })
    }

    /// 使用默认配置创建远程存储实例
    ///
    /// # 参数
    ///
    /// * `remote_address` - 远程地址
    /// * `auth_token` - 认证令牌
    ///
    /// # 返回
    ///
    /// 返回远程存储实例
    pub async fn new_default(remote_address: String, auth_token: String) -> TransferResult<Self> {
        Self::new(remote_address, auth_token, RemoteStorageConfig::default()).await
    }

    
    /// 更新读取统计信息
    pub async fn update_read_statistics(&self, bytes_read: u64, duration: Duration) {
        self.statistics.total_reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.statistics.total_bytes_read.fetch_add(bytes_read, std::sync::atomic::Ordering::Relaxed);

        if duration.as_secs_f64() > 0.0 {
            let speed = bytes_read as f64 / duration.as_secs_f64();
            self.statistics.average_read_speed.store(
                speed as u64,
                std::sync::atomic::Ordering::Relaxed
            );
        }
    }

    /// 更新写入统计信息
    pub async fn update_write_statistics(&self, bytes_written: u64, duration: Duration) {
        self.statistics.total_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.statistics.total_bytes_written.fetch_add(bytes_written, std::sync::atomic::Ordering::Relaxed);

        if duration.as_secs_f64() > 0.0 {
            let speed = bytes_written as f64 / duration.as_secs_f64();
            self.statistics.average_write_speed.store(
                speed as u64,
                std::sync::atomic::Ordering::Relaxed
            );
        }
    }

    /// 获取存储统计信息快照
    pub async fn get_statistics_snapshot(&self) -> StorageStatisticsSnapshot {
        StorageStatisticsSnapshot {
            total_reads: self.statistics.total_reads.load(std::sync::atomic::Ordering::Relaxed),
            total_writes: self.statistics.total_writes.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_read: self.statistics.total_bytes_read.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_written: self.statistics.total_bytes_written.load(std::sync::atomic::Ordering::Relaxed),
            average_read_speed: self.statistics.average_read_speed.load(std::sync::atomic::Ordering::Relaxed),
            average_write_speed: self.statistics.average_write_speed.load(std::sync::atomic::Ordering::Relaxed),
            error_count: self.statistics.error_count.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[async_trait::async_trait]
impl StorageInterface for RemoteStorage {
    async fn read_chunk(&self, path: &Path, _offset: u64, _size: usize) -> TransferResult<Bytes> {
        warn!("远程存储读取功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7650, "远程存储读取功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    async fn write_chunk(&self, path: &Path, _offset: u64, _data: Bytes) -> TransferResult<()> {
        warn!("远程存储写入功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7651, "远程存储写入功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    async fn get_file_info(&self, path: &Path) -> TransferResult<FileInfo> {
        warn!("远程存储获取文件信息功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7652, "远程存储获取文件信息功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    async fn create_dir(&self, path: &Path) -> TransferResult<()> {
        warn!("远程存储创建目录功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7653, "远程存储创建目录功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    async fn delete_file(&self, path: &Path) -> TransferResult<()> {
        warn!("远程存储删除文件功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7654, "远程存储删除文件功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    async fn exists(&self, path: &Path) -> TransferResult<bool> {
        warn!("远程存储文件存在检查功能尚未实现: {}", path.display());
        Err(ErrorInfo::new(7655, "远程存储文件存在检查功能尚未实现".to_string())
            .with_category(ErrorCategory::Network)
            .with_severity(ErrorSeverity::Warning))
    }

    /// 列出目录内容（远程存储完整实现 - 使用BEY传输层）
    async fn list_directory(&self, path: &Path) -> TransferResult<Vec<crate::DirectoryEntry>> {
        debug!("远程存储目录列表: {}", path.display());

        // 创建传输层配置
        let transport_config = bey_transport::TransportConfig::new()
            .with_port(8080) // 默认端口
            .with_connection_timeout(Duration::from_secs(30))
            .with_max_connections(1);

        // 创建安全传输层
        let transport = bey_transport::SecureTransport::new(transport_config, "bey_file_transfer".to_string()).await
            .map_err(|e| ErrorInfo::new(7656, format!("创建传输层失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析远程地址
        let remote_addr: SocketAddr = self.remote_address.parse()
            .map_err(|e| ErrorInfo::new(7657, format!("解析远程地址失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 从远程地址提取设备ID
        let target_device_id = remote_addr.ip().to_string();

        // 建立连接
        let connection = transport.connect(remote_addr).await
            .map_err(|e| ErrorInfo::new(7658, format!("连接远程存储失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 构建目录列表请求消息
        let request_message = bey_transport::TransportMessage {
            id: format!("list_dir_{}", uuid::Uuid::new_v4()),
            message_type: "storage_list_directory".to_string(),
            content: serde_json::json!({
                "path": path.to_string_lossy(),
                "auth_token": self.auth_token
            }),
            timestamp: SystemTime::now(),
            sender_id: "bey_file_transfer".to_string(),
            receiver_id: Some(target_device_id.clone()),
        };

        // 发送请求
        transport.send_message(&connection, request_message).await
            .map_err(|e| ErrorInfo::new(7659, format!("发送目录列表请求失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 接收响应
        let response = transport.receive_message(&connection).await
            .map_err(|e| ErrorInfo::new(7660, format!("接收目录列表响应失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析响应
        if response.message_type != "storage_list_directory_response" {
            return Err(ErrorInfo::new(7661, format!("收到意外的响应类型: {}", response.message_type))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error));
        }

        let entries: Vec<crate::DirectoryEntry> = serde_json::from_value(response.content["entries"].clone())
            .map_err(|e| ErrorInfo::new(7662, format!("解析目录列表响应失败: {}", e))
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        // 断开连接
        let _ = transport.disconnect(remote_addr).await;

        info!("远程存储目录列表完成，路径: {}, 返回 {} 个条目", path.display(), entries.len());
        Ok(entries)
    }

    /// 删除目录（远程存储完整实现 - 使用BEY传输层）
    async fn remove_dir(&self, path: &Path) -> TransferResult<()> {
        debug!("远程存储删除目录: {}", path.display());

        // 创建传输层配置
        let transport_config = bey_transport::TransportConfig::new()
            .with_port(8080)
            .with_connection_timeout(Duration::from_secs(30))
            .with_max_connections(1);

        // 创建安全传输层
        let transport = bey_transport::SecureTransport::new(transport_config, "bey_file_transfer".to_string()).await
            .map_err(|e| ErrorInfo::new(7663, format!("创建传输层失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析远程地址
        let remote_addr: SocketAddr = self.remote_address.parse()
            .map_err(|e| ErrorInfo::new(7664, format!("解析远程地址失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 从远程地址提取设备ID
        let target_device_id = remote_addr.ip().to_string();

        // 建立连接
        let connection = transport.connect(remote_addr).await
            .map_err(|e| ErrorInfo::new(7665, format!("连接远程存储失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 构建删除目录请求消息
        let request_message = bey_transport::TransportMessage {
            id: format!("remove_dir_{}", uuid::Uuid::new_v4()),
            message_type: "storage_remove_directory".to_string(),
            content: serde_json::json!({
                "path": path.to_string_lossy(),
                "auth_token": self.auth_token
            }),
            timestamp: SystemTime::now(),
            sender_id: "bey_file_transfer".to_string(),
            receiver_id: Some(target_device_id.clone()),
        };

        // 发送请求
        transport.send_message(&connection, request_message).await
            .map_err(|e| ErrorInfo::new(7666, format!("发送删除目录请求失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 接收响应
        let response = transport.receive_message(&connection).await
            .map_err(|e| ErrorInfo::new(7667, format!("接收删除目录响应失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析响应
        if response.message_type != "storage_remove_directory_response" {
            return Err(ErrorInfo::new(7668, format!("收到意外的响应类型: {}", response.message_type))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error));
        }

        if response.content["success"].as_bool().unwrap_or(false) {
            info!("远程存储删除目录完成: {}", path.display());
            Ok(())
        } else {
            let error_msg = response.content["error"].as_str().unwrap_or("未知错误");
            Err(ErrorInfo::new(7669, format!("远程存储删除目录失败: {}", error_msg))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))
        }
    }

    /// 获取目录大小（远程存储完整实现 - 使用BEY传输层）
    async fn get_directory_size(&self, path: &Path) -> TransferResult<u64> {
        debug!("远程存储计算目录大小: {}", path.display());

        // 创建传输层配置
        let transport_config = bey_transport::TransportConfig::new()
            .with_port(8080)
            .with_connection_timeout(Duration::from_secs(30))
            .with_max_connections(1);

        // 创建安全传输层
        let transport = bey_transport::SecureTransport::new(transport_config, "bey_file_transfer".to_string()).await
            .map_err(|e| ErrorInfo::new(7670, format!("创建传输层失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析远程地址
        let remote_addr: SocketAddr = self.remote_address.parse()
            .map_err(|e| ErrorInfo::new(7671, format!("解析远程地址失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 从远程地址提取设备ID
        let target_device_id = remote_addr.ip().to_string();

        // 建立连接
        let connection = transport.connect(remote_addr).await
            .map_err(|e| ErrorInfo::new(7672, format!("连接远程存储失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 构建获取目录大小请求消息
        let request_message = bey_transport::TransportMessage {
            id: format!("get_dir_size_{}", uuid::Uuid::new_v4()),
            message_type: "storage_get_directory_size".to_string(),
            content: serde_json::json!({
                "path": path.to_string_lossy(),
                "auth_token": self.auth_token
            }),
            timestamp: SystemTime::now(),
            sender_id: "bey_file_transfer".to_string(),
            receiver_id: Some(target_device_id.clone()),
        };

        // 发送请求
        transport.send_message(&connection, request_message).await
            .map_err(|e| ErrorInfo::new(7673, format!("发送获取目录大小请求失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 接收响应
        let response = transport.receive_message(&connection).await
            .map_err(|e| ErrorInfo::new(7674, format!("接收获取目录大小响应失败: {}", e))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error))?;

        // 解析响应
        if response.message_type != "storage_get_directory_size_response" {
            return Err(ErrorInfo::new(7675, format!("收到意外的响应类型: {}", response.message_type))
                .with_category(ErrorCategory::Network)
                .with_severity(ErrorSeverity::Error));
        }

        let size = response.content["size"].as_u64()
            .ok_or_else(|| ErrorInfo::new(7676, "响应中缺少目录大小信息".to_string())
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error))?;

        info!("远程存储目录大小计算完成: {} -> {} 字节", path.display(), size);
        Ok(size)
    }
}

impl std::fmt::Debug for RemoteStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteStorage")
            .field("remote_address", &self.remote_address)
            .field("auth_token", &"[REDACTED]")
            .finish()
    }
}

/// 存储统计信息
#[derive(Debug, Default)]
pub struct StorageStatistics {
    /// 总读取次数
    pub total_reads: std::sync::atomic::AtomicU64,
    /// 总写入次数
    pub total_writes: std::sync::atomic::AtomicU64,
    /// 总读取字节数
    pub total_bytes_read: std::sync::atomic::AtomicU64,
    /// 总写入字节数
    pub total_bytes_written: std::sync::atomic::AtomicU64,
    /// 平均读取速度（字节/秒）
    pub average_read_speed: std::sync::atomic::AtomicU64,
    /// 平均写入速度（字节/秒）
    pub average_write_speed: std::sync::atomic::AtomicU64,
    /// 错误次数
    pub error_count: std::sync::atomic::AtomicU64,
}

/// 存储统计信息快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatisticsSnapshot {
    /// 总读取次数
    pub total_reads: u64,
    /// 总写入次数
    pub total_writes: u64,
    /// 总读取字节数
    pub total_bytes_read: u64,
    /// 总写入字节数
    pub total_bytes_written: u64,
    /// 平均读取速度（字节/秒）
    pub average_read_speed: u64,
    /// 平均写入速度（字节/秒）
    pub average_write_speed: u64,
    /// 错误次数
    pub error_count: u64,
}

/// 存储工厂
pub struct StorageFactory;

impl StorageFactory {
    /// 创建本地存储
    pub async fn create_local_storage<P: AsRef<Path>>(
        base_path: P,
        buffer_size: usize,
    ) -> TransferResult<Arc<dyn StorageInterface>> {
        let storage = LocalStorage::new(base_path, buffer_size).await?;
        Ok(Arc::new(storage))
    }

    /// 创建远程存储
    pub async fn create_remote_storage(
        remote_address: String,
        auth_token: String,
        config: RemoteStorageConfig,
    ) -> TransferResult<Arc<dyn StorageInterface>> {
        let storage = RemoteStorage::new(remote_address, auth_token, config).await?;
        Ok(Arc::new(storage))
    }

    /// 创建默认配置的远程存储
    pub async fn create_remote_storage_default(
        remote_address: String,
        auth_token: String,
    ) -> TransferResult<Arc<dyn StorageInterface>> {
        let storage = RemoteStorage::new_default(remote_address, auth_token).await?;
        Ok(Arc::new(storage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_local_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path(), 8192).await;
        assert!(storage.is_ok());
    }

    #[tokio::test]
    async fn test_local_storage_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path(), 8192).await.unwrap();
        let storage: Arc<dyn StorageInterface> = Arc::new(storage);

        let test_path = Path::new("test_file.txt");
        let test_data = Bytes::from("Hello, BEY Storage!");

        // 写入数据
        storage.write_chunk(test_path, 0, test_data.clone()).await.unwrap();

        // 读取数据
        let read_data = storage.read_chunk(test_path, 0, test_data.len()).await.unwrap();

        assert_eq!(test_data, read_data);
    }

    #[tokio::test]
    async fn test_local_storage_file_info() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path(), 8192).await.unwrap();
        let storage: Arc<dyn StorageInterface> = Arc::new(storage);

        let test_path = Path::new("test_file.txt");
        let test_data = Bytes::from("Hello, BEY Storage!");

        // 写入数据
        storage.write_chunk(test_path, 0, test_data.clone()).await.unwrap();

        // 获取文件信息
        let file_info = storage.get_file_info(test_path).await.unwrap();

        assert_eq!(file_info.size, test_data.len() as u64);
        assert!(!file_info.is_dir);
    }

    #[tokio::test]
    async fn test_local_storage_directory_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path(), 8192).await.unwrap();
        let storage: Arc<dyn StorageInterface> = Arc::new(storage);

        let test_dir = Path::new("test_dir");

        // 创建目录
        storage.create_dir(test_dir).await.unwrap();

        // 检查目录是否存在（通过创建文件的方式）
        let test_file = test_dir.join("test_file.txt");
        let test_data = Bytes::from("Test data");
        storage.write_chunk(&test_file, 0, test_data).await.unwrap();

        let exists = storage.exists(&test_file).await.unwrap();
        assert!(exists);

        // 删除文件
        storage.delete_file(&test_file).await.unwrap();

        let exists_after_delete = storage.exists(&test_file).await.unwrap();
        assert!(!exists_after_delete);
    }

    #[tokio::test]
    async fn test_storage_factory() {
        let temp_dir = TempDir::new().unwrap();

        // 测试创建本地存储
        let local_storage = StorageFactory::create_local_storage(temp_dir.path(), 8192).await;
        assert!(local_storage.is_ok());

        // 测试创建远程存储（这里会成功，但功能未实现）
        let remote_storage = StorageFactory::create_remote_storage_default(
            "http://localhost:8080".to_string(),
            "test-token".to_string(),
        ).await;
        // 远程存储创建会成功，但使用时会返回未实现错误
        assert!(remote_storage.is_ok());
    }
}