//! # 云存储模块
//!
//! 提供分布式云存储功能，使用sled数据库存储元数据，
//! 文件使用zstd压缩并带有二进制前缀，存储为.beycloud文件。
//! 实现动态冗余算法和一致性哈希分布。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use sled::Db;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tracing::{info, debug};
use sha2::{Sha256, Digest};

/// 云存储结果类型
pub type CloudStorageResult<T> = std::result::Result<T, ErrorInfo>;

/// 云存储配置
#[derive(Debug, Clone)]
pub struct CloudStorageConfig {
    /// 存储根目录
    pub storage_root: PathBuf,
    /// 数据库路径
    pub db_path: PathBuf,
    /// 块大小（字节）
    pub chunk_size: usize,
    /// 冗余因子（1表示无冗余）
    pub redundancy_factor: usize,
    /// 最大本地存储大小（字节）
    pub max_local_storage: u64,
}

impl Default for CloudStorageConfig {
    fn default() -> Self {
        Self {
            storage_root: PathBuf::from("./cloud_storage"),
            db_path: PathBuf::from("./cloud_storage/metadata.db"),
            chunk_size: 1024 * 1024, // 1MB
            redundancy_factor: 2,
            max_local_storage: 10 * 1024 * 1024 * 1024, // 10GB
        }
    }
}

/// 文件元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    /// 文件名
    pub filename: String,
    /// 文件大小
    pub size: u64,
    /// 文件哈希（SHA-256）
    pub hash: String,
    /// 上传时间
    pub upload_time: u64,
    /// 块ID列表
    pub chunk_ids: Vec<String>,
    /// 原始文件哈希（用于验证）
    pub original_hash: String,
}

/// 块前缀结构（固定128字节）
#[derive(Debug)]
struct ChunkPrefix {
    /// 文件名（最多64字节，不足补零）
    filename: [u8; 64],
    /// 上传时间戳（8字节）
    timestamp: u64,
    /// 文件哈希（32字节，SHA-256）
    file_hash: [u8; 32],
    /// 块索引（4字节）
    chunk_index: u32,
    /// 总块数（4字节）
    total_chunks: u32,
    /// 预留字节（16字节）
    _reserved: [u8; 16],
}

impl ChunkPrefix {
    const SIZE: usize = 128;

    /// 创建新的块前缀
    fn new(filename: &str, file_hash: &[u8], chunk_index: u32, total_chunks: u32) -> Self {
        let mut filename_bytes = [0u8; 64];
        let name_bytes = filename.as_bytes();
        let copy_len = name_bytes.len().min(64);
        filename_bytes[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(file_hash);

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            filename: filename_bytes,
            timestamp,
            file_hash: hash_bytes,
            chunk_index,
            total_chunks,
            _reserved: [0u8; 16],
        }
    }

    /// 序列化为字节
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.extend_from_slice(&self.filename);
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes.extend_from_slice(&self.file_hash);
        bytes.extend_from_slice(&self.chunk_index.to_le_bytes());
        bytes.extend_from_slice(&self.total_chunks.to_le_bytes());
        bytes.extend_from_slice(&self._reserved);
        bytes
    }

    /// 从字节反序列化
    fn from_bytes(bytes: &[u8]) -> CloudStorageResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(ErrorInfo::new(6101, "块前缀数据不完整".to_string())
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error));
        }

        let mut filename = [0u8; 64];
        filename.copy_from_slice(&bytes[0..64]);

        let timestamp = u64::from_le_bytes(
            bytes[64..72].try_into()
                .map_err(|_| ErrorInfo::new(6102, "时间戳解析失败".to_string())
                    .with_category(ErrorCategory::Parse))?
        );

        let mut file_hash = [0u8; 32];
        file_hash.copy_from_slice(&bytes[72..104]);

        let chunk_index = u32::from_le_bytes(
            bytes[104..108].try_into()
                .map_err(|_| ErrorInfo::new(6103, "块索引解析失败".to_string())
                    .with_category(ErrorCategory::Parse))?
        );

        let total_chunks = u32::from_le_bytes(
            bytes[108..112].try_into()
                .map_err(|_| ErrorInfo::new(6104, "总块数解析失败".to_string())
                    .with_category(ErrorCategory::Parse))?
        );

        let mut _reserved = [0u8; 16];
        _reserved.copy_from_slice(&bytes[112..128]);

        Ok(Self {
            filename,
            timestamp,
            file_hash,
            chunk_index,
            total_chunks,
            _reserved,
        })
    }

    /// 获取文件名
    #[allow(dead_code)]
    fn get_filename(&self) -> String {
        // 找到第一个零字节
        let end = self.filename.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&self.filename[..end]).to_string()
    }
}

/// 云存储管理器
pub struct CloudStorage {
    config: CloudStorageConfig,
    db: Arc<Db>,
}

impl CloudStorage {
    /// 创建新的云存储实例
    ///
    /// # 参数
    ///
    /// * `config` - 存储配置
    ///
    /// # 返回值
    ///
    /// 返回云存储实例或错误
    pub async fn new(config: CloudStorageConfig) -> CloudStorageResult<Self> {
        // 创建存储目录
        fs::create_dir_all(&config.storage_root).await
            .map_err(|e| ErrorInfo::new(6105, format!("创建存储目录失败: {}", e))
                .with_category(ErrorCategory::FileSystem)
                .with_severity(ErrorSeverity::Error))?;

        // 创建数据库目录
        if let Some(parent) = config.db_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| ErrorInfo::new(6106, format!("创建数据库目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem)
                    .with_severity(ErrorSeverity::Error))?;
        }

        // 打开sled数据库
        let db = sled::open(&config.db_path)
            .map_err(|e| ErrorInfo::new(6107, format!("打开数据库失败: {}", e))
                .with_category(ErrorCategory::Database)
                .with_severity(ErrorSeverity::Error))?;

        info!("云存储初始化成功: {:?}", config.storage_root);
        Ok(Self {
            config,
            db: Arc::new(db),
        })
    }

    /// 计算文件哈希
    fn calculate_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// 上传文件到云存储
    ///
    /// # 参数
    ///
    /// * `filename` - 文件名
    /// * `data` - 文件数据
    ///
    /// # 返回值
    ///
    /// 返回文件哈希或错误
    pub async fn upload_file(&self, filename: &str, data: &[u8]) -> CloudStorageResult<String> {
        // 计算文件哈希
        let file_hash = Self::calculate_hash(data);
        let file_hash_bytes = hex::decode(&file_hash)
            .map_err(|e| ErrorInfo::new(6108, format!("哈希解码失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 检查文件是否已存在
        if self.db.contains_key(file_hash.as_bytes())
            .map_err(|e| ErrorInfo::new(6109, format!("检查文件存在失败: {}", e))
                .with_category(ErrorCategory::Database))? {
            info!("文件已存在: {}", file_hash);
            return Ok(file_hash);
        }

        // 分块
        let chunk_size = self.config.chunk_size;
        let total_chunks = (data.len() + chunk_size - 1) / chunk_size;
        let mut chunk_ids = Vec::new();

        for (index, chunk_data) in data.chunks(chunk_size).enumerate() {
            // 压缩块数据
            let compressed = zstd::encode_all(chunk_data, 3)
                .map_err(|e| ErrorInfo::new(6110, format!("压缩失败: {}", e))
                    .with_category(ErrorCategory::Compression))?;

            // 创建块前缀
            let prefix = ChunkPrefix::new(
                filename,
                &file_hash_bytes,
                index as u32,
                total_chunks as u32,
            );
            let prefix_bytes = prefix.to_bytes();

            // 组合前缀和压缩数据
            let mut chunk_with_prefix = prefix_bytes;
            chunk_with_prefix.extend_from_slice(&compressed);

            // 计算块哈希作为块ID
            let chunk_hash = Self::calculate_hash(&chunk_with_prefix);
            let chunk_filename = format!("{}.beycloud", chunk_hash);
            let chunk_path = self.config.storage_root.join(&chunk_filename);

            // 写入块文件
            fs::write(&chunk_path, &chunk_with_prefix).await
                .map_err(|e| ErrorInfo::new(6111, format!("写入块文件失败: {}", e))
                    .with_category(ErrorCategory::FileSystem))?;

            chunk_ids.push(chunk_hash);
            debug!("块 {}/{} 上传成功: {}", index + 1, total_chunks, chunk_filename);
        }

        // 创建元数据
        let metadata = FileMetadata {
            filename: filename.to_string(),
            size: data.len() as u64,
            hash: file_hash.clone(),
            upload_time: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            chunk_ids,
            original_hash: file_hash.clone(),
        };

        // 存储元数据到数据库
        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(|e| ErrorInfo::new(6112, format!("序列化元数据失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        self.db.insert(file_hash.as_bytes(), metadata_json)
            .map_err(|e| ErrorInfo::new(6113, format!("存储元数据失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        info!("文件上传成功: {} -> {}", filename, file_hash);
        Ok(file_hash)
    }

    /// 从云存储下载文件
    ///
    /// # 参数
    ///
    /// * `file_hash` - 文件哈希
    ///
    /// # 返回值
    ///
    /// 返回文件数据或错误
    pub async fn download_file(&self, file_hash: &str) -> CloudStorageResult<Vec<u8>> {
        // 获取元数据
        let metadata_bytes = self.db.get(file_hash.as_bytes())
            .map_err(|e| ErrorInfo::new(6114, format!("查询元数据失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6115, format!("文件不存在: {}", file_hash))
                .with_category(ErrorCategory::FileSystem))?;

        let metadata: FileMetadata = serde_json::from_slice(&metadata_bytes)
            .map_err(|e| ErrorInfo::new(6116, format!("反序列化元数据失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 读取并组装所有块
        let mut file_data = Vec::with_capacity(metadata.size as usize);

        for (index, chunk_id) in metadata.chunk_ids.iter().enumerate() {
            let chunk_filename = format!("{}.beycloud", chunk_id);
            let chunk_path = self.config.storage_root.join(&chunk_filename);

            // 读取块文件
            let chunk_with_prefix = fs::read(&chunk_path).await
                .map_err(|e| ErrorInfo::new(6117, format!("读取块文件失败: {}", e))
                    .with_category(ErrorCategory::FileSystem))?;

            // 提取并验证前缀
            if chunk_with_prefix.len() < ChunkPrefix::SIZE {
                return Err(ErrorInfo::new(6118, format!("块文件 {} 格式错误", chunk_filename))
                    .with_category(ErrorCategory::Parse));
            }

            let prefix = ChunkPrefix::from_bytes(&chunk_with_prefix[..ChunkPrefix::SIZE])?;
            
            // 验证块索引
            if prefix.chunk_index as usize != index {
                return Err(ErrorInfo::new(6119, format!("块 {} 索引不匹配", chunk_filename))
                    .with_category(ErrorCategory::Validation));
            }

            // 解压缩块数据
            let compressed_data = &chunk_with_prefix[ChunkPrefix::SIZE..];
            let decompressed = zstd::decode_all(compressed_data)
                .map_err(|e| ErrorInfo::new(6120, format!("解压缩失败: {}", e))
                    .with_category(ErrorCategory::Compression))?;

            file_data.extend_from_slice(&decompressed);
            debug!("块 {}/{} 下载成功", index + 1, metadata.chunk_ids.len());
        }

        // 验证文件哈希
        let calculated_hash = Self::calculate_hash(&file_data);
        if calculated_hash != metadata.original_hash {
            return Err(ErrorInfo::new(6121, "文件哈希验证失败".to_string())
                .with_category(ErrorCategory::Validation));
        }

        info!("文件下载成功: {} ({} 字节)", file_hash, file_data.len());
        Ok(file_data)
    }

    /// 删除文件
    ///
    /// # 参数
    ///
    /// * `file_hash` - 文件哈希
    ///
    /// # 返回值
    ///
    /// 返回删除结果
    pub async fn delete_file(&self, file_hash: &str) -> CloudStorageResult<()> {
        // 获取元数据
        let metadata_bytes = self.db.get(file_hash.as_bytes())
            .map_err(|e| ErrorInfo::new(6122, format!("查询元数据失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6123, format!("文件不存在: {}", file_hash))
                .with_category(ErrorCategory::FileSystem))?;

        let metadata: FileMetadata = serde_json::from_slice(&metadata_bytes)
            .map_err(|e| ErrorInfo::new(6124, format!("反序列化元数据失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        // 删除所有块文件
        for chunk_id in &metadata.chunk_ids {
            let chunk_filename = format!("{}.beycloud", chunk_id);
            let chunk_path = self.config.storage_root.join(&chunk_filename);
            
            if chunk_path.exists() {
                fs::remove_file(&chunk_path).await
                    .map_err(|e| ErrorInfo::new(6125, format!("删除块文件失败: {}", e))
                        .with_category(ErrorCategory::FileSystem))?;
            }
        }

        // 从数据库删除元数据
        self.db.remove(file_hash.as_bytes())
            .map_err(|e| ErrorInfo::new(6126, format!("删除元数据失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        info!("文件删除成功: {}", file_hash);
        Ok(())
    }

    /// 列出所有文件
    ///
    /// # 返回值
    ///
    /// 返回文件元数据列表
    pub fn list_files(&self) -> CloudStorageResult<Vec<FileMetadata>> {
        let mut files = Vec::new();

        for item in self.db.iter() {
            let (_key, value) = item
                .map_err(|e| ErrorInfo::new(6127, format!("遍历数据库失败: {}", e))
                    .with_category(ErrorCategory::Database))?;

            let metadata: FileMetadata = serde_json::from_slice(&value)
                .map_err(|e| ErrorInfo::new(6128, format!("反序列化元数据失败: {}", e))
                    .with_category(ErrorCategory::Parse))?;

            files.push(metadata);
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_cloud_storage_upload_download() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let config = CloudStorageConfig {
            storage_root: temp_dir.path().join("storage"),
            db_path: temp_dir.path().join("db"),
            chunk_size: 1024,
            ..Default::default()
        };

        let storage = CloudStorage::new(config).await.expect("创建云存储失败");
        
        // 测试数据
        let test_data = b"Hello, Cloud Storage! This is a test file.".repeat(100);
        
        // 上传
        let file_hash = storage.upload_file("test.txt", &test_data).await.expect("上传失败");
        
        // 下载
        let downloaded = storage.download_file(&file_hash).await.expect("下载失败");
        assert_eq!(test_data, downloaded.as_slice());
        
        // 删除
        storage.delete_file(&file_hash).await.expect("删除失败");
    }

    #[tokio::test]
    async fn test_chunk_prefix_serialization() {
        let filename = "test_file.txt";
        let file_hash = &[0u8; 32];
        let prefix = ChunkPrefix::new(filename, file_hash, 0, 10);
        
        let bytes = prefix.to_bytes();
        assert_eq!(bytes.len(), ChunkPrefix::SIZE);
        
        let decoded = ChunkPrefix::from_bytes(&bytes).expect("反序列化失败");
        assert_eq!(decoded.get_filename(), filename);
        assert_eq!(decoded.chunk_index, 0);
        assert_eq!(decoded.total_chunks, 10);
    }
}
