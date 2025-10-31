#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_smart_compressor_new() {
        let strategy = CompressionStrategy::default();
        let compressor = SmartCompressor::new(strategy);

        // 验证压缩器已正确初始化
        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_compressions, 0);
        assert_eq!(stats.total_decompressions, 0);
    }

    #[tokio::test]
    async fn test_compress_none_algorithm() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let data = b"test data";

        let result = compressor.compress(data, CompressionAlgorithm::None).await.unwrap();

        assert_eq!(result.algorithm, CompressionAlgorithm::None);
        assert_eq!(result.original_size, data.len() as u64);
        assert_eq!(result.compressed_size, data.len() as u64);
        assert_eq!(result.compression_ratio, 1.0);
        assert!(!result.is_beneficial);
    }

    #[tokio::test]
    async fn test_compress_decompress_consistency() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let original_data = b"This is test data that should be compressed and decompressed correctly.";

        // 测试所有算法的压缩和解压缩一致性
        let algorithms = [
            CompressionAlgorithm::Lz4,
            CompressionAlgorithm::Zstd,
            CompressionAlgorithm::ZstdMax,
        ];

        for algorithm in algorithms {
            // 压缩
            let compressed_result = compressor.compress(original_data, algorithm).await.unwrap();
            let compressed_data = compressed_result.get_compressed_data();

            // 解压缩
            let decompressed_data = compressor.decompress(&compressed_data, algorithm).await.unwrap();

            // 验证数据一致性
            assert_eq!(decompressed_data, original_data, "Algorithm: {:?}", algorithm);
        }
    }

    #[tokio::test]
    async fn test_compress_beneficial_calculation() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());

        // 可压缩的数据
        let compressible_data = b"aaaaabbbbbcccccdddddeeeeefffffggggghhhhhi".repeat(10);
        let result = compressor.compress(&compressible_data, CompressionAlgorithm::Lz4).await.unwrap();

        assert!(result.compressed_size < result.original_size);
        assert!(result.compression_ratio < 1.0);
        assert!(result.is_beneficial);

        // 不可压缩的数据（随机）
        let random_data = vec![0u8; 100];
        let result = compressor.compress(&random_data, CompressionAlgorithm::Lz4).await.unwrap();

        // LZ4可能会增加大小，但压缩器应该正确标记
        assert_eq!(result.algorithm, CompressionAlgorithm::Lz4);
    }

    #[tokio::test]
    async fn test_async_compress() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let data = b"test data for async compression";

        let result = compressor.compress_async(data, CompressionAlgorithm::Zstd).await.unwrap();

        assert_eq!(result.algorithm, CompressionAlgorithm::Zstd);
        assert!(result.compression_time_ms > 0);
    }

    #[tokio::test]
    async fn test_batch_compress() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let data_sets = vec![
            b"first data set".to_vec(),
            b"second data set".to_vec(),
            b"third data set".to_vec(),
        ];

        let results = compressor.batch_compress(&data_sets, CompressionAlgorithm::Lz4).await.unwrap();

        assert_eq!(results.len(), 3);
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.original_size, data_sets[i].len() as u64);
            assert_eq!(result.algorithm, CompressionAlgorithm::Lz4);
        }
    }

    #[tokio::test]
    async fn test_compression_stats() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let data = b"test data for statistics";

        // 执行一些压缩操作
        compressor.compress(data, CompressionAlgorithm::Lz4).await.unwrap();
        compressor.compress(data, CompressionAlgorithm::Zstd).await.unwrap();

        let decompressed_data = compressor.decompress(
            &compressor.compress(data, CompressionAlgorithm::Lz4).await.unwrap().get_compressed_data(),
            CompressionAlgorithm::Lz4
        ).await.unwrap();

        let stats = compressor.get_stats().await;
        assert_eq!(stats.total_compressions, 2);
        assert_eq!(stats.total_decompressions, 1);

        // 测试统计重置
        compressor.reset_stats().await;
        let reset_stats = compressor.get_stats().await;
        assert_eq!(reset_stats.total_compressions, 0);
        assert_eq!(reset_stats.total_decompressions, 0);
    }

    #[tokio::test]
    async fn test_decompress_invalid_data() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let invalid_data = vec![0u8; 10]; // 无效的压缩数据

        let result = compressor.decompress(&invalid_data, CompressionAlgorithm::Lz4).await;
        assert!(result.is_err());

        let result = compressor.decompress(&invalid_data, CompressionAlgorithm::Zstd).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_algorithm_display() {
        assert_eq!(format!("{}", CompressionAlgorithm::None), "none");
        assert_eq!(format!("{}", CompressionAlgorithm::Lz4), "lz4");
        assert_eq!(format!("{}", CompressionAlgorithm::Zstd), "zstd");
        assert_eq!(format!("{}", CompressionAlgorithm::ZstdMax), "zstd_max");
        assert_eq!(format!("{}", CompressionAlgorithm::Auto), "auto");
    }

    #[test]
    fn test_compression_strategy_default() {
        let strategy = CompressionStrategy::default();
        assert!(strategy.small_data_threshold > 0);
        assert!(strategy.large_data_threshold > strategy.small_data_threshold);
        assert!(strategy.auto_algorithm != CompressionAlgorithm::None);
    }

    #[test]
    fn test_compression_result_creation() {
        let result = CompressionResult {
            algorithm: CompressionAlgorithm::Zstd,
            original_size: 1000,
            compressed_size: 500,
            compression_ratio: 0.5,
            compression_time_ms: 10,
            is_beneficial: true,
        };

        assert_eq!(result.algorithm, CompressionAlgorithm::Zstd);
        assert_eq!(result.original_size, 1000);
        assert_eq!(result.compressed_size, 500);
        assert_eq!(result.compression_ratio, 0.5);
        assert_eq!(result.compression_time_ms, 10);
        assert!(result.is_beneficial);
    }

    #[tokio::test]
    async fn test_compression_with_empty_data() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let empty_data = b"";

        let result = compressor.compress(empty_data, CompressionAlgorithm::Lz4).await.unwrap();
        assert_eq!(result.original_size, 0);
        assert_eq!(result.compressed_size, 0);

        let decompressed = compressor.decompress(&result.get_compressed_data(), CompressionAlgorithm::Lz4).await.unwrap();
        assert_eq!(decompressed, empty_data);
    }

    #[tokio::test]
    async fn test_compression_with_large_data() {
        let compressor = SmartCompressor::new(CompressionStrategy::default());
        let large_data = vec![0u8; 1024 * 1024]; // 1MB

        let result = compressor.compress(&large_data, CompressionAlgorithm::Zstd).await.unwrap();
        assert!(result.compressed_size < result.original_size);
        assert!(result.is_beneficial);

        let decompressed = compressor.decompress(&result.get_compressed_data(), CompressionAlgorithm::Zstd).await.unwrap();
        assert_eq!(decompressed, large_data);
    }

    #[tokio::test]
    async fn test_concurrent_compression() {
        let compressor = std::sync::Arc::new(SmartCompressor::new(CompressionStrategy::default()));
        let mut handles = vec![];

        // 创建多个并发压缩任务
        for i in 0..10 {
            let compressor_clone = compressor.clone();
            let data = format!("test data {}", i).into_bytes();

            let handle = tokio::spawn(async move {
                compressor_clone.compress(&data, CompressionAlgorithm::Lz4).await
            });

            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }
}