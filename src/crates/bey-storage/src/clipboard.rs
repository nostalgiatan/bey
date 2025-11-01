//! # 剪切板同步模块
//!
//! 提供剪切板数据的同步功能，支持差异同步、群组同步、点对点同步。
//! 使用sled数据库进行持久化存储，通过bey-net模块进行实时同步。

use error::{ErrorInfo, ErrorCategory};
use sled::Db;
use std::sync::Arc;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

/// 剪切板同步结果类型
pub type ClipboardResult<T> = std::result::Result<T, ErrorInfo>;

/// 剪切板条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipboardEntry {
    /// 条目ID
    pub id: String,
    /// 内容
    pub content: Vec<u8>,
    /// 内容类型（text, image, file等）
    pub content_type: String,
    /// 来源设备DNS ID
    pub source_device_id: String,
    /// 时间戳
    pub timestamp: u64,
    /// 版本号（用于冲突解决）
    pub version: u64,
}

/// 剪切板同步模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// 点对点同步
    PeerToPeer,
    /// 群组同步
    Group,
    /// 广播（所有设备）
    Broadcast,
}

/// 剪切板同步事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardEvent {
    /// 新增条目
    Add(ClipboardEntry),
    /// 更新条目
    Update(ClipboardEntry),
    /// 删除条目
    Delete { id: String, timestamp: u64 },
    /// 请求完整同步
    RequestFullSync { requester_id: String },
    /// 完整同步响应
    FullSyncResponse { entries: Vec<ClipboardEntry> },
}

/// 剪切板管理器
pub struct ClipboardManager {
    /// 本地设备ID
    device_id: String,
    /// sled数据库
    db: Arc<Db>,
    /// 最大条目数
    max_entries: usize,
}

impl ClipboardManager {
    /// 创建新的剪切板管理器
    ///
    /// # 参数
    ///
    /// * `device_id` - 本地设备ID
    /// * `db_path` - sled数据库路径
    ///
    /// # 返回值
    ///
    /// 返回剪切板管理器实例或错误
    pub async fn new(device_id: String, db_path: std::path::PathBuf) -> ClipboardResult<Self> {
        // 创建数据库目录
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| ErrorInfo::new(6201, format!("创建数据库目录失败: {}", e))
                    .with_category(ErrorCategory::FileSystem))?;
        }

        // 打开sled数据库
        let db = sled::open(&db_path)
            .map_err(|e| ErrorInfo::new(6202, format!("打开数据库失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        info!("剪切板管理器初始化成功");
        Ok(Self {
            device_id,
            db: Arc::new(db),
            max_entries: 1000,
        })
    }

    /// 添加剪切板条目
    ///
    /// # 参数
    ///
    /// * `content` - 内容
    /// * `content_type` - 内容类型
    ///
    /// # 返回值
    ///
    /// 返回条目ID或错误
    pub async fn add_entry(&self, content: Vec<u8>, content_type: String) -> ClipboardResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = ClipboardEntry {
            id: id.clone(),
            content,
            content_type,
            source_device_id: self.device_id.clone(),
            timestamp,
            version: 1,
        };

        // 序列化并存储
        let entry_bytes = serde_json::to_vec(&entry)
            .map_err(|e| ErrorInfo::new(6203, format!("序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        self.db.insert(id.as_bytes(), entry_bytes)
            .map_err(|e| ErrorInfo::new(6204, format!("存储失败: {}", e))
                .with_category(ErrorCategory::Database))?;

        // 限制条目数量
        let count = self.db.len();
        if count > self.max_entries {
            // 删除最旧的条目
            if let Some(oldest_key) = self.find_oldest_entry_key() {
                let _ = self.db.remove(oldest_key);
            }
        }

        debug!("添加剪切板条目: {}", id);
        Ok(id)
    }

    /// 获取剪切板条目
    ///
    /// # 参数
    ///
    /// * `id` - 条目ID
    ///
    /// # 返回值
    ///
    /// 返回条目或错误
    pub async fn get_entry(&self, id: &str) -> ClipboardResult<ClipboardEntry> {
        let entry_bytes = self.db.get(id.as_bytes())
            .map_err(|e| ErrorInfo::new(6205, format!("查询失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6206, format!("剪切板条目不存在: {}", id))
                .with_category(ErrorCategory::Storage))?;

        let entry: ClipboardEntry = serde_json::from_slice(&entry_bytes)
            .map_err(|e| ErrorInfo::new(6207, format!("反序列化失败: {}", e))
                .with_category(ErrorCategory::Parse))?;

        Ok(entry)
    }

    /// 获取最新的剪切板条目
    ///
    /// # 返回值
    ///
    /// 返回最新条目或None
    pub async fn get_latest(&self) -> Option<ClipboardEntry> {
        let mut latest: Option<ClipboardEntry> = None;
        let mut latest_timestamp = 0u64;

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(entry) = serde_json::from_slice::<ClipboardEntry>(&value) {
                    if entry.timestamp > latest_timestamp {
                        latest_timestamp = entry.timestamp;
                        latest = Some(entry);
                    }
                }
            }
        }

        latest
    }

    /// 获取所有剪切板条目
    ///
    /// # 返回值
    ///
    /// 返回所有条目的列表
    pub async fn list_entries(&self) -> Vec<ClipboardEntry> {
        let mut entries = Vec::new();

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(entry) = serde_json::from_slice::<ClipboardEntry>(&value) {
                    entries.push(entry);
                }
            }
        }

        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries
    }

    /// 删除剪切板条目
    ///
    /// # 参数
    ///
    /// * `id` - 条目ID
    ///
    /// # 返回值
    ///
    /// 返回删除结果
    pub async fn delete_entry(&self, id: &str) -> ClipboardResult<()> {
        self.db.remove(id.as_bytes())
            .map_err(|e| ErrorInfo::new(6208, format!("删除失败: {}", e))
                .with_category(ErrorCategory::Database))?
            .ok_or_else(|| ErrorInfo::new(6209, format!("剪切板条目不存在: {}", id))
                .with_category(ErrorCategory::Storage))?;

        debug!("删除剪切板条目: {}", id);
        Ok(())
    }

    /// 处理同步事件
    ///
    /// # 参数
    ///
    /// * `event` - 同步事件
    ///
    /// # 返回值
    ///
    /// 返回处理结果
    pub async fn handle_sync_event(&self, event: ClipboardEvent) -> ClipboardResult<()> {
        match event {
            ClipboardEvent::Add(entry) => {
                self.merge_entry(entry).await?;
            }
            ClipboardEvent::Update(entry) => {
                self.merge_entry(entry).await?;
            }
            ClipboardEvent::Delete { id, .. } => {
                let _ = self.delete_entry(&id).await;
            }
            ClipboardEvent::RequestFullSync { requester_id: _ } => {
                // 这里应该通过net模块发送完整同步响应
                debug!("收到完整同步请求");
            }
            ClipboardEvent::FullSyncResponse { entries } => {
                for entry in entries {
                    self.merge_entry(entry).await?;
                }
            }
        }
        Ok(())
    }

    /// 合并远程条目（冲突解决）
    async fn merge_entry(&self, remote_entry: ClipboardEntry) -> ClipboardResult<()> {
        // 尝试获取本地条目
        let should_update = if let Ok(local_entry) = self.get_entry(&remote_entry.id).await {
            // 条目已存在，比较版本
            if remote_entry.version > local_entry.version {
                true
            } else if remote_entry.version == local_entry.version 
                && remote_entry.timestamp > local_entry.timestamp {
                true
            } else {
                false
            }
        } else {
            // 新条目
            true
        };

        if should_update {
            let entry_bytes = serde_json::to_vec(&remote_entry)
                .map_err(|e| ErrorInfo::new(6210, format!("序列化失败: {}", e))
                    .with_category(ErrorCategory::Parse))?;

            self.db.insert(remote_entry.id.as_bytes(), entry_bytes)
                .map_err(|e| ErrorInfo::new(6211, format!("存储失败: {}", e))
                    .with_category(ErrorCategory::Database))?;

            debug!("合并剪切板条目: {}", remote_entry.id);
        }

        Ok(())
    }

    /// 获取差异（自指定时间戳以来的变化）
    ///
    /// # 参数
    ///
    /// * `since_timestamp` - 起始时间戳
    ///
    /// # 返回值
    ///
    /// 返回差异条目列表
    pub async fn get_diff(&self, since_timestamp: u64) -> Vec<ClipboardEntry> {
        let mut diff = Vec::new();

        for item in self.db.iter() {
            if let Ok((_, value)) = item {
                if let Ok(entry) = serde_json::from_slice::<ClipboardEntry>(&value) {
                    if entry.timestamp > since_timestamp {
                        diff.push(entry);
                    }
                }
            }
        }

        diff
    }

    /// 查找最旧条目的键
    fn find_oldest_entry_key(&self) -> Option<Vec<u8>> {
        let mut oldest_key: Option<Vec<u8>> = None;
        let mut oldest_timestamp = u64::MAX;

        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                if let Ok(entry) = serde_json::from_slice::<ClipboardEntry>(&value) {
                    if entry.timestamp < oldest_timestamp {
                        oldest_timestamp = entry.timestamp;
                        oldest_key = Some(key.to_vec());
                    }
                }
            }
        }

        oldest_key
    }

    /// 清空所有条目
    pub async fn clear(&self) -> ClipboardResult<()> {
        self.db.clear()
            .map_err(|e| ErrorInfo::new(6212, format!("清空失败: {}", e))
                .with_category(ErrorCategory::Database))?;
        
        info!("清空剪切板");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_clipboard_manager_add_and_get() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("clipboard.db");
        
        let manager = ClipboardManager::new("device1".to_string(), db_path).await
            .expect("创建管理器失败");

        // 添加条目
        let id = manager.add_entry(b"Hello".to_vec(), "text".to_string()).await
            .expect("添加失败");

        // 获取条目
        let entry = manager.get_entry(&id).await.expect("获取失败");
        assert_eq!(entry.content, b"Hello");
        assert_eq!(entry.content_type, "text");
        assert_eq!(entry.source_device_id, "device1");
    }

    #[tokio::test]
    async fn test_clipboard_manager_persistence() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("clipboard.db");
        
        // 创建管理器并添加数据
        let id = {
            let manager = ClipboardManager::new("device1".to_string(), db_path.clone()).await
                .expect("创建管理器失败");
            manager.add_entry(b"Test".to_vec(), "text".to_string()).await
                .expect("添加失败")
        };

        // 创建新管理器，应该能加载之前的数据
        {
            let manager = ClipboardManager::new("device1".to_string(), db_path).await
                .expect("创建管理器失败");
            let entry = manager.get_entry(&id).await.expect("获取失败");
            assert_eq!(entry.content, b"Test");
        }
    }

    #[tokio::test]
    async fn test_clipboard_merge() {
        let temp_dir = tempdir().expect("创建临时目录失败");
        let db_path = temp_dir.path().join("clipboard.db");
        
        let manager = ClipboardManager::new("device1".to_string(), db_path).await
            .expect("创建管理器失败");

        // 添加本地条目
        let id = manager.add_entry(b"Local".to_vec(), "text".to_string()).await
            .expect("添加失败");

        // 模拟远程更新（版本更高）
        let remote_entry = ClipboardEntry {
            id: id.clone(),
            content: b"Remote Updated".to_vec(),
            content_type: "text".to_string(),
            source_device_id: "device2".to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            version: 2,
        };

        manager.handle_sync_event(ClipboardEvent::Update(remote_entry)).await
            .expect("处理事件失败");

        // 验证更新
        let entry = manager.get_entry(&id).await.expect("获取失败");
        assert_eq!(entry.content, b"Remote Updated");
        assert_eq!(entry.version, 2);
    }
}
