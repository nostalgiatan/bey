//! # 事件发射器
//!
//! 负责将后端事件发送到前端 GUI

use std::sync::Arc;
use tauri::{AppHandle, Manager};
use serde::Serialize;

/// 事件发射器
///
/// 封装 Tauri 的事件发送功能，提供类型安全的事件发送接口
pub struct EventEmitter {
    /// Tauri 应用句柄
    app_handle: Arc<AppHandle>,
}

impl EventEmitter {
    /// 创建新的事件发射器
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle: Arc::new(app_handle),
        }
    }

    /// 发送网络同步事件
    pub fn emit_network_sync<T: Serialize + Clone>(
        &self,
        sync_type: &str,
        device_id: &str,
        data: T,
    ) -> Result<(), String> {
        let event_data = serde_json::json!({
            "sync_type": sync_type,
            "device_id": device_id,
            "data": data,
            "timestamp": chrono::Utc::now().timestamp(),
        });

        self.app_handle
            .emit_all("network-sync", event_data)
            .map_err(|e| format!("发送网络同步事件失败: {}", e))
    }

    /// 发送设备上线事件
    pub fn emit_device_online(&self, device_id: &str, device_name: &str) -> Result<(), String> {
        self.emit_network_sync(
            "device_online",
            device_id,
            serde_json::json!({
                "device_name": device_name,
            }),
        )
    }

    /// 发送设备下线事件
    pub fn emit_device_offline(&self, device_id: &str) -> Result<(), String> {
        self.emit_network_sync("device_offline", device_id, serde_json::json!({}))
    }

    /// 发送列表更新事件
    pub fn emit_list_update<T: Serialize + Clone>(
        &self,
        list_type: &str,
        operation: &str,
        item: T,
    ) -> Result<(), String> {
        let event_data = serde_json::json!({
            "list_type": list_type,
            "operation": operation,
            "item": item,
        });

        self.app_handle
            .emit_all("list-update", event_data)
            .map_err(|e| format!("发送列表更新事件失败: {}", e))
    }

    /// 发送新消息事件
    pub fn emit_new_message(
        &self,
        message_id: &str,
        from_device: &str,
        from_device_name: &str,
        content: &str,
        message_type: &str,
    ) -> Result<(), String> {
        let event_data = serde_json::json!({
            "message_id": message_id,
            "from_device": from_device,
            "from_device_name": from_device_name,
            "content": content,
            "message_type": message_type,
            "timestamp": chrono::Utc::now().timestamp(),
            "is_read": false,
        });

        self.app_handle
            .emit_all("new-message", event_data)
            .map_err(|e| format!("发送新消息事件失败: {}", e))
    }

    /// 发送新文件事件
    pub fn emit_new_file(
        &self,
        file_id: &str,
        file_name: &str,
        file_size: u64,
        from_device: &str,
        from_device_name: &str,
        file_type: &str,
    ) -> Result<(), String> {
        let event_data = serde_json::json!({
            "file_id": file_id,
            "file_name": file_name,
            "file_size": file_size,
            "from_device": from_device,
            "from_device_name": from_device_name,
            "file_type": file_type,
            "timestamp": chrono::Utc::now().timestamp(),
            "progress": 0,
        });

        self.app_handle
            .emit_all("new-file", event_data)
            .map_err(|e| format!("发送新文件事件失败: {}", e))
    }

    /// 发送文件传输进度事件
    pub fn emit_file_progress(
        &self,
        transfer_id: &str,
        file_name: &str,
        transferred_bytes: u64,
        total_bytes: u64,
        speed: u64,
    ) -> Result<(), String> {
        let progress = if total_bytes > 0 {
            ((transferred_bytes as f64 / total_bytes as f64) * 100.0) as u8
        } else {
            0
        };

        let event_data = serde_json::json!({
            "transfer_id": transfer_id,
            "file_name": file_name,
            "transferred_bytes": transferred_bytes,
            "total_bytes": total_bytes,
            "progress": progress,
            "speed": speed,
        });

        self.app_handle
            .emit_all("file-progress", event_data)
            .map_err(|e| format!("发送文件进度事件失败: {}", e))
    }

    /// 发送系统通知事件
    pub fn emit_system_notification(
        &self,
        notification_type: &str,
        title: &str,
        message: &str,
    ) -> Result<(), String> {
        let event_data = serde_json::json!({
            "notification_type": notification_type,
            "title": title,
            "message": message,
            "timestamp": chrono::Utc::now().timestamp(),
        });

        self.app_handle
            .emit_all("system-notification", event_data)
            .map_err(|e| format!("发送系统通知事件失败: {}", e))
    }

    /// 发送信息通知
    pub fn emit_info(&self, title: &str, message: &str) -> Result<(), String> {
        self.emit_system_notification("info", title, message)
    }

    /// 发送警告通知
    pub fn emit_warning(&self, title: &str, message: &str) -> Result<(), String> {
        self.emit_system_notification("warning", title, message)
    }

    /// 发送错误通知
    pub fn emit_error(&self, title: &str, message: &str) -> Result<(), String> {
        self.emit_system_notification("error", title, message)
    }

    /// 发送成功通知
    pub fn emit_success(&self, title: &str, message: &str) -> Result<(), String> {
        self.emit_system_notification("success", title, message)
    }
}
