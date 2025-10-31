//! # 完整性校验器
//!
//! 负责文件传输过程中的数据完整性校验和验证。
//! 使用BLAKE3哈希算法确保数据在传输过程中未被篡改或损坏。
//!
//! ## 核心功能
//!
//! - **哈希校验**: 使用BLAKE3算法计算和验证文件哈希
//! - **块级验证**: 支持文件块的独立完整性校验
//! - **增量验证**: 支持增量数据完整性检查
//! - **校验缓存**: 高效的校验结果缓存机制
//! - **错误检测**: 精确的数据损坏和篡改检测

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, instrument};
use dashmap::DashMap;
use blake3::Hasher;
use crate::{TransferConfig, TransferResult, ChunkInfo};

/// 完整性校验器
///
/// 负责文件传输过程中的数据完整性校验和验证。
/// 使用BLAKE3哈希算法确保数据完整性。
#[derive(Debug)]
pub struct IntegrityChecker {
    /// 哈希缓存
    hash_cache: Arc<DashMap<String, CachedHash>>,
    /// 校验结果缓存
    verification_cache: Arc<RwLock<HashMap<String, VerificationResult>>>,
    /// 配置信息
    config: Arc<TransferConfig>,
    /// 校验统计信息
    statistics: Arc<IntegrityStatistics>,
}

/// 缓存的哈希值
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHash {
    /// 文件路径
    file_path: PathBuf,
    /// 文件大小
    file_size: u64,
    /// 文件修改时间
    modified_time: SystemTime,
    /// BLAKE3哈希值
    hash: String,
    /// 块哈希映射
    chunk_hashes: HashMap<usize, String>,
    /// 缓存创建时间
    cached_at: SystemTime,
}

/// 校验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerificationResult {
    /// 校验是否通过
    is_valid: bool,
    /// 校验时间
    verified_at: SystemTime,
    /// 错误信息
    error_message: Option<String>,
    /// 校验详情
    details: VerificationDetails,
}

/// 校验详情
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerificationDetails {
    /// 校验的块数量
    chunks_verified: usize,
    /// 成功的块数量
    chunks_valid: usize,
    /// 失败的块数量
    chunks_invalid: usize,
    /// 校验耗时（毫秒）
    verification_time_ms: u64,
    /// 校验算法
    algorithm: String,
}

/// 完整性统计信息
#[derive(Debug, Default)]
struct IntegrityStatistics {
    /// 总校验次数
    total_verifications: Arc<std::sync::atomic::AtomicU64>,
    /// 成功校验次数
    successful_verifications: Arc<std::sync::atomic::AtomicU64>,
    /// 失败校验次数
    failed_verifications: Arc<std::sync::atomic::AtomicU64>,
    /// 总校验字节数
    total_bytes_verified: Arc<std::sync::atomic::AtomicU64>,
    /// 平均校验速度（字节/秒）
    average_verification_speed: Arc<std::sync::atomic::AtomicU64>,
}

/// 完整性校验报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    /// 文件路径
    pub file_path: PathBuf,
    /// 文件大小
    pub file_size: u64,
    /// 文件哈希
    pub file_hash: String,
    /// 块级校验结果
    pub chunk_results: Vec<ChunkVerificationResult>,
    /// 整体验证结果
    pub is_valid: bool,
    /// 校验时间
    pub verified_at: SystemTime,
    /// 校验耗时（毫秒）
    pub verification_time_ms: u64,
}

/// 块校验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkVerificationResult {
    /// 块索引
    pub chunk_index: usize,
    /// 块偏移量
    pub chunk_offset: u64,
    /// 块大小
    pub chunk_size: usize,
    /// 期望哈希
    pub expected_hash: String,
    /// 实际哈希
    pub actual_hash: String,
    /// 校验是否通过
    pub is_valid: bool,
}

impl IntegrityChecker {
    /// 创建新的完整性校验器
    ///
    /// # 参数
    ///
    /// * `config` - 传输配置
    ///
    /// # 返回
    ///
    /// 返回完整性校验器实例
    #[instrument(skip(config))]
    pub fn new(config: Arc<TransferConfig>) -> Self {
        info!("创建完整性校验器");

        Self {
            hash_cache: Arc::new(DashMap::new()),
            verification_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            statistics: Arc::new(IntegrityStatistics::default()),
        }
    }

    /// 计算文件哈希
    ///
    /// # 参数
    ///
    /// * `file_path` - 文件路径
    ///
    /// # 返回
    ///
    /// 返回文件哈希值或错误信息
    #[instrument(skip(self), fields(file_path = %file_path.display()))]
    pub async fn calculate_file_hash(&self, file_path: &Path) -> TransferResult<String> {
        info!("计算文件哈希: {}", file_path.display());

        let start_time = SystemTime::now();

        // 检查缓存
        if let Some(cached_hash) = self.get_cached_hash(file_path).await? {
            info!("使用缓存的文件哈希: {}", cached_hash.hash);
            return Ok(cached_hash.hash);
        }

        // 打开文件
        let mut file = File::open(file_path).await.map_err(|e| {
            error!("打开文件失败: {}", e);
            ErrorInfo::new(
                7401,
                format!("打开文件失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 获取文件元数据
        let metadata = file.metadata().await.map_err(|e| {
            error!("获取文件元数据失败: {}", e);
            ErrorInfo::new(
                7402,
                format!("获取文件元数据失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        let file_size = metadata.len();
        let modified_time = metadata.modified().unwrap_or(UNIX_EPOCH);

        // 计算哈希
        let mut hasher = Hasher::new();
        let mut buffer = vec![0u8; self.config.buffer_size];
        let mut total_bytes_read = 0u64;

        loop {
            let bytes_read = file.read(&mut buffer).await.map_err(|e| {
                error!("读取文件失败: {}", e);
                ErrorInfo::new(
                    7403,
                    format!("读取文件失败: {}", e)
                )
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Error)
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            total_bytes_read += bytes_read as u64;
        }

        let hash = hasher.finalize();
        let hash_string = hash.to_hex().to_string();

        // 缓存结果
        self.cache_hash(file_path, file_size, modified_time, &hash_string, HashMap::new()).await;

        // 更新统计信息
        self.update_statistics(total_bytes_read, SystemTime::now().duration_since(start_time).unwrap_or_default()).await;

        info!("文件哈希计算完成: {} -> {}", file_path.display(), hash_string);
        Ok(hash_string)
    }

    /// 计算数据块哈希
    ///
    /// # 参数
    ///
    /// * `data` - 数据块
    ///
    /// # 返回
    ///
    /// 返回数据块哈希值
    #[instrument(skip(self, data), fields(data_size = data.len()))]
    pub async fn calculate_chunk_hash(&self, data: &[u8]) -> String {
        debug!("计算数据块哈希，数据大小: {} 字节", data.len());

        let mut hasher = Hasher::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let hash_string = hash.to_hex().to_string();

        debug!("数据块哈希计算完成: {}", hash_string);
        hash_string
    }

    /// 验证文件完整性
    ///
    /// # 参数
    ///
    /// * `file_path` - 文件路径
    /// * `expected_hash` - 期望的文件哈希
    ///
    /// # 返回
    ///
    /// 返回验证结果
    #[instrument(skip(self), fields(file_path = %file_path.display(), expected_hash))]
    pub async fn verify_file_integrity(&self, file_path: &Path, expected_hash: &str) -> TransferResult<bool> {
        info!("验证文件完整性: {} (期望哈希: {})", file_path.display(), expected_hash);

        let start_time = SystemTime::now();

        // 计算实际哈希
        let actual_hash = self.calculate_file_hash(file_path).await?;

        // 比较哈希值
        let is_valid = actual_hash == expected_hash;

        // 缓存验证结果
        let verification_result = VerificationResult {
            is_valid,
            verified_at: SystemTime::now(),
            error_message: if !is_valid {
                Some(format!("哈希不匹配，期望: {}, 实际: {}", expected_hash, actual_hash))
            } else {
                None
            },
            details: VerificationDetails {
                chunks_verified: 1,
                chunks_valid: if is_valid { 1 } else { 0 },
                chunks_invalid: if !is_valid { 1 } else { 0 },
                verification_time_ms: SystemTime::now().duration_since(start_time).unwrap_or_default().as_millis() as u64,
                algorithm: "BLAKE3".to_string(),
            },
        };

        let cache_key = format!("{}:{}", file_path.display(), expected_hash);
        self.verification_cache.write().await.insert(cache_key, verification_result);

        // 更新统计信息
        self.statistics.total_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if is_valid {
            self.statistics.successful_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.statistics.failed_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        if is_valid {
            info!("文件完整性验证通过: {}", file_path.display());
        } else {
            warn!("文件完整性验证失败: {} (哈希不匹配)", file_path.display());
        }

        Ok(is_valid)
    }

    /// 验证数据块完整性
    ///
    /// # 参数
    ///
    /// * `chunk_info` - 数据块信息
    /// * `data` - 数据块内容
    ///
    /// # 返回
    ///
    /// 返回验证结果
    #[instrument(skip(self, chunk_info, data), fields(chunk_index = chunk_info.index))]
    pub async fn verify_chunk_integrity(&self, chunk_info: &ChunkInfo, data: &[u8]) -> TransferResult<bool> {
        debug!("验证数据块完整性，块索引: {}, 期望哈希: {}", chunk_info.index, chunk_info.hash);

        // 验证数据大小
        if data.len() != chunk_info.size {
            warn!("数据块大小不匹配，期望: {}, 实际: {}", chunk_info.size, data.len());
            return Ok(false);
        }

        // 计算实际哈希
        let actual_hash = self.calculate_chunk_hash(data).await;

        // 比较哈希值
        let is_valid = actual_hash == chunk_info.hash;

        if is_valid {
            debug!("数据块完整性验证通过，块索引: {}", chunk_info.index);
        } else {
            warn!("数据块完整性验证失败，块索引: {} (哈希不匹配)", chunk_info.index);
        }

        Ok(is_valid)
    }

    /// 验证传输的完整性
    ///
    /// # 参数
    ///
    /// * `file_path` - 文件路径
    /// * `chunks` - 数据块信息列表
    ///
    /// # 返回
    ///
    /// 返回完整性校验报告
    #[instrument(skip(self, file_path, chunks), fields(file_path = %file_path.display(), chunk_count = chunks.len()))]
    pub async fn verify_transfer_integrity(
        &self,
        file_path: &Path,
        chunks: &[ChunkInfo],
    ) -> TransferResult<IntegrityReport> {
        info!("验证传输完整性，文件: {}, 块数量: {}", file_path.display(), chunks.len());

        let start_time = SystemTime::now();
        let mut chunk_results = Vec::new();
        let mut chunks_valid = 0;
        let mut chunks_invalid = 0;

        // 打开文件
        let mut file = File::open(file_path).await.map_err(|e| {
            error!("打开文件失败: {}", e);
            ErrorInfo::new(
                7404,
                format!("打开文件失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        // 获取文件信息
        let metadata = file.metadata().await.map_err(|e| {
            ErrorInfo::new(
                7405,
                format!("获取文件元数据失败: {}", e)
            )
            .with_category(ErrorCategory::Storage)
            .with_severity(ErrorSeverity::Error)
        })?;

        let file_size = metadata.len();

        // 验证每个数据块
        for chunk_info in chunks {
            // 定位到块位置
            file.seek(std::io::SeekFrom::Start(chunk_info.offset)).await.map_err(|e| {
                error!("文件定位失败: {}", e);
                ErrorInfo::new(
                    7406,
                    format!("文件定位失败: {}", e)
                )
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Error)
            })?;

            // 读取块数据
            let mut buffer = vec![0u8; chunk_info.size];
            file.read_exact(&mut buffer).await.map_err(|e| {
                error!("读取数据块失败: {}", e);
                ErrorInfo::new(
                    7407,
                    format!("读取数据块失败: {}", e)
                )
                .with_category(ErrorCategory::Storage)
                .with_severity(ErrorSeverity::Error)
            })?;

            // 验证块完整性
            let actual_hash = self.calculate_chunk_hash(&buffer).await;
            let is_valid = actual_hash == chunk_info.hash;

            if is_valid {
                chunks_valid += 1;
            } else {
                chunks_invalid += 1;
            }

            chunk_results.push(ChunkVerificationResult {
                chunk_index: chunk_info.index,
                chunk_offset: chunk_info.offset,
                chunk_size: chunk_info.size,
                expected_hash: chunk_info.hash.clone(),
                actual_hash,
                is_valid,
            });
        }

        // 计算文件哈希
        let file_hash = self.calculate_file_hash(file_path).await?;

        let verification_time = SystemTime::now().duration_since(start_time).unwrap_or_default();
        let is_fully_valid = chunks_invalid == 0;

        let report = IntegrityReport {
            file_path: file_path.to_path_buf(),
            file_size,
            file_hash,
            chunk_results,
            is_valid: is_fully_valid,
            verified_at: SystemTime::now(),
            verification_time_ms: verification_time.as_millis() as u64,
        };

        // 更新统计信息
        self.statistics.total_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if is_fully_valid {
            self.statistics.successful_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.statistics.failed_verifications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        info!("传输完整性验证完成，文件: {}, 有效块: {}/{}, 耗时: {}ms",
              file_path.display(), chunks_valid, chunks.len(), verification_time.as_millis());

        Ok(report)
    }

    /// 获取完整性统计信息
    ///
    /// # 返回
    ///
    /// 返回完整性校验统计信息
    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> IntegrityStatisticsSnapshot {
        IntegrityStatisticsSnapshot {
            total_verifications: self.statistics.total_verifications.load(std::sync::atomic::Ordering::Relaxed),
            successful_verifications: self.statistics.successful_verifications.load(std::sync::atomic::Ordering::Relaxed),
            failed_verifications: self.statistics.failed_verifications.load(std::sync::atomic::Ordering::Relaxed),
            total_bytes_verified: self.statistics.total_bytes_verified.load(std::sync::atomic::Ordering::Relaxed),
            average_verification_speed: self.statistics.average_verification_speed.load(std::sync::atomic::Ordering::Relaxed),
            cache_size: self.hash_cache.len(),
        }
    }

    /// 清理过期的缓存
    ///
    /// # 返回
    ///
    /// 返回清理的缓存条目数量
    #[instrument(skip(self))]
    pub async fn cleanup_cache(&self) -> usize {
        info!("开始清理过期的完整性校验缓存");

        let mut expired_keys = Vec::new();
        let current_time = SystemTime::now();
        let expiration_threshold = Duration::from_secs(3600); // 1小时过期

        // 清理哈希缓存
        for entry in self.hash_cache.iter() {
            if let Ok(elapsed) = current_time.duration_since(entry.value().cached_at) {
                if elapsed > expiration_threshold {
                    expired_keys.push(entry.key().clone());
                }
            }
        }

        for key in &expired_keys {
            self.hash_cache.remove(key);
        }

        let cleaned_count = expired_keys.len();

        // 清理校验结果缓存
        let mut verification_cache = self.verification_cache.write().await;
        let mut expired_verification_keys = Vec::new();

        for (key, result) in verification_cache.iter() {
            if let Ok(elapsed) = current_time.duration_since(result.verified_at) {
                if elapsed > expiration_threshold {
                    expired_verification_keys.push(key.clone());
                }
            }
        }

        for key in &expired_verification_keys {
            verification_cache.remove(key);
        }

        info!("缓存清理完成，删除了 {} 个哈希缓存和 {} 个校验结果缓存",
              cleaned_count, expired_verification_keys.len());

        cleaned_count + expired_verification_keys.len()
    }

    // 私有方法

    /// 获取缓存的哈希
    async fn get_cached_hash(&self, file_path: &Path) -> TransferResult<Option<CachedHash>> {
        let path_str = file_path.to_string_lossy().to_string();

        if let Some(cached_hash) = self.hash_cache.get(&path_str) {
            // 检查缓存是否仍然有效
            if let Ok(metadata) = tokio::fs::metadata(file_path).await {
                if let Ok(modified_time) = metadata.modified() {
                    if cached_hash.modified_time == modified_time && cached_hash.file_size == metadata.len() {
                        return Ok(Some(cached_hash.clone()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// 缓存哈希值
    async fn cache_hash(
        &self,
        file_path: &Path,
        file_size: u64,
        modified_time: SystemTime,
        hash: &str,
        chunk_hashes: HashMap<usize, String>,
    ) {
        let path_str = file_path.to_string_lossy().to_string();

        let cached_hash = CachedHash {
            file_path: file_path.to_path_buf(),
            file_size,
            modified_time,
            hash: hash.to_string(),
            chunk_hashes,
            cached_at: SystemTime::now(),
        };

        self.hash_cache.insert(path_str, cached_hash);
    }

    /// 更新统计信息
    async fn update_statistics(&self, bytes_verified: u64, duration: Duration) {
        self.statistics.total_bytes_verified.fetch_add(bytes_verified, std::sync::atomic::Ordering::Relaxed);

        if duration.as_secs() > 0 {
            let speed = bytes_verified / duration.as_secs();
            self.statistics.average_verification_speed.store(speed, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// 完整性统计信息快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityStatisticsSnapshot {
    /// 总校验次数
    pub total_verifications: u64,
    /// 成功校验次数
    pub successful_verifications: u64,
    /// 失败校验次数
    pub failed_verifications: u64,
    /// 总校验字节数
    pub total_bytes_verified: u64,
    /// 平均校验速度（字节/秒）
    pub average_verification_speed: u64,
    /// 缓存大小
    pub cache_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInfo, TransferConfig};
    use std::sync::Arc;
    use std::time::SystemTime;
    use tempfile::NamedTempFile;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_integrity_checker_creation() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);
        assert_eq!(checker.hash_cache.len(), 0);
    }

    #[tokio::test]
    async fn test_calculate_file_hash() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = "This is test data for hash calculation".as_bytes();
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // 计算哈希
        let hash1 = checker.calculate_file_hash(temp_file.path()).await.unwrap();
        let hash2 = checker.calculate_file_hash(temp_file.path()).await.unwrap();

        // 验证哈希一致性
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
        assert_eq!(hash1.len(), 64); // BLAKE3哈希长度
    }

    #[tokio::test]
    async fn test_calculate_chunk_hash() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        let data = "test data for hash calculation".as_bytes();
        let hash1 = checker.calculate_chunk_hash(data).await;
        let hash2 = checker.calculate_chunk_hash(data).await;

        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
        assert_eq!(hash1.len(), 64);
    }

    #[tokio::test]
    async fn test_verify_file_integrity() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"File integrity verification test data";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // 计算正确哈希
        let correct_hash = checker.calculate_file_hash(temp_file.path()).await.unwrap();

        // 验证正确哈希
        let is_valid = checker.verify_file_integrity(temp_file.path(), &correct_hash).await.unwrap();
        assert!(is_valid);

        // 验证错误哈希
        let is_valid = checker.verify_file_integrity(temp_file.path(), "wrong_hash").await.unwrap();
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_verify_chunk_integrity() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        let data = b"Chunk integrity verification test";
        let hash = checker.calculate_chunk_hash(data).await;

        let chunk_info = ChunkInfo {
            index: 0,
            offset: 0,
            size: data.len(),
            hash: hash.clone(),
            timestamp: SystemTime::now(),
        };

        // 验证正确数据
        let is_valid = checker.verify_chunk_integrity(&chunk_info, data).await.unwrap();
        assert!(is_valid);

        // 验证错误数据
        let wrong_data = b"Wrong data content";
        let is_valid = checker.verify_chunk_integrity(&chunk_info, wrong_data).await.unwrap();
        assert!(!is_valid);
    }

    #[tokio::test]
    async fn test_verify_transfer_integrity() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Transfer integrity verification test data with multiple chunks";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // 创建数据块信息
        let chunk_size = 10;
        let mut chunks = Vec::new();
        for (i, chunk) in test_data.chunks(chunk_size).enumerate() {
            let hash = checker.calculate_chunk_hash(chunk).await;
            chunks.push(ChunkInfo {
                index: i,
                offset: (i * chunk_size) as u64,
                size: chunk.len(),
                hash,
                timestamp: SystemTime::now(),
            });
        }

        // 验证传输完整性
        let report = checker.verify_transfer_integrity(temp_file.path(), &chunks).await.unwrap();
        assert!(report.is_valid);
        assert_eq!(report.chunk_results.len(), chunks.len());
        assert_eq!(report.file_size, test_data.len() as u64);

        // 验证所有块都通过验证
        for chunk_result in &report.chunk_results {
            assert!(chunk_result.is_valid);
        }
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        // 创建临时文件并执行一些校验操作
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Statistics test data";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let hash = checker.calculate_file_hash(temp_file.path()).await.unwrap();
        checker.verify_file_integrity(temp_file.path(), &hash).await.unwrap();

        // 获取统计信息
        let stats = checker.get_statistics().await;
        assert_eq!(stats.total_verifications, 1);
        assert_eq!(stats.successful_verifications, 1);
        assert_eq!(stats.failed_verifications, 0);
        assert!(stats.total_bytes_verified > 0);
    }

    #[tokio::test]
    async fn test_cleanup_cache() {
        let config = Arc::new(TransferConfig::default());
        let checker = IntegrityChecker::new(config);

        // 创建临时文件并生成缓存
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Cache cleanup test data";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // 生成缓存条目
        let _hash = checker.calculate_file_hash(temp_file.path()).await.unwrap();
        assert_eq!(checker.hash_cache.len(), 1);

        // 清理缓存（由于缓存是新的，不应该被清理）
        let cleaned_count = checker.cleanup_cache().await;
        assert_eq!(cleaned_count, 0);
        assert_eq!(checker.hash_cache.len(), 1);
    }
}