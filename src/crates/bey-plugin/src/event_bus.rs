//! # 事件总线模块
//!
//! 提供事件分发和订阅功能

use dashmap::DashMap;
use serde::{Serialize, Deserialize};
use tracing::debug;

/// 事件优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventPriority {
    /// 低优先级
    Low = 0,
    /// 普通优先级
    Normal = 1,
    /// 高优先级
    High = 2,
    /// 紧急优先级
    Critical = 3,
}

/// 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件名称
    pub name: String,
    /// 事件数据
    pub data: Vec<u8>,
    /// 事件优先级
    pub priority: EventPriority,
    /// 事件时间戳
    pub timestamp: u64,
}

impl Event {
    /// 创建新事件
    pub fn new(name: String, data: Vec<u8>) -> Self {
        Self {
            name,
            data,
            priority: EventPriority::Normal,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
    
    /// 设置优先级
    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// 事件总线
///
/// 负责事件的订阅和分发
pub struct EventBus {
    /// 事件订阅表: 事件名 -> 订阅者列表
    subscriptions: DashMap<String, Vec<String>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
        }
    }
    
    /// 订阅事件
    ///
    /// # 参数
    ///
    /// * `event_name` - 事件名称
    /// * `plugin_name` - 插件名称
    pub async fn subscribe(&self, event_name: &str, plugin_name: &str) {
        self.subscriptions
            .entry(event_name.to_string())
            .or_insert_with(Vec::new)
            .push(plugin_name.to_string());
        
        debug!("插件 {} 订阅事件: {}", plugin_name, event_name);
    }
    
    /// 取消订阅事件
    ///
    /// # 参数
    ///
    /// * `event_name` - 事件名称
    /// * `plugin_name` - 插件名称
    pub async fn unsubscribe(&self, event_name: &str, plugin_name: &str) {
        if let Some(mut subscribers) = self.subscriptions.get_mut(event_name) {
            subscribers.retain(|name| name != plugin_name);
            debug!("插件 {} 取消订阅事件: {}", plugin_name, event_name);
        }
    }
    
    /// 获取事件的订阅者列表
    ///
    /// # 参数
    ///
    /// * `event_name` - 事件名称
    ///
    /// # 返回值
    ///
    /// 返回订阅者名称列表
    pub async fn get_subscribers(&self, event_name: &str) -> Vec<String> {
        self.subscriptions
            .get(event_name)
            .map(|subscribers| subscribers.clone())
            .unwrap_or_default()
    }
    
    /// 清除所有订阅
    pub fn clear(&self) {
        self.subscriptions.clear();
    }
    
    /// 获取订阅的事件数量
    pub fn event_count(&self) -> usize {
        self.subscriptions.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let bus = EventBus::new();
        
        bus.subscribe("test.event", "plugin1").await;
        bus.subscribe("test.event", "plugin2").await;
        
        let subscribers = bus.get_subscribers("test.event").await;
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&"plugin1".to_string()));
        assert!(subscribers.contains(&"plugin2".to_string()));
    }
    
    #[tokio::test]
    async fn test_event_bus_unsubscribe() {
        let bus = EventBus::new();
        
        bus.subscribe("test.event", "plugin1").await;
        bus.subscribe("test.event", "plugin2").await;
        bus.unsubscribe("test.event", "plugin1").await;
        
        let subscribers = bus.get_subscribers("test.event").await;
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&"plugin2".to_string()));
    }
    
    #[test]
    fn test_event_creation() {
        let event = Event::new("test".to_string(), vec![1, 2, 3])
            .with_priority(EventPriority::High);
        
        assert_eq!(event.name, "test");
        assert_eq!(event.data, vec![1, 2, 3]);
        assert_eq!(event.priority, EventPriority::High);
    }
}
