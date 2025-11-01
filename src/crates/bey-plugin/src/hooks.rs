//! # 钩子系统模块
//!
//! 提供在关键处理流程中插入自定义逻辑的能力

use dashmap::DashMap;
use std::sync::Arc;
use async_trait::async_trait;
use error::{ErrorInfo, ErrorCategory};
use tracing::debug;

/// 钩子结果类型
pub type HookResult<T> = std::result::Result<T, ErrorInfo>;

/// 钩子点定义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPoint {
    // 网络层钩子
    /// 消息发送前
    NetworkBeforeSend,
    /// 消息发送后
    NetworkAfterSend,
    /// 消息接收前
    NetworkBeforeReceive,
    /// 消息接收后
    NetworkAfterReceive,
    /// 连接建立
    NetworkConnectionEstablished,
    /// 连接关闭
    NetworkConnectionClosed,
    
    // 存储层钩子
    /// 数据写入前
    StorageBeforeWrite,
    /// 数据写入后
    StorageAfterWrite,
    /// 数据读取前
    StorageBeforeRead,
    /// 数据读取后
    StorageAfterRead,
    /// 数据删除前
    StorageBeforeDelete,
    /// 数据删除后
    StorageAfterDelete,
    
    // 消息层钩子
    /// 消息发送前
    MessageBeforeSend,
    /// 消息发送后
    MessageAfterSend,
    /// 消息接收
    MessageReceived,
    /// 消息处理完成
    MessageProcessed,
    
    // 剪切板钩子
    /// 同步前
    ClipboardBeforeSync,
    /// 同步后
    ClipboardAfterSync,
    /// 条目添加
    ClipboardEntryAdded,
    /// 条目删除
    ClipboardEntryDeleted,
    
    // 云存储钩子
    /// 文件上传前
    CloudStorageBeforeUpload,
    /// 文件上传后
    CloudStorageAfterUpload,
    /// 文件下载前
    CloudStorageBeforeDownload,
    /// 文件下载后
    CloudStorageAfterDownload,
}

impl HookPoint {
    /// 获取钩子点名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::NetworkBeforeSend => "network.before_send",
            Self::NetworkAfterSend => "network.after_send",
            Self::NetworkBeforeReceive => "network.before_receive",
            Self::NetworkAfterReceive => "network.after_receive",
            Self::NetworkConnectionEstablished => "network.connection_established",
            Self::NetworkConnectionClosed => "network.connection_closed",
            
            Self::StorageBeforeWrite => "storage.before_write",
            Self::StorageAfterWrite => "storage.after_write",
            Self::StorageBeforeRead => "storage.before_read",
            Self::StorageAfterRead => "storage.after_read",
            Self::StorageBeforeDelete => "storage.before_delete",
            Self::StorageAfterDelete => "storage.after_delete",
            
            Self::MessageBeforeSend => "message.before_send",
            Self::MessageAfterSend => "message.after_send",
            Self::MessageReceived => "message.received",
            Self::MessageProcessed => "message.processed",
            
            Self::ClipboardBeforeSync => "clipboard.before_sync",
            Self::ClipboardAfterSync => "clipboard.after_sync",
            Self::ClipboardEntryAdded => "clipboard.entry_added",
            Self::ClipboardEntryDeleted => "clipboard.entry_deleted",
            
            Self::CloudStorageBeforeUpload => "cloud_storage.before_upload",
            Self::CloudStorageAfterUpload => "cloud_storage.after_upload",
            Self::CloudStorageBeforeDownload => "cloud_storage.before_download",
            Self::CloudStorageAfterDownload => "cloud_storage.after_download",
        }
    }
}

/// 钩子处理器特征
#[async_trait]
pub trait Hook: Send + Sync {
    /// 执行钩子
    ///
    /// # 参数
    ///
    /// * `data` - 钩子数据
    ///
    /// # 返回值
    ///
    /// 返回修改后的数据或错误
    async fn execute(&self, data: Vec<u8>) -> HookResult<Vec<u8>>;
}

/// 钩子注册表
pub struct HookRegistry {
    /// 钩子表: 钩子点 -> 钩子处理器列表
    hooks: DashMap<HookPoint, Vec<Arc<dyn Hook>>>,
}

impl HookRegistry {
    /// 创建新的钩子注册表
    pub fn new() -> Self {
        Self {
            hooks: DashMap::new(),
        }
    }
    
    /// 注册钩子
    ///
    /// # 参数
    ///
    /// * `point` - 钩子点
    /// * `hook` - 钩子处理器
    pub fn register(&self, point: HookPoint, hook: Arc<dyn Hook>) {
        self.hooks
            .entry(point)
            .or_insert_with(Vec::new)
            .push(hook);
        
        debug!("注册钩子: {}", point.name());
    }
    
    /// 执行钩子链
    ///
    /// # 参数
    ///
    /// * `point` - 钩子点
    /// * `data` - 初始数据
    ///
    /// # 返回值
    ///
    /// 返回经过所有钩子处理后的数据
    pub async fn execute(&self, point: HookPoint, mut data: Vec<u8>) -> HookResult<Vec<u8>> {
        if let Some(hooks) = self.hooks.get(&point) {
            for hook in hooks.iter() {
                data = hook.execute(data).await
                    .map_err(|e| ErrorInfo::new(8100, format!("钩子执行失败: {}", e))
                        .with_category(ErrorCategory::System))?;
            }
        }
        Ok(data)
    }
    
    /// 获取钩子数量
    pub fn hook_count(&self, point: HookPoint) -> usize {
        self.hooks.get(&point).map(|h| h.len()).unwrap_or(0)
    }
    
    /// 清除所有钩子
    pub fn clear(&self) {
        self.hooks.clear();
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    struct TestHook {
        suffix: String,
    }
    
    #[async_trait]
    impl Hook for TestHook {
        async fn execute(&self, mut data: Vec<u8>) -> HookResult<Vec<u8>> {
            data.extend_from_slice(self.suffix.as_bytes());
            Ok(data)
        }
    }
    
    #[tokio::test]
    async fn test_hook_registry() {
        let registry = HookRegistry::new();
        
        let hook1 = Arc::new(TestHook { suffix: "_1".to_string() });
        let hook2 = Arc::new(TestHook { suffix: "_2".to_string() });
        
        registry.register(HookPoint::NetworkBeforeSend, hook1);
        registry.register(HookPoint::NetworkBeforeSend, hook2);
        
        let result = registry.execute(HookPoint::NetworkBeforeSend, b"test".to_vec()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"test_1_2");
    }
    
    #[test]
    fn test_hook_point_name() {
        assert_eq!(HookPoint::NetworkBeforeSend.name(), "network.before_send");
        assert_eq!(HookPoint::StorageAfterWrite.name(), "storage.after_write");
    }
}
