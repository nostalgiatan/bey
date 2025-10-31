//! # 智能压缩模块
//!
//! 提供自动大小判断的压缩策略，支持多种压缩算法和智能选择。

use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tracing::{debug, info};

/// 压缩算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// 无压缩
    None,
    /// LZ4快速压缩
    Lz4,
    /// Zstd标准压缩
    Zstd,
    /// Zstd最高压缩
    ZstdMax,
}

/// 压缩策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStrategy {
    /// 小文件阈值（字节）
    pub small_file_threshold: u64,
    /// 中等文件阈值
    pub medium_file_threshold: u64,
    /// 大文件阈值
    pub large_file_threshold: u64,
    /// 小文件压缩算法
    pub small_file_algorithm: CompressionAlgorithm,
    /// 中等文件压缩算法
    pub medium_file_algorithm: CompressionAlgorithm,
    /// 大文件压缩算法
    pub large_file_algorithm: CompressionAlgorithm,
    /// 压缩率阈值（低于此值不压缩）
    pub compression_ratio_threshold: f32,
    /// 最大压缩时间（毫秒）
    pub max_compression_time_ms: u64,
}

impl Default for CompressionStrategy {
    fn default() -> Self {
        Self {
            small_file_threshold: 1024,      // 1KB
            medium_file_threshold: 1024 * 1024, // 1MB
            large_file_threshold: 10 * 1024 * 1024, // 10MB
            small_file_algorithm: CompressionAlgorithm::None,
            medium_file_algorithm: CompressionAlgorithm::Lz4,
            large_file_algorithm: CompressionAlgorithm::Zstd,
            compression_ratio_threshold: 0.9, // 压缩率至少要10%
            max_compression_time_ms: 5000, // 5秒
        }
    }
}

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// 使用的压缩算法
    pub algorithm: CompressionAlgorithm,
    /// 原始大小
    pub original_size: u64,
    /// 压缩后大小
    pub compressed_size: u64,
    /// 压缩率
    pub compression_ratio: f32,
    /// 压缩耗时（毫秒）
    pub compression_time_ms: u64,
    /// 是否值得压缩
    pub is_beneficial: bool,
    /// 压缩后的数据（不序列化，仅用于内存传递）
    #[serde(skip)]
    pub compressed_data: Option<Vec<u8>>,
}

impl CompressionResult {
    /// 获取压缩后的数据
    pub fn get_compressed_data(&self) -> Vec<u8> {
        match &self.compressed_data {
            Some(data) => data.clone(),
            None => {
                // 对于None算法（CompressionAlgorithm::None），返回原始数据
                // 注意：这里调用者需要自行处理，因为压缩结果中没有原始数据
                // 在实际使用中，None算法的压缩结果应该使用原始数据
                vec![] // 返回空向量，调用者应该检查算法类型
            }
        }
    }
}

/// 智能压缩器
pub struct SmartCompressor {
    strategy: CompressionStrategy,
}

impl SmartCompressor {
    /// 创建新的智能压缩器
    pub fn new(strategy: CompressionStrategy) -> Self {
        Self { strategy }
    }

    /// 自动选择压缩算法
    pub fn select_algorithm(&self, file_size: u64, file_type: &str) -> CompressionAlgorithm {
        // 对于某些已经压缩的文件类型，跳过压缩
        if self.is_already_compressed(file_type) {
            return CompressionAlgorithm::None;
        }

        if file_size <= self.strategy.small_file_threshold {
            self.strategy.small_file_algorithm
        } else if file_size <= self.strategy.medium_file_threshold {
            self.strategy.medium_file_algorithm
        } else if file_size <= self.strategy.large_file_threshold {
            self.strategy.large_file_algorithm
        } else {
            // 超大文件使用快速压缩
            CompressionAlgorithm::Lz4
        }
    }

    /// 判断文件是否已经压缩
    fn is_already_compressed(&self, file_type: &str) -> bool {
        let compressed_types = [
            "zip", "rar", "7z", "gz", "bz2", "xz", "lz4", "zst",
            "jpg", "jpeg", "png", "gif", "webp", "mp3", "mp4",
            "avi", "mkv", "pdf", "docx", "xlsx"
        ];

        compressed_types.contains(&file_type.to_lowercase().as_str())
    }

    /// 压缩数据（同步版本）
    pub fn compress_sync(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<CompressionResult, ErrorInfo> {
        let start_time = std::time::Instant::now();
        let original_size = data.len() as u64;

        match algorithm {
            CompressionAlgorithm::None => {
                Ok(CompressionResult {
                    algorithm,
                    original_size,
                    compressed_size: original_size,
                    compression_ratio: 1.0,
                    compression_time_ms: 0,
                    is_beneficial: false,
                    compressed_data: None,
                })
            }
            CompressionAlgorithm::Lz4 => {
                let compressed = lz4_flex::block::compress(data);

                let compression_time = start_time.elapsed().as_millis() as u64;
                let compressed_size = compressed.len() as u64;
                let compression_ratio = compressed_size as f32 / original_size as f32;

                Ok(CompressionResult {
                    algorithm,
                    original_size,
                    compressed_size,
                    compression_ratio,
                    compression_time_ms: compression_time,
                    is_beneficial: compression_ratio < self.strategy.compression_ratio_threshold,
                    compressed_data: Some(compressed),
                })
            }
            CompressionAlgorithm::Zstd => {
                let compressed = zstd::encode_all(Cursor::new(data), 3)
                    .map_err(|e| ErrorInfo::new(7002, format!("Zstd压缩失败: {}", e))
                        .with_category(ErrorCategory::Compression)
                        .with_severity(ErrorSeverity::Error))?;

                let compression_time = start_time.elapsed().as_millis() as u64;
                let compressed_size = compressed.len() as u64;
                let compression_ratio = compressed_size as f32 / original_size as f32;

                Ok(CompressionResult {
                    algorithm,
                    original_size,
                    compressed_size,
                    compression_ratio,
                    compression_time_ms: compression_time,
                    is_beneficial: compression_ratio < self.strategy.compression_ratio_threshold,
                    compressed_data: Some(compressed),
                })
            }
            CompressionAlgorithm::ZstdMax => {
                let compressed = zstd::encode_all(Cursor::new(data), 22)
                    .map_err(|e| ErrorInfo::new(7003, format!("Zstd最大压缩失败: {}", e))
                        .with_category(ErrorCategory::Compression)
                        .with_severity(ErrorSeverity::Error))?;

                let compression_time = start_time.elapsed().as_millis() as u64;
                let compressed_size = compressed.len() as u64;
                let compression_ratio = compressed_size as f32 / original_size as f32;

                Ok(CompressionResult {
                    algorithm,
                    original_size,
                    compressed_size,
                    compression_ratio,
                    compression_time_ms: compression_time,
                    is_beneficial: compression_ratio < self.strategy.compression_ratio_threshold,
                    compressed_data: Some(compressed),
                })
            }
        }
    }

    /// 解压数据（同步版本）
    pub fn decompress_sync(&self, compressed_data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, ErrorInfo> {
        match algorithm {
            CompressionAlgorithm::None => Ok(compressed_data.to_vec()),
            CompressionAlgorithm::Lz4 => {
                // LZ4解压需要知道原始大小，我们可以使用一个足够大的缓冲区
                // 或者使用decompress_size_known函数
                let max_possible_size = compressed_data.len() * 100; // 100倍应该足够了
                lz4_flex::block::decompress(compressed_data, max_possible_size)
                    .map_err(|e| ErrorInfo::new(7004, format!("LZ4解压失败: {}", e))
                        .with_category(ErrorCategory::Compression)
                        .with_severity(ErrorSeverity::Error))
            }
            CompressionAlgorithm::Zstd | CompressionAlgorithm::ZstdMax => {
                let cursor = std::io::Cursor::new(compressed_data);
                zstd::decode_all(cursor)
                    .map_err(|e| ErrorInfo::new(7005, format!("Zstd解压失败: {}", e))
                        .with_category(ErrorCategory::Compression)
                        .with_severity(ErrorSeverity::Error))
            }
        }
    }

    /// 异步压缩数据
    pub async fn compress_async(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<CompressionResult, ErrorInfo> {
        let data = data.to_vec();
        let compressor = SmartCompressor::new(self.strategy.clone());

        tokio::task::spawn_blocking(move || {
            compressor.compress_sync(&data, algorithm)
        }).await.map_err(|e| ErrorInfo::new(7006, format!("压缩任务失败: {}", e))
            .with_category(ErrorCategory::System)
            .with_severity(ErrorSeverity::Error))?
    }

    /// 异步解压数据
    pub async fn decompress_async(&self, compressed_data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>, ErrorInfo> {
        let compressed_data = compressed_data.to_vec();
        let compressor = SmartCompressor::new(self.strategy.clone());

        tokio::task::spawn_blocking(move || {
            compressor.decompress_sync(&compressed_data, algorithm)
        }).await.map_err(|e| ErrorInfo::new(7007, format!("解压任务失败: {}", e))
            .with_category(ErrorCategory::System)
            .with_severity(ErrorSeverity::Error))?
    }

    /// 智能压缩（自动选择算法）
    pub async fn smart_compress(&self, data: &[u8], file_type: &str) -> Result<CompressionResult, ErrorInfo> {
        let file_size = data.len() as u64;
        let algorithm = self.select_algorithm(file_size, file_type);

        debug!("智能压缩: 文件大小={}, 文件类型={}, 选择算法={:?}",
               file_size, file_type, algorithm);

        let result = self.compress_async(data, algorithm).await?;

        if result.is_beneficial {
            info!("压缩成功: {} -> {} (压缩率: {:.2}%)",
                  result.original_size, result.compressed_size,
                  (1.0 - result.compression_ratio) * 100.0);
        } else {
            debug!("压缩无收益，跳过压缩");
        }

        Ok(result)
    }

    /// 获取压缩策略
    pub fn strategy(&self) -> &CompressionStrategy {
        &self.strategy
    }

    /// 更新压缩策略
    pub fn update_strategy(&mut self, strategy: CompressionStrategy) {
        self.strategy = strategy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_algorithm_selection() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());

        // 小文件
        assert_eq!(compressor.select_algorithm(512, "txt"), CompressionAlgorithm::None);

        // 中等文件
        assert_eq!(compressor.select_algorithm(100 * 1024, "txt"), CompressionAlgorithm::Lz4);

        // 大文件
        assert_eq!(compressor.select_algorithm(5 * 1024 * 1024, "txt"), CompressionAlgorithm::Zstd);

        // 超大文件
        assert_eq!(compressor.select_algorithm(50 * 1024 * 1024, "txt"), CompressionAlgorithm::Lz4);

        // 已压缩文件
        assert_eq!(compressor.select_algorithm(5 * 1024 * 1024, "zip"), CompressionAlgorithm::None);
    }

    #[tokio::test]
    async fn test_smart_compression() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());

        // 创建测试数据
        let test_data = "Hello, World! ".repeat(1000);
        let data = test_data.as_bytes();

        let result = compressor.smart_compress(data, "txt").await.unwrap();

        assert!(result.compressed_size <= result.original_size);
        assert!(result.compression_ratio <= 1.0);
    }

    #[test]
    fn test_lz4_compression() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());

        let test_data = "Hello, World! ".repeat(100);
        let data = test_data.as_bytes();

        let result = compressor.compress_sync(data, CompressionAlgorithm::Lz4).unwrap();

        // LZ4算法应该有压缩数据
        assert!(result.compressed_data.is_some());
        let compressed_data = result.get_compressed_data();
        let decompressed = compressor.decompress_sync(&compressed_data, CompressionAlgorithm::Lz4).unwrap();

        assert!(result.compressed_size < data.len() as u64);
        assert!(result.compression_ratio < 1.0);
        assert_eq!(decompressed, data);
    }
}